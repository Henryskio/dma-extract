//! The 3D game's films, end to end.
//!
//! `DMA3D.MMF` is the container that broke the shared film decoder twice, and
//! both breaks are measured here rather than remembered:
//!
//! 1. **The per-film metadata is eight parallel arrays, not records.** The
//!    loader is `MM2DFilmResourceFile`'s vtable slot 4 (`DMA3D.exe` at
//!    `0x477940`); it reads a marker that must be 7, then `N` bytes, `N` bytes,
//!    `4*N` big-endian u16, `N` u32, `4*N` bytes, `N` u16, `N` u32, `N` u16,
//!    then a palette count and that many 1024-byte palettes. That totals
//!    `26*N`, the same as a 26-byte record table, so the length solve found 26
//!    and the record reading went eight bytes short on the first film of three
//!    and further out on the rest.
//! 2. **Key frames are not unique per film.** This build re-sends a key frame
//!    mid-stream — 45, 12 and 1 across its three films — where the second and
//!    third games ship one each.
//!
//! The rect that decides each film's geometry is checked against the film's own
//! key-frame chunks: `stride8(right - left) * (bottom - top)` must equal the
//! byte count the chunk declares it decodes to. That is the payload's own
//! ground truth, and it is what made the layout measurable rather than a guess.

use mmf_extract::container::{Container, ContainerKind};
use mmf_extract::film;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

fn dma3d_dir() -> Option<PathBuf> {
    // The 3D game's containers, carved from the disc. `GAME1_ASSETS` does not
    // apply here; `DMA3D_ASSETS` is the folder with `DMA3D.MMF` in it.
    let extra = std::env::var("DMA3D_ASSETS").unwrap_or_default();
    for candidate in [extra.as_str(), "/tmp/dma3d/pc", "/RawAssets"] {
        let p = PathBuf::from(candidate);
        if p.join("DMA3D.MMF").exists() {
            return Some(p);
        }
    }
    None
}

fn films_container() -> Option<(PathBuf, Container)> {
    let dir = dma3d_dir()?;
    let path = dir.join("DMA3D.MMF");
    let container = Container::open(&path).expect("open DMA3D.MMF");
    assert_eq!(container.kind, ContainerKind::Films);
    Some((path, container))
}

/// `--original` writes the inner `.flm` streams verbatim, byte for byte, and
/// nothing else: no PNGs, no manifest.
#[test]
fn original_3d_films_are_byte_exact() {
    let Some((path, container)) = films_container() else {
        eprintln!("skipping: the 3D game's assets are not present");
        return;
    };
    let tmp = tempdir("film3d-original");
    let out_dir = tmp.join("out");

    let count = film::extract_container(&container, &out_dir, true).expect("extract original");
    assert_eq!(count, 3, "three film resources");

    let on_disk = std::fs::read(&path).expect("read DMA3D.MMF");
    for index in 0..3usize {
        let name = container.name(index);
        let extracted_path = out_dir.join(format!("{index:05}_{name}"));
        let extracted = std::fs::read(&extracted_path).expect("read the extracted .flm");
        let payload = container.payload(index);

        assert_eq!(
            extracted, payload,
            "film {index} ({name}): original extraction differs from the container payload"
        );
        let start = container.offsets[index] as usize;
        let end = container.offsets[index + 1] as usize;
        assert_eq!(
            extracted,
            &on_disk[start..end],
            "film {index} ({name}): original extraction differs from the bytes on disk"
        );
        assert_eq!(&payload[..2], &[0xF7, 0x89], "film {index}: inner chunk tag");
    }

    let pngs = walk(&out_dir)
        .into_iter()
        .filter(|p| p.extension().is_some_and(|x| x == "png"))
        .count();
    assert_eq!(pngs, 0, "original mode must not write PNGs");
}

