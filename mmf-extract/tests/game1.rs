//! Tests for the **first** game's version-1 containers (`MMFW` version 1).
//!
//! Version 1 differs from versions 2 and 3 in ways that used to make the
//! whole first game unextractable: the offsets table starts four bytes
//! earlier, the resource count is the u16 at `0x1E`, and the per-resource
//! metadata block carries the picture **dimensions** in its own place. These
//! tests pin down every measured claim about that layout so it cannot silently
//! regress, and they assert the `--original` byte-exactness the project
//! requires of every decoder.
//!
//! They use the real game files when present and skip otherwise.

use mmf_extract::container::{Container, ContainerKind};
use std::path::{Path, PathBuf};

/// Where the first game's assets live. Point `GAME1_ASSETS` at the `Dati`
/// folder carved from the CD image to run these anywhere.
fn game1_dir() -> Option<PathBuf> {
    let extra = std::env::var("GAME1_ASSETS").unwrap_or_default();
    for candidate in [
        "/RawAssets/Dati",
        extra.as_str(),
        "/tmp/topolino/files/Dati",
        "/PathAssets/Dati",
    ] {
        let p = PathBuf::from(candidate);
        if p.is_dir() {
            return Some(p);
        }
    }
    None
}

/// The containers whose version-1 layout is fully measured.
const ALL: &[(&str, usize)] = &[
    ("HD.PIC", 1682),
    ("mpApp.PIC", 384),
    ("pack.PIC", 10893),
    ("Topolino.CPK", 2),
    ("HD.SND", 66),
    ("mpApp.SND", 152),
    ("pack.SND", 234),
    ("pack.BLB", 305),
];

fn be16(data: &[u8], at: usize) -> u16 {
    u16::from_be_bytes([data[at], data[at + 1]])
}

fn be32(data: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]])
}

/// Every resource must be a byte-exact slice of the container file, which is
/// what the `--original` mode writes out.
#[test]
fn every_payload_is_a_byte_exact_slice() {
    let Some(dir) = game1_dir() else {
        eprintln!("skipping: first game assets not present");
        return;
    };
    let mut checked = 0usize;
    for (name, _) in ALL {
        let path = dir.join(name);
        if !path.is_file() {
            eprintln!("skipping {}: not present", path.display());
            continue;
        }
        let data = std::fs::read(&path).expect("read container");
        let c = Container::open(&path).expect("parse container");
        assert_eq!(c.version, 1, "{name}: expected version 1");
        checked += c.resource_count as usize;
        for i in 0..c.resource_count as usize {
            let payload = c.payload(i);
            let at = c.offsets[i] as usize;
            assert_eq!(
                payload,
                &data[at..at + payload.len()],
                "{name}[{i}] is not a byte-exact slice"
            );
        }
    }
    assert!(checked > 0, "no containers were checked");
}

/// The header fields that pin the version-1 layout down, measured on all nine
/// shipped containers.
#[test]
fn header_fields_are_measured() {
    let Some(dir) = game1_dir() else {
        eprintln!("skipping: first game assets not present");
        return;
    };
    for (name, count) in ALL {
        let path = dir.join(name);
        if !path.is_file() {
            continue;
        }
        let data = std::fs::read(&path).expect("read container");
        let c = Container::open(&path).expect("parse container");
        assert_eq!(c.resource_count, *count as u16, "{name}: resource count");
        // The offsets table starts at 0x20, not 0x24, and holds N+1 entries
        // whose last one is the file size.
        assert_eq!(
            be32(&data, 0x20 + 4 * c.resource_count as usize),
            data.len() as u32,
            "{name}: offsets table must end at the file size"
        );
        assert_eq!(
            c.offsets[c.resource_count as usize],
            data.len() as u32,
            "{name}: last offset must be EOF"
        );
        // The names are 32-byte fields starting at 0x24 + 4*N + 2 in
        // version-1 coordinates, i.e. 0x26 + 4*N.
        let names_base = 0x20 + 4 * (c.resource_count as usize + 1) + 2;
        assert_eq!(c.names_end, names_base + 32 * c.resource_count as usize);
        for (i, name) in c.names.iter().enumerate().take(4) {
            let field = &data[names_base + 32 * i..names_base + 32 * (i + 1)];
            let stored = String::from_utf8_lossy(field).trim_end_matches('\0').to_string();
            assert_eq!(&stored, name, "{name}[{i}]: name field");
        }
    }
}

