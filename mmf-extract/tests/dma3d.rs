//! Tests for the 3D game's `Models` and `Textures` containers.
//!
//! These run against the shipped containers when they are present, and skip
//! otherwise (the same rule the other asset-dependent suites follow). The
//! assets are carved from the CD image with `iso_range.py` -- the Italian
//! (`Disney Creazioni in 3D`) and English (`Disney's Magic Artist 3D`) builds
//! both work, and the tests take whichever folder `DMA3D_ASSETS` points at.
//!
//! What is asserted here is the *measured* part of the format: the skeleton
//! the container shares with the other games, the two new kinds' metadata
//! region sizes, and a byte-exact payload for every resource in both modes.
//! The payload *contents* (`.mod` geometry, texture pixels) are not decoded
//! yet, so nothing here claims they are -- see `docs/DMA3D.md`.

use mmf_extract::container::{Container, ContainerKind};
use std::path::{Path, PathBuf};

/// A `Models` container and a `Textures` container, by name.
const MODEL_CONTAINER: &str = "DMA3D.MMM";
const TEXTURE_CONTAINER: &str = "DMA3D.MMX";

fn dma3d_dir() -> Option<PathBuf> {
    let extra = std::env::var("DMA3D_ASSETS").unwrap_or_default();
    for candidate in [
        "/RawAssets/Dati3D",
        extra.as_str(),
        "/tmp/dma3d/pc",
        "/tmp/en/dma3d_en",
    ] {
        let path = PathBuf::from(candidate);
        if path.is_dir() && path.join(MODEL_CONTAINER).exists() {
            return Some(path);
        }
    }
    None
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

/// The two new kinds must be selected by the tag in the signature.
#[test]
fn the_3d_containers_open_with_their_own_kinds() {
    let Some(dir) = dma3d_dir() else {
        eprintln!("skipping: the 3D game's assets are not present");
        return;
    };

    let models = Container::open(dir.join(MODEL_CONTAINER)).expect("open Models");
    assert_eq!(models.kind, ContainerKind::Models);
    assert_eq!(models.version, 3);
    // The skeleton the games share: N at 0x22, 4*(N+1) offsets, 32-byte names.
    assert_eq!(models.resource_count, 460);
    assert_eq!(models.offsets.len(), 461);
    assert_eq!(*models.offsets.last().unwrap() as usize, models.data.len());
    assert!(models.offsets.windows(2).all(|w| w[0] <= w[1]));
    assert_eq!(models.big_endian, true);
    assert_eq!(models.name(0), "createAmbientLight.mod");

    let textures = Container::open(dir.join(TEXTURE_CONTAINER)).expect("open Textures");
    assert_eq!(textures.kind, ContainerKind::Textures);
    assert_eq!(textures.version, 3);
    assert_eq!(textures.resource_count, 705);
    assert_eq!(textures.offsets.len(), 706);
    assert_eq!(
        *textures.offsets.last().unwrap() as usize,
        textures.data.len()
    );
    assert_eq!(textures.name(0), "CreationSlotTex.TGA");
}

/// The metadata region of each new kind is `4*N + 100` / `6*N + 2`, which is
/// what pins the boundary between the names table and the first payload.
#[test]
fn the_metadata_regions_are_measured() {
    let Some(dir) = dma3d_dir() else {
        eprintln!("skipping: the 3D game's assets are not present");
        return;
    };

    for (name, n_expected, region_of) in [
        (
            MODEL_CONTAINER,
            460usize,
            mmf_extract::dma3d::models_meta_len as fn(usize) -> usize,
        ),
        (
            TEXTURE_CONTAINER,
            705usize,
            mmf_extract::dma3d::textures_meta_len as fn(usize) -> usize,
        ),
    ] {
        let data = std::fs::read(dir.join(name)).expect("read container");
        let n = u16::from_be_bytes([data[0x22], data[0x23]]) as usize;
        assert_eq!(n, n_expected, "{name}: resource count");

        let off_base = 0x24usize;
        let first = u32::from_be_bytes([
            data[off_base],
            data[off_base + 1],
            data[off_base + 2],
            data[off_base + 3],
        ]) as usize;
        let last = u32::from_be_bytes([
            data[off_base + 4 * n],
            data[off_base + 4 * n + 1],
            data[off_base + 4 * n + 2],
            data[off_base + 4 * n + 3],
        ]) as usize;
        assert_eq!(last, data.len(), "{name}: the offset table must end at EOF");

        let names_end = off_base + 4 * (n + 1) + 2;
        let region = first - (names_end + 32 * n);
        let want = region_of(n);
        assert_eq!(
            region, want,
            "{name}: metadata region is {region} bytes, expected {want}"
        );
    }
}

/// Every resource's payload must come out byte-exact in `--original` mode,
/// and the modern path must agree with it while it is a passthrough.
#[test]
fn every_3d_payload_round_trips_byte_exactly() {
    let Some(dir) = dma3d_dir() else {
        eprintln!("skipping: the 3D game's assets are not present");
        return;
    };
    let tmp = tempdir("dma3d");

    for (name, kind) in [
        (MODEL_CONTAINER, ContainerKind::Models),
        (TEXTURE_CONTAINER, ContainerKind::Textures),
    ] {
        let container = Container::open(dir.join(name)).expect("open container");
        assert_eq!(container.kind, kind);
        let n = container.resource_count as usize;

        let modern = tmp.join(format!("modern/{name}"));
        let original = tmp.join(format!("original/{name}"));
        let report = match kind {
            ContainerKind::Models => mmf_extract::dma3d::extract_models(&container, &modern, false)
                .expect("extract models"),
            _ => mmf_extract::dma3d::extract_textures(&container, &modern, false)
                .expect("extract textures"),
        };
        assert!(report.failed.is_empty(), "{name}: {:?}", report.failed);
        assert_eq!(report.written, n, "{name}: not every resource was written");

        let report_orig = match kind {
            ContainerKind::Models => {
                mmf_extract::dma3d::extract_models(&container, &original, true)
                    .expect("extract models")
            }
            _ => mmf_extract::dma3d::extract_textures(&container, &original, true)
                .expect("extract textures"),
        };
        assert_eq!(report_orig.written, n);

        for entry in &report.resources {
            let payload = container.payload(entry.index);
            assert_eq!(entry.bytes, payload.len());

            let modern_bytes = std::fs::read(&entry.path).expect("read modern output");
            assert_eq!(
                modern_bytes, payload,
                "{name} resource {}: modern output is not byte-exact",
                entry.index
            );
            // The tripwire: while the payload is a passthrough, `--original`
            // writes the same bytes. A real decoder breaks this on purpose.
            let original_bytes = std::fs::read(
                &original.join(entry.path.file_name().expect("output has a file name")),
            )
            .expect("read original output");
            assert_eq!(
                original_bytes, payload,
                "{name} resource {}: --original is not byte-exact",
                entry.index
            );
        }
        eprintln!("{name}: {n} resources, all byte-exact");
    }
}

/// The model payloads begin with the `mod II` magic and recur inside the
/// stream. This is what the geometry decoder has to explain; asserting it
/// here keeps the measurement visible next to the decoder that will use it.
#[test]
fn the_model_payloads_carry_the_mod_magic() {
    let Some(dir) = dma3d_dir() else {
        eprintln!("skipping: the 3D game's assets are not present");
        return;
    };
    let container = Container::open(dir.join(MODEL_CONTAINER)).expect("open Models");
    let n = container.resource_count as usize;

    let mut with_magic = 0usize;
    for i in 0..n {
        let payload = container.payload(i);
        if payload.starts_with(b"mod II") {
            with_magic += 1;
        }
    }
    assert_eq!(
        with_magic, n,
        "{with_magic} of {n} model payloads start with the `mod II` magic"
    );

    // The magic is little-endian ("II"), and the stream carries 32-bit
    // floats: 0x3f800000 read little-endian from payload[0] is 1.0.
    let payload = container.payload(0);
    let float_at = |off: usize| {
        f32::from_le_bytes([
            payload[off],
            payload[off + 1],
            payload[off + 2],
            payload[off + 3],
        ])
    };
    let floats = (6..payload.len().saturating_sub(4))
        .step_by(4)
        .map(float_at)
        .filter(|f| f.is_finite() && f.abs() < 1e6)
        .count();
    assert!(
        floats > 0,
        "the model stream holds no plausible little-endian floats"
    );
}