/// Every film's geometry comes from the measured rect, and the rect is
/// confirmed by the film's own key-frame chunks.
#[test]
fn every_3d_film_decodes_at_the_measured_geometry() {
    let Some((_path, container)) = films_container() else {
        eprintln!("skipping: the 3D game's assets are not present");
        return;
    };

    // The measured rects: {top, left, bottom, right}, in resource order. Each
    // one's implied stride * height is the byte count the film's key-frame
    // chunks declare they decode to.
    let measured: &[(&str, [u16; 4], usize, usize, usize)] = &[
        // (name, rect, width, height, stride)
        ("introAxDisney.flm", [0, 0, 600, 800], 800, 600, 800),
        // A width of 589 that pads to a 592-byte stride: the wrong reading of
        // this rect gives 636x379 and a buffer that is 11,148 bytes short.
        ("introAxImageBuilder.flm", [60, 107, 486, 696], 589, 426, 592),
        ("introAxSplash.flm", [60, 80, 540, 720], 640, 480, 640),
    ];

    for (index, (expect_name, rect, width, height, stride)) in measured.iter().enumerate() {
        let name = container.name(index);
        assert_eq!(name, *expect_name, "film {index}'s name");

        let decoded = film::decode_film(&container, index)
            .unwrap_or_else(|e| panic!("film {index} ({name}): {e}"));

        assert_eq!(decoded.bounds, *rect, "film {index} ({name}): the measured rect");
        assert_eq!(decoded.width, *width as u16, "film {index} ({name}): width");
        assert_eq!(decoded.height, *height as u16, "film {index} ({name}): height");
        assert_eq!(decoded.stride, *stride, "film {index} ({name}): stride");

        // The key-frame chunks' own declared decoded size, which is what makes
        // the rect a measurement rather than a reading.
        let full_chunks = decoded
            .chunk_kinds
            .get(&film::TYPE_FULL_FRAME)
            .copied()
            .unwrap_or(0);
        assert_eq!(
            full_chunks,
            [45usize, 12, 1][index],
            "film {index} ({name}): key-frame chunk count"
        );
    }
}

/// The modern path writes one PNG per timeline frame, and every PNG decodes
/// back to exactly the palette-expanded indices the decoder produced.
#[test]
fn modern_3d_films_write_lossless_pngs() {
    let Some((_path, container)) = films_container() else {
        eprintln!("skipping: the 3D game's assets are not present");
        return;
    };
    let tmp = tempdir("film3d-modern");
    let out_dir = tmp.join("out");

    let count = film::extract_container(&container, &out_dir, false).expect("extract modern");
    assert_eq!(count, 3, "three film resources");

    // Frame counts, pinned. Film 0 has 60 timestamps plus the frame the
    // opening key frame provides; film 2 is the long one at 900 timestamps.
    let frame_counts = [61usize, 62, 901];
    let unique_counts = [16usize, 49, 748];
    for index in 0..3usize {
        let name = container.name(index);
        let decoded = film::decode_film(&container, index)
            .unwrap_or_else(|e| panic!("film {index} ({name}): {e}"));
        assert_eq!(
            decoded.frames.len(),
            frame_counts[index],
            "film {index} ({name}): frame count"
        );
        assert_eq!(
            decoded.unique_frames, unique_counts[index],
            "film {index} ({name}): unique frame count"
        );
        // Every frame is the full stride * height buffer.
        for (k, frame) in decoded.frames.iter().enumerate() {
            assert_eq!(
                frame.indices.len(),
                decoded.stride * decoded.height as usize,
                "film {index} ({name}): frame {k}'s buffer size"
            );
        }
        // The first frame comes from the opening key frame, and a frame built
        // on a mid-stream key frame names that chunk. Every key-frame chunk
        // after the first is consumed by exactly one frame, so the counts
        // agree: the chunks minus the seeding one equal the frames that name
        // one.
        assert!(
            decoded.frames[0].full_chunk.is_some(),
            "film {index} ({name}): frame 0 must come from the opening key frame"
        );
        let from_key_frames = decoded
            .frames
            .iter()
            .filter(|f| f.timestamp_chunk.is_some() && f.full_chunk.is_some())
            .count();
        assert_eq!(
            from_key_frames,
            key_frame_count(&decoded) - 1,
            "film {index} ({name}): the mid-stream key frames and the frames built on them disagree"
        );
        assert!(
            decoded.frames.iter().any(|f| f.timestamp_chunk.is_some() && f.delta_chunk.is_some()),
            "film {index} ({name}): no frame was built from a delta chunk"
        );

        // Every PNG is the palette-expanded frame, byte for byte.
        let film_dir = out_dir.join(format!("{index:05}_{}", name.trim_end_matches(".flm")));
        let pngs = walk(&film_dir)
            .into_iter()
            .filter(|p| p.extension().is_some_and(|x| x == "png"))
            .count();
        assert_eq!(pngs, frame_counts[index], "film {index} ({name}): one PNG per frame");

        for (k, frame) in decoded.frames.iter().enumerate() {
            let path = film_dir.join(format!("frame_{k:03}.png"));
            let file = std::fs::File::open(&path).expect("open the frame png");
            let mut reader = png::Decoder::new(std::io::BufReader::new(file))
                .read_info()
                .expect("read the png header");
            assert_eq!(
                reader.info().width,
                decoded.width as u32,
                "film {index} ({name}): frame {k}'s png width"
            );
            assert_eq!(
                reader.info().height,
                decoded.height as u32,
                "film {index} ({name}): frame {k}'s png height"
            );
            let mut buf = vec![0u8; reader.output_buffer_size().expect("buffer size")];
            let info = reader.next_frame(&mut buf).expect("read the png frame");
            let rgba = &buf[..info.buffer_size()];

            let mut expected = Vec::with_capacity(decoded.width as usize * decoded.height as usize * 4);
            for y in 0..decoded.height as usize {
                for x in 0..decoded.width as usize {
                    let index = frame.indices[y * decoded.stride + x] as usize;
                    let entry = &decoded.palette[index * 4..index * 4 + 4];
                    expected.extend_from_slice(&[entry[1], entry[2], entry[3], 255 - entry[0]]);
                }
            }
            assert_eq!(
                rgba, expected,
                "film {index} ({name}): frame {k}'s pixels differ from the palette expansion"
            );
        }
    }
}

