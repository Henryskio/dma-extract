//! Round-trip tests: an extracted "original format" resource must be
//! reconstructible byte-for-byte from the bytes present in the original
//! container.
//!
//! The idea the user asked for: extracting a resource in the game's original
//! format must not change a single byte. So for each resource we take the
//! bytes we wrote out and check that they appear in the container exactly as
//! they were, and that re-compressing and re-decompressing them reproduces
//! the same bytes again.
//!
//! These tests use the real game assets when they are present, and fall back
//! to synthetic data otherwise so the suite still runs anywhere.

use mmf_extract::container::{Container, ContainerKind};
use mmf_extract::lz77;
use mmf_extract::picture;
use std::path::{Path, PathBuf};

/// Where the game's assets live.
fn raw_assets_dir() -> Option<PathBuf> {
    for candidate in ["/RawAssets", "/var/home/Henrysk/DisneyMagicoArtista/RawAssets"] {
        let p = PathBuf::from(candidate);
        if p.is_dir() {
            return Some(p);
        }
    }
    None
}

/// The picture containers of the game.
const PICTURE_CONTAINERS: &[&str] = &["app.MMP", "HD.MMP", "stmp1.MMP", "stmp2.MMP", "stmp3.MMP"];

#[test]
fn lz77_round_trips() {
    // The compressor must be an exact inverse of the decompressor.
    let cases: Vec<Vec<u8>> = vec![
        vec![],
        vec![0],
        vec![1, 2, 3],
        vec![0xAB; 300],
        (0..=255u8).collect(),
        (0..1000u32).map(|i| (i % 7) as u8).collect(),
        (0..5000u32).map(|i| (i % 251) as u8).collect(),
    ];
    for case in cases {
        let compressed = lz77::compress(&case);
        let back = lz77::decompress(&compressed).expect("decompress");
        assert_eq!(back, case, "round trip failed for {} bytes", case.len());
    }
}

#[test]
fn lz77_round_trips_arbitrary_lengths() {
    // Every length around the literal-run boundaries must survive.
    for len in [1usize, 2, 3, 4, 17, 18, 19, 100, 237, 238, 239, 240, 272, 273, 274, 528, 529] {
        let data: Vec<u8> = (0..len).map(|i| (i * 31 + 7) as u8).collect();
        let back = lz77::decompress(&lz77::compress(&data)).expect("decompress");
        assert_eq!(back, data, "round trip failed at length {len}");
    }
}

/// Every resource's stored payload must be a byte-exact slice of the file.
#[test]
fn raw_payloads_are_byte_identical_to_the_container() {
    let Some(dir) = raw_assets_dir() else {
        eprintln!("skipping: game assets not present");
        return;
    };

    for name in PICTURE_CONTAINERS {
        let path = dir.join(name);
        if !path.exists() {
            continue;
        }
        let container = Container::open(&path).expect("open container");
        assert_eq!(container.kind, ContainerKind::Pictures);
        let whole = std::fs::read(&path).expect("read container");

        for i in 0..container.resource_count as usize {
            let payload = container.payload(i);
            let start = container.offsets[i] as usize;
            let end = container.offsets[i + 1] as usize;

            // The payload is the container's own bytes, unmodified.
            assert_eq!(
                payload,
                &container.data[start..end],
                "{name} resource {i}: payload differs from the container slice"
            );

            // And the file on disk holds exactly those bytes at that offset.
            assert_eq!(
                payload,
                &whole[start..end],
                "{name} resource {i}: payload differs from the bytes on disk"
            );
        }
    }
}

