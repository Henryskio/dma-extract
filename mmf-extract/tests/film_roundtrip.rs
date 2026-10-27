//! Round-trip tests for Films containers.
//!
//! The important promise is the same as for pictures: `--original` must
//! write the game's own bytes without changing a single one.  For Films the
//! container's `sizes` table is *not* a decompressed-size table, so the
//! correct expectation is the stored payload itself, which is already the
//! complete inner `.flm` stream.
//!
//! The modern path is checked against the decoded frames: every written PNG
//! must decode back to exactly the RGBA pixels obtained from the palette and
//! the frame's 8bpp indices, and the timeline must contain the measured
//! frame count, duplicates and timestamps.

use mmf_extract::container::{Container, ContainerKind};
use mmf_extract::film;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

fn raw_assets_dir() -> Option<PathBuf> {
    // The asset folder is normally mounted at /RawAssets; point DMA_ASSETS at
    // another copy if needed.
    let extra = std::env::var("DMA_ASSETS").unwrap_or_default();
    for candidate in ["/RawAssets", extra.as_str()] {
        let p = PathBuf::from(candidate);
        if p.is_dir() {
            return Some(p);
        }
    }
    None
}

fn film_container() -> Option<(PathBuf, Container)> {
    let dir = raw_assets_dir()?;
    let path = dir.join("DMA.MMF");
    if !path.exists() {
        return None;
    }
    let container = Container::open(&path).expect("open DMA.MMF");
    assert_eq!(container.kind, ContainerKind::Films);
    assert_eq!(container.resource_count, 1);
    Some((path, container))
}

/// `--original` writes the container payload verbatim, byte for byte.
#[test]
fn original_film_is_the_verbatim_inner_flm() {
    let Some((path, container)) = film_container() else {
        eprintln!("skipping: game assets not present");
        return;
    };
    let tmp = tempdir("film-original");
    let out_dir = tmp.join("out");

    let count = film::extract_container(&container, &out_dir, true).expect("extract original");
    assert_eq!(count, 1, "one film resource");

    let extracted_path = out_dir.join("00000_Intro.flm");
    let extracted = std::fs::read(&extracted_path).expect("read extracted .flm");
    let payload = container.payload(0);

    // The payload is the inner .flm stream, uncompressed.
    assert_eq!(payload.len(), 6_856_628);
    assert_eq!(&payload[..2], &[0xF7, 0x89]);
    assert_eq!(
        extracted, payload,
        "original extraction differs from the container payload"
    );

    // It is also a byte-exact slice of the file on disk.
    let on_disk = std::fs::read(&path).expect("read DMA.MMF");
    let start = container.offsets[0] as usize;
    let end = container.offsets[1] as usize;
    assert_eq!(
        extracted,
        &on_disk[start..end],
        "original extraction differs from the bytes on disk"
    );

    // The generic films size table is bogus and must not be used as a
    // decompressed length.  (This is the trap the old stub fell into.)
    assert_eq!(container.sizes[0], 26_870_306);
    assert_ne!(container.sizes[0] as usize, payload.len());
    assert_ne!(container.plain_len(0), payload.len());

    // The written file parses back as the complete inner stream.
    let film = film::decode_film(&container, 0).expect("decode film");
    assert_eq!(film.chunks.len(), 412);
    assert_eq!(film.chunks[0].kind, film::TYPE_FULL_FRAME);
    assert_eq!(film.chunks.last().unwrap().kind, film::TYPE_END);
    assert_eq!(film.chunks.last().unwrap().offset + 8, payload.len());

    assert!(
        !out_dir.read_dir().unwrap().any(|e| e
            .unwrap()
            .path()
            .extension()
            .is_some_and(|x| x == "png")),
        "original mode must not write PNGs"
    );
}