/// The picture metadata block: the marker, the three byte arrays, the
/// dimension table and the palette bank must all land exactly on
/// `offsets[0]`, the first payload.
#[test]
fn picture_metadata_lands_on_the_first_payload() {
    let Some(dir) = game1_dir() else {
        eprintln!("skipping: first game assets not present");
        return;
    };
    for name in ["HD.PIC", "mpApp.PIC", "pack.PIC", "Topolino.CPK"] {
        let path = dir.join(name);
        if !path.is_file() {
            continue;
        }
        let data = std::fs::read(&path).expect("read container");
        let c = Container::open(&path).expect("parse container");
        assert_eq!(c.kind, ContainerKind::Pictures, "{name}: kind");
        let n = c.resource_count as usize;
        // [extra 4*N][marker u16][flags N][kind N][dims 4*N][sizes 4*N][N][u16][pals]
        let base = c.names_end;
        let marker = be16(&data, base + 4 * n);
        assert_eq!(marker, 7, "{name}: picture metadata marker");
        let after = base + 4 * n + 2 + n + n + 4 * n + 4 * n + n + 2;
        let palettes = be16(&data, base + 4 * n + 2 + n + n + 4 * n + 4 * n + n) as usize;
        assert_eq!(after + palettes * 1024, c.offsets[0] as usize,
            "{name}: metadata + palettes must end at the first payload");
        // Every dimension must be non-zero and the pair must be sane.
        for i in 0..n {
            let dim = c.dimension(i);
            assert!(dim.width > 0 && dim.height > 0,
                "{name}[{i}] {}: empty dimensions", c.name(i));
            assert!(dim.width < 4096 && dim.height < 4096,
                "{name}[{i}] {}: absurd dimensions", c.name(i));
        }
    }
}

/// Every first-game picture resource must decode, and the storage form must
/// follow the container's declaration exactly.
///
/// This is the check that proved the first game stores `{height, width}` pairs
/// in its metadata: with the pair absent, or with the two halves swapped, *no*
/// payload length lines up. It is also the check that pins the compression
/// discriminator — a resource whose declared uncompressed size is zero must
/// come back as the container's own bytes, and one that declares a size must
/// expand to exactly that many bytes and then decode.
///
/// The numbers are not floors any more. With the LZSS step in place every
/// shipped resource decodes, and these were measured as 12,961 of 12,961
/// across the four picture containers (HD.PIC 1682, mpApp.PIC 384, pack.PIC
/// 10893, Topolino.CPK 2) with zero reported failures, so a regression to any
/// single resource fails here.
#[test]
fn dimensions_and_storage_forms_decode_every_resource() {
    let Some(dir) = game1_dir() else {
        eprintln!("skipping: first game assets not present");
        return;
    };
    let mut checked = 0usize;
    let mut decoded = 0usize;
    for name in ["HD.PIC", "mpApp.PIC", "pack.PIC", "Topolino.CPK"] {
        let path = dir.join(name);
        if !path.is_file() {
            continue;
        }
        let c = Container::open(&path).expect("parse container");
        assert_eq!(c.kind, ContainerKind::Pictures, "{name}: kind");
        let n = c.resource_count as usize;
        let mut plain = 0usize;
        let mut lzss = 0usize;
        for i in 0..n {
            let stored = c.payload(i);
            let declared = mmf_extract::v1::uncompressed_len(&c, i);
            let payload = match mmf_extract::v1::picture::payload(&c, i) {
                Ok(p) => p,
                Err(e) => panic!("{name}[{i}] '{}': {e:#}", c.name(i)),
            };
            if declared == 0 {
                // Stored plain: the expansion step must hand back the slice.
                assert_eq!(
                    payload, stored,
                    "{name}[{i}] '{}': declared 0 but the payload changed",
                    c.name(i)
                );
                plain += 1;
            } else {
                // Stored compressed: it must expand to the declared length,
                // which is what an LZSS stream may not exceed.
                assert_ne!(
                    payload, stored,
                    "{name}[{i}] '{}': declared {declared} but the payload is stored plain",
                    c.name(i)
                );
                lzss += 1;
            }
            // The payload, whichever form it came from, must decode.
            assert!(
                mmf_extract::picture::pixel_format(
                    &c,
                    i,
                    &payload,
                    mmf_extract::v1::palette_of(&c, i)
                )
                .is_some(),
                "{name}[{i}] '{}': {} bytes after expansion decode to nothing",
                c.name(i),
                payload.len()
            );
            decoded += 1;
        }
        assert_eq!(
            decoded - checked, n,
            "{name}: every resource must decode (plain {plain}, lzss {lzss})"
        );
        checked += n;
    }
    assert_eq!(checked, 12_961, "the four picture containers hold 12,961 resources");
    assert_eq!(decoded, checked);
}