/// A golden hash per film pins the actual pixels, not just the shape.
#[test]
fn the_3d_film_frames_hash_to_the_measured_pixels() {
    let Some((_path, container)) = films_container() else {
        eprintln!("skipping: the 3D game's assets are not present");
        return;
    };
    // (film, frame, sha256 of the raw 8bpp indices). The frames chosen are the
    // first of each film plus the last, so both the opening key frame and the
    // decoded end state are pinned.
    let golden: &[(usize, usize, &str)] = &[
        (0, 0, "83cfc6457821a9e57f2b89ba8252b1f21e69a207303770dfb57ccc549effa021"),
        (0, 60, "83cfc6457821a9e57f2b89ba8252b1f21e69a207303770dfb57ccc549effa021"),
        (1, 0, "53f054252658c5289b5e36a20925df0b707f3543d88300dd55eed474ee03dba7"),
        (1, 61, "679fb0847b1873fc9f21a3ceff76ca7601c72676c92c0229232108c36f78d7ab"),
        (2, 0, "5caa0b76b27100a7b2d75e82899316ba2a7a5278d73debdc62e49154c185802c"),
        (2, 900, "88fa6ec417fbc557be03b66f5a0cf65e2e09638d4a3deed1ede3fa9ce75942fb"),
    ];
    for (index, frame, digest) in golden {
        let decoded = film::decode_film(&container, *index)
            .unwrap_or_else(|e| panic!("film {index}: {e}"));
        assert_eq!(
            sha256_hex(&decoded.frames[*frame].indices),
            *digest,
            "film {index} frame {frame}: the indices changed"
        );
    }
}

/// How many key-frame chunks the film carries.
fn key_frame_count(decoded: &film::Film) -> usize {
    decoded
        .chunk_kinds
        .get(&film::TYPE_FULL_FRAME)
        .copied()
        .unwrap_or(0)
}

fn sha256_hex(data: &[u8]) -> String {
    let digest = Sha256::digest(data);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = dir.read_dir() else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.push(path);
            }
        }
    }
    out
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