/// The modern path writes one PNG per timeline frame and a manifest, and
/// every PNG decodes back to the exact palette-expanded pixels.
#[test]
fn modern_film_writes_lossless_pngs_and_timeline() {
    let Some((_path, container)) = film_container() else {
        eprintln!("skipping: game assets not present");
        return;
    };
    let tmp = tempdir("film-modern");
    let out_dir = tmp.join("out");

    let count = film::extract_container(&container, &out_dir, false).expect("extract modern");
    assert_eq!(count, 1, "one film resource");

    let film = film::decode_film(&container, 0).expect("decode film");
    assert_eq!(film.width, 546);
    assert_eq!(film.height, 410);
    assert_eq!(film.stride, 548);
    assert_eq!(film.bounds, [0, 0, 410, 546]);
    assert_eq!(film.palette_count, 1);
    assert_eq!(&film.palette[..4], &[255, 0, 0, 0], "transparent black");
    assert_eq!(
        &film.record[..10],
        &[0x19, 0x02, 0, 0, 0, 0, 0x01, 0x9a, 0x02, 0x22]
    );

    assert_eq!(film.frames.len(), 139);
    assert_eq!(film.unique_frames, 133);
    assert_eq!(film.frames[0].start_tick, 0);
    assert_eq!(film.frames[1].start_tick, 83);
    assert_eq!(film.frames[138].start_tick, 11_454);
    assert_eq!(film.duration_ticks(), 11_537);
    assert!(film.frames.iter().all(|f| f.duration_tick == 83));

    let duplicates: Vec<usize> = film
        .frames
        .iter()
        .enumerate()
        .filter(|(_, f)| f.duplicate)
        .map(|(i, _)| i)
        .collect();
    assert_eq!(duplicates, vec![6, 51, 52, 53, 137, 138]);

    // A few golden hashes pin the actual pixel decoding, not just its shape.
    assert_eq!(
        sha256_hex(&film.frames[0].indices),
        "153192a1d8a2f4ea0ba3285ba29531a687e5749b4dfd238e4891d63dff64c8c8"
    );
    assert_eq!(
        sha256_hex(&film.frames[138].indices),
        "661f73dda583c23eef4ecd3398bb32987534d1fe0487247d9a23c6002127f202"
    );

    assert_eq!(film.audio.len(), 139);
    assert_eq!(film.audio.iter().filter(|a| a.sync).count(), 12);
    assert_eq!(film.audio.iter().filter(|a| !a.sync).count(), 127);
    assert_eq!(film.chunk_kinds[&film::TYPE_AUDIO], 139);
    assert_eq!(film.chunk_kinds[&film::TYPE_TIMESTAMP], 138);
    assert_eq!(film.chunk_kinds[&film::TYPE_DELTA], 132);
    assert_eq!(film.chunk_kinds[&film::TYPE_FULL_FRAME], 1);

    let film_dir = out_dir.join("00000_Intro");
    let png_count = film_dir
        .read_dir()
        .expect("read film dir")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "png"))
        .count();
    assert_eq!(png_count, 139, "one PNG per frame");

    let manifest =
        std::fs::read_to_string(film_dir.join("manifest.json")).expect("read manifest.json");
    for needle in [
        "\"frame_count\": 139",
        "\"unique_frame_count\": 133",
        "\"duration_ms\": 11537",
        "\"chunk_count\": 412",
        "\"chunk_count\": 139",
        "\"sync_blocks\": 12",
        "\"duplicate\": true",
    ] {
        assert!(manifest.contains(needle), "manifest lacks {needle}");
    }
    assert_eq!(
        manifest.matches("\"duplicate\": true").count(),
        6,
        "manifest duplicate flags"
    );

    for (i, frame) in film.frames.iter().enumerate() {
        let path = film_dir.join(format!("frame_{i:03}.png"));
        let file = std::fs::File::open(&path).expect("open frame png");
        let mut reader = png::Decoder::new(std::io::BufReader::new(file))
            .read_info()
            .expect("read png header");
        assert_eq!(reader.info().width, 546);
        assert_eq!(reader.info().height, 410);
        let mut buf = vec![0u8; reader.output_buffer_size().expect("buffer size")];
        let info = reader.next_frame(&mut buf).expect("read png frame");
        assert_eq!(info.width, 546);
        assert_eq!(info.height, 410);
        let rgba = &buf[..info.buffer_size()];

        let mut expected = Vec::with_capacity(546 * 410 * 4);
        for y in 0..410usize {
            for x in 0..546usize {
                let index = frame.indices[y * film.stride + x] as usize;
                let entry = &film.palette[index * 4..index * 4 + 4];
                expected.extend_from_slice(&[entry[1], entry[2], entry[3], 255 - entry[0]]);
            }
        }
        assert_eq!(
            rgba, expected,
            "frame {i}: PNG pixels differ from the palette-expanded frame"
        );
    }
}

fn sha256_hex(data: &[u8]) -> String {
    let digest = Sha256::digest(data);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

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