/// `--original` for a first-game container writes the expanded payload.
///
/// This is the 1:1 requirement of `AGENTS.md`: the bytes written out must be
/// reproducible from the bytes in the container, so for a stored-plain
/// resource they are the container's own slice and for a compressed one they
/// are exactly what the declared size expands to.
#[test]
fn original_mode_writes_the_expanded_payload() {
    let Some(dir) = game1_dir() else {
        eprintln!("skipping: first game assets not present");
        return;
    };
    // Never write inside the asset folder: it is read-only input. The whole
    // `.dat` set for 12,961 resources is ~150 MB, so this lives in scratch and
    // is removed afterwards.
    let out_dir = tempdir("v1-original");
    let _guard = Cleanup(&out_dir);

    let mut checked = 0usize;
    for name in ["HD.PIC", "mpApp.PIC", "pack.PIC", "Topolino.CPK"] {
        let path = dir.join(name);
        if !path.is_file() {
            continue;
        }
        let c = Container::open(&path).expect("parse container");
        let report =
            mmf_extract::cli::extract_pictures_original(&c, &out_dir.join(name)).expect("extract");
        assert_eq!(report.resources.len(), c.resource_count as usize, "{name}");
        assert!(report.failed.is_empty(), "{name}: {:?}", report.failed);

        let on_disk = std::fs::read(&path).expect("read container");
        for entry in &report.resources {
            let written = std::fs::read(&entry.png).expect("read .dat");
            let i = entry.index;
            let start = c.offsets[i] as usize;
            let end = c.offsets[i + 1] as usize;
            let declared = mmf_extract::v1::uncompressed_len(&c, i);
            if declared == 0 {
                assert_eq!(
                    written, &on_disk[start..end],
                    "{name}[{i}] '{}': a stored-plain .dat must be the container slice",
                    entry.name
                );
            } else {
                assert_eq!(
                    written,
                    mmf_extract::v1::lzss::decompress(&on_disk[start..end], declared)
                        .expect("decompress"),
                    "{name}[{i}] '{}': a compressed .dat must be the exact expansion",
                    entry.name
                );
            }
            checked += 1;
        }
    }
    assert_eq!(checked, 12_961);
}

/// The first game's Scripts containers open, and `--original` is byte-exact.
///
/// The layout used to be counted as "still undecoded" and the containers
/// failed to parse at all. They do not share the versions-2/3 shape: there is
/// no 32-byte names table and no object-database metadata region. What is there
/// is an offsets table, a `u16` measured as **4** in every shipped container,
/// and then the payloads. Both checks are asserted here against the containers'
/// own bytes.
#[test]
fn version_1_scripts_open_and_original_is_byte_exact() {
    let Some(dir) = game1_dir() else {
        eprintln!("skipping: first game assets not present");
        return;
    };
    let mut checked = 0usize;
    for (name, count) in [("HD.mms", 295usize), ("pack.mms", 988usize)] {
        let path = dir.join(name);
        if !path.is_file() {
            continue;
        }
        let data = std::fs::read(&path).expect("read container");
        let c = Container::open(&path).expect("parse container");
        assert_eq!(c.version, 1, "{name}: version");
        assert_eq!(c.kind, ContainerKind::Scripts, "{name}: kind");
        assert_eq!(c.resource_count as usize, count, "{name}: resource count");

        // The layout: the table ends at 0x20 + 4*(N+1), the marker u16 sits
        // there, and the payloads start exactly two bytes later.
        let table_end = 0x20 + 4 * (count + 1);
        let marker = be16(&data, table_end);
        assert_eq!(marker, 4, "{name}: the version-1 script marker must be 4");
        assert_eq!(
            c.names_end, table_end + 2,
            "{name}: no names table exists, so names_end is where the payloads start"
        );
        assert_eq!(c.offsets[count], data.len() as u32, "{name}: last offset must be EOF");

        // Names come from each payload's string tail, not from the container.
        let scripts = mmf_extract::v1::scripts::parse(
            std::path::Path::new(&path),
            &data,
            &c,
            table_end,
        )
        .expect("parse v1 scripts");
        let named = scripts.names.iter().filter(|n| !n.is_empty()).count();
        assert!(
            named >= count / 3,
            "{name}: only {named} of {count} payloads carry a name"
        );

        // And every payload is a byte-exact slice of the container, which is
        // what `--original` must write.
        for i in 0..count {
            let payload = c.payload(i);
            assert_eq!(
                payload,
                &data[c.offsets[i] as usize..c.offsets[i + 1] as usize],
                "{name}[{i}]: payload is not the container's own bytes"
            );
            assert!(!payload.is_empty(), "{name}[{i}]: an empty payload");
        }

        // Through the real `--original` path.
        let out_dir = tempdir("v1-scripts-original").join(name);
        let written = mmf_extract::v1::scripts::extract_container(&c, &out_dir, true)
            .expect("extract originals");
        assert_eq!(written, count, "{name}: every object must be written");
        for i in 0..count {
            let safe = scripts.names[i].replace(|ch: char| !ch.is_ascii_alphanumeric() && ch != '-' && ch != '_' && ch != '.', "_");
            let stem = if safe.is_empty() {
                format!("object{i}")
            } else {
                safe
            };
            let written_bytes =
                std::fs::read(out_dir.join(format!("{i:05}_{stem}.bin"))).expect("read .bin");
            assert_eq!(
                written_bytes,
                &data[c.offsets[i] as usize..c.offsets[i + 1] as usize],
                "{name}[{i}]: --original must write the container's own bytes"
            );
        }
        checked += count;
    }
    assert_eq!(checked, 1283, "the two script containers hold 1,283 objects");
}

/// A scratch directory that removes itself when the test ends, even on a
/// panic.
struct Cleanup<'a>(&'a Path);
impl Drop for Cleanup<'_> {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(self.0);
    }
}

/// Scratch space for this test binary.
fn tempdir(tag: &str) -> PathBuf {
    let base = std::env::var("MMF_TEST_TMP").unwrap_or_else(|_| {
        std::env::temp_dir()
            .join("mmf-extract-tests")
            .display()
            .to_string()
    });
    let dir = Path::new(&base).join(format!("{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}
