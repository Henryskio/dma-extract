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

/// The dimensions decoded from the metadata must be consistent with the
/// payload length for every resource stored in one of the raw layouts, and
/// the real decoder must consume at least the measured number of resources.
///
/// This is the check that proved the first game stores `{height, width}`
/// pairs in its metadata: with the pair absent, or with the two halves
/// swapped, *no* payload length lines up. The floors are the counts measured
/// on the shipped files; the sub-formats still undecoded keep `mpApp.PIC` and
/// `pack.PIC` from reaching 100%, which is why these are floors and not equalities.
#[test]
fn dimensions_agree_with_the_payload_lengths() {
    let Some(dir) = game1_dir() else {
        eprintln!("skipping: first game assets not present");
        return;
    };
    // container -> (minimum raw-length matches, minimum decoded resources)
    let floors: &[(&str, usize, usize)] = &[
        ("HD.PIC", 1000, 1682),
        ("mpApp.PIC", 200, 250),
        ("pack.PIC", 100, 5100),
        ("Topolino.CPK", 2, 2),
    ];
    for (name, raw_floor, decoded_floor) in floors {
        let path = dir.join(name);
        if !path.is_file() {
            continue;
        }
        let c = Container::open(&path).expect("parse container");
        let n = c.resource_count as usize;
        let mut raw = 0usize;
        let mut decoded = 0usize;
        for i in 0..n {
            // Version 1 stores every payload uncompressed.
            let data = c.payload(i);
            let w = c.dimension(i).width as usize;
            let h = c.dimension(i).height as usize;
            let stride = (w + 3) / 4 * 4;
            // Every raw layout, including the padded 32bpp variant.
            let candidates = [
                1024 + stride * h,
                stride * h,
                1024 + w * h,
                w * h,
                1024 + 4 * stride * h,
                4 * w * h,
                4 * stride * h,
            ];
            if candidates.contains(&data.len()) {
                raw += 1;
            }
            if mmf_extract::picture::pixel_format(&c, i, data).is_some() {
                decoded += 1;
            }
        }
        assert!(
            raw >= *raw_floor,
            "{name}: only {raw} of {n} payload lengths agree with the dimensions (floor {raw_floor})"
        );
        assert!(
            decoded >= *decoded_floor,
            "{name}: the decoder only consumes {decoded} of {n} resources (floor {decoded_floor})"
        );
    }
}