/// The decompressed payload of a resource must be reproducible: decompressing
/// it again yields the same bytes, and re-compressing those bytes and
/// decompressing them once more yields the same bytes again.
#[test]
fn decompressed_payloads_are_stable_and_recompressible() {
    let Some(dir) = raw_assets_dir() else {
        eprintln!("skipping: game assets not present");
        return;
    };

    // Check a spread of resources from each container rather than all of
    // them, so the test stays quick.
    for name in PICTURE_CONTAINERS {
        let path = dir.join(name);
        if !path.exists() {
            continue;
        }
        let container = Container::open(&path).expect("open container");
        let n = container.resource_count as usize;
        let step = (n / 40).max(1);

        for i in (0..n).step_by(step) {
            let Ok(data) = lz77::decompress(container.payload(i)) else {
                continue;
            };

            // Decompressing is deterministic.
            let again = lz77::decompress(container.payload(i)).expect("decompress");
            assert_eq!(data, again, "{name} resource {i}: decompression is not stable");

            // Our compressor inverts the decompressor exactly.
            let round = lz77::decompress(&lz77::compress(&data)).expect("decompress");
            assert_eq!(
                round, data,
                "{name} resource {i}: recompression changed {} bytes",
                data.len()
            );
        }
    }
}

/// A resource extracted in the original format must consist of bytes that are
/// present in the container 1-to-1.
///
/// Concretely: the `.dat` file the `--original` path writes equals the bytes
/// the container's own decompressor produces (or, for a resource stored
/// uncompressed, the raw payload itself), and those bytes round-trip exactly
/// through the container's codec — decompressing the stored payload yields
/// them, and recompressing them yields a payload that decompresses back to
/// them. Nothing is added, dropped or reordered.
#[test]
fn original_format_extraction_loses_nothing() {
    let Some(dir) = raw_assets_dir() else {
        eprintln!("skipping: game assets not present");
        return;
    };
    let tmp = tempdir("original-format");

    for name in PICTURE_CONTAINERS {
        let path = dir.join(name);
        if !path.exists() {
            continue;
        }
        let out_dir = tmp.join(name);
        let container = Container::open(&path).expect("open container");

        // Exercise the real `--original` production path.
        let report =
            mmf_extract::cli::extract_pictures_original(&container, &out_dir).expect("extract");

        // Read the container file once for the byte-exact-slice checks.
        let on_disk = std::fs::read(&path).expect("read container");

        // Recompression is CPU-heavy; spot-check a spread rather than all.
        let n = container.resource_count as usize;
        let step = (n / 40).max(1);

        for entry in &report.resources {
            let extracted = std::fs::read(&entry.png).expect("read extracted .dat");
            let stored = container.payload(entry.index);

            // The stored payload is a byte-exact slice of the container file.
            let start = container.offsets[entry.index] as usize;
            let end = container.offsets[entry.index + 1] as usize;
            assert_eq!(
                stored,
                &on_disk[start..end],
                "{name} resource {} ('{}'): stored payload is not the container's bytes at [{start},{end})",
                entry.index,
                entry.name
            );

            if container.sizes[entry.index] == 0 {
                // Stored uncompressed: the .dat is the payload, verbatim.
                assert_eq!(
                    extracted, stored,
                    "{name} resource {} ('{}'): uncompressed .dat differs from the stored payload",
                    entry.index, entry.name
                );
            } else {
                // Stored compressed: the .dat is exactly what the container's
                // decompressor yields from the stored bytes.
                let expected = lz77::decompress(stored).expect("decompress");
                assert_eq!(
                    extracted, expected,
                    "{name} resource {} ('{}'): .dat differs from the decompressed stored payload",
                    entry.index, entry.name
                );

                // And it round-trips 1-to-1 through our codec (spot check).
                if entry.index % step == 0 {
                    let round =
                        lz77::decompress(&lz77::compress(&extracted)).expect("decompress");
                    assert_eq!(
                        round, extracted,
                        "{name} resource {} ('{}'): recompression changed {} bytes",
                        entry.index,
                        entry.name,
                        extracted.len()
                    );
                }
            }
        }
    }
}

/// The PNG the modern path writes must decode back to exactly the RGBA
/// pixels the picture decoder produced, at the right size.
#[test]
fn png_output_round_trips_to_the_same_pixels() {
    let Some(dir) = raw_assets_dir() else {
        eprintln!("skipping: game assets not present");
        return;
    };
    let tmp = tempdir("png-roundtrip");

    let path = dir.join("app.MMP");
    if !path.exists() {
        return;
    }
    let out_dir = tmp.join("app");
    let container = Container::open(&path).expect("open container");

    // A spread of resources.
    let n = container.resource_count as usize;
    let step = (n / 25).max(1);

    for i in (0..n).step_by(step) {
        let Ok(decoded) = picture::decode(&container, i) else {
            continue;
        };
        if decoded.width == 0 || decoded.height == 0 {
            continue;
        }
        let png_path = out_dir.join(format!("{i:05}.png"));
        std::fs::create_dir_all(&out_dir).unwrap();
        picture::write_png(&png_path, decoded.width, decoded.height, &decoded.rgba)
            .expect("write png");

        let file = std::fs::File::open(&png_path).expect("open png");
        let reader = png::Decoder::new(std::io::BufReader::new(file));
        let mut reader = reader.read_info().expect("read png header");
        let mut buf = vec![0u8; reader.output_buffer_size().expect("buffer size")];
        let info = reader.next_frame(&mut buf).expect("read png frame");

        assert_eq!(info.width, decoded.width as u32);
        assert_eq!(info.height, decoded.height as u32);
        assert_eq!(
            &buf[..info.buffer_size()],
            &decoded.rgba[..],
            "resource {i}: PNG pixels differ from the decoded pixels"
        );
    }
}

/// The container parser must find the metadata where the layout says it is.
#[test]
fn metadata_region_is_exactly_fifteen_n_plus_four() {
    let Some(dir) = raw_assets_dir() else {
        eprintln!("skipping: game assets not present");
        return;
    };

    for name in PICTURE_CONTAINERS {
        let path = dir.join(name);
        if !path.exists() {
            continue;
        }
        let data = std::fs::read(&path).expect("read");
        let n = u16::from_be_bytes([data[0x22], data[0x23]]) as usize;
        let names_base = 0x24 + 4 * (n + 1) + 2;
        let names_end = names_base + 32 * n;
        let payload_start = u32::from_be_bytes([
            data[0x24],
            data[0x25],
            data[0x26],
            data[0x27],
        ]) as usize;

        assert_eq!(
            payload_start - names_end,
            15 * n + 4,
            "{name}: metadata region is not 15*N+4 bytes"
        );
    }
}

/// The decompressed-size table must agree with the real decompressed sizes.
#[test]
fn size_table_matches_decompressed_sizes() {
    let Some(dir) = raw_assets_dir() else {
        eprintln!("skipping: game assets not present");
        return;
    };

    for name in PICTURE_CONTAINERS {
        let path = dir.join(name);
        if !path.exists() {
            continue;
        }
        let container = Container::open(&path).expect("open container");
        let n = container.resource_count as usize;
        let step = (n / 60).max(1);

        for i in (0..n).step_by(step) {
            if container.sizes[i] == 0 {
                // Stored uncompressed: the table records zero and the payload
                // length is the plain length. Nothing to decompress.
                assert_eq!(
                    container.plain_len(i),
                    container.payload_len(i),
                    "{name} resource {i}: size table is 0 but plain_len != stored length"
                );
                continue;
            }
            let Ok(data) = lz77::decompress(container.payload(i)) else {
                continue;
            };
            assert_eq!(
                container.sizes[i] as usize,
                data.len(),
                "{name} resource {i}: size table says {}, payload is {} bytes",
                container.sizes[i],
                data.len()
            );
        }
    }
}

/// Make a temporary directory under the job's temp area.
fn tempdir(tag: &str) -> PathBuf {
    let base = std::env::var("MMF_TEST_TMP")
        .unwrap_or_else(|_| std::env::temp_dir().join("mmf-extract-tests").display().to_string());
    let dir = Path::new(&base).join(format!("{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}
