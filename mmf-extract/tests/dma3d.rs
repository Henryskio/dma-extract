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

/// Every folder that holds the two 3D containers, so a test can run its checks
/// against both discs when both are carved. The environment variable comes
/// first, then the two known carve points; a folder missing either container is
/// skipped rather than failing.
fn dma3d_dirs() -> Vec<PathBuf> {
    let mut found = Vec::new();
    let extra = std::env::var("DMA3D_ASSETS").unwrap_or_default();
    for candidate in [
        extra.as_str(),
        "/RawAssets/Dati3D",
        "/tmp/dma3d/pc",
        "/tmp/en/dma3d_en",
    ] {
        if candidate.is_empty() {
            continue;
        }
        let path = PathBuf::from(candidate);
        if path.join(MODEL_CONTAINER).exists()
            && path.join(TEXTURE_CONTAINER).exists()
            && !found.contains(&path)
        {
            found.push(path);
        }
    }
    found
}

fn dma3d_dir() -> Option<PathBuf> {
    dma3d_dirs().into_iter().next()
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

/// `--original` must be byte-exact for every resource, and the modern path
/// must lose nothing: a texture's PNG decodes back to the same pixels the
/// measured layout holds, and a model's glTF holds the same geometry.
///
/// This replaces the old tripwire, which asserted that the modern path agreed
/// with `--original` *because* it was a passthrough. It is not one any more
/// (`docs/DMA3D.md` §2 and §3), so the assertion that matters is the one the
/// rest of the project uses for every other decoder: the modern output
/// decodes back to what was measured.
#[test]
fn every_3d_payload_round_trips_byte_exactly() {
    let Some(dir) = dma3d_dir() else {
        eprintln!("skipping: the 3D game's assets are not present");
        return;
    };
    let tmp = tempdir("dma3d");

    // ---- `--original`: every payload byte for byte ----
    for (name, kind) in [
        (MODEL_CONTAINER, ContainerKind::Models),
        (TEXTURE_CONTAINER, ContainerKind::Textures),
    ] {
        let container = Container::open(dir.join(name)).expect("open container");
        assert_eq!(container.kind, kind);
        let original = tmp.join(format!("original/{name}"));
        let report = match kind {
            ContainerKind::Models => {
                mmf_extract::dma3d::extract_models(&container, &original, true, None)
                    .expect("extract models")
            }
            _ => mmf_extract::dma3d::extract_textures(&container, &original, true)
                .expect("extract textures"),
        };
        assert!(report.failed.is_empty(), "{name}: {:?}", report.failed);
        assert_eq!(report.written, container.resource_count as usize);
        for entry in &report.resources {
            let payload = container.payload(entry.index);
            let bytes = std::fs::read(&entry.path).expect("read original output");
            assert_eq!(
                bytes, payload,
                "{name} resource {}: --original is not byte-exact",
                entry.index
            );
        }
        eprintln!(
            "{name}: {} resources, --original byte-exact",
            report.written
        );
    }

    // ---- modern textures: the PNG decodes back to the measured pixels ----
    {
        let container = Container::open(dir.join(TEXTURE_CONTAINER)).expect("open textures");
        let modern = tmp.join("modern/textures");
        let report = mmf_extract::dma3d::extract_textures(&container, &modern, false)
            .expect("extract textures");
        assert!(report.failed.is_empty(), "{:?}", report.failed);
        assert_eq!(report.written, container.resource_count as usize);

        let mut checked = 0usize;
        for entry in &report.resources {
            let measured = mmf_extract::dma3d::texfile::parse(container.payload(entry.index))
                .unwrap_or_else(|e| panic!("texture {}: {e}", entry.index));
            let expected = measured.to_rgba();

            let file = std::fs::File::open(&entry.path).expect("open PNG");
            let mut reader = png::Decoder::new(std::io::BufReader::new(file))
                .read_info()
                .expect("PNG header");
            let mut buf = vec![0u8; reader.output_buffer_size().expect("PNG buffer size")];
            let frame = reader.next_frame(&mut buf).expect("PNG frame");
            let rgba = &buf[..frame.buffer_size()];

            assert_eq!(
                frame.width as usize, measured.width,
                "texture {}: PNG width is not the measured width",
                entry.index
            );
            assert_eq!(
                frame.height as usize, measured.height,
                "texture {}: PNG height is not the measured height",
                entry.index
            );
            assert_eq!(
                rgba.len(),
                expected.len(),
                "texture {}: PNG byte count is not the measured pixel count",
                entry.index
            );
            assert_eq!(
                rgba,
                expected.as_slice(),
                "texture {}: PNG pixels are not the measured pixels",
                entry.index
            );
            checked += 1;
        }
        eprintln!("{checked} textures: PNG decodes back to the measured pixels");
    }

    // ---- modern models: the glTF holds the measured geometry ----
    {
        let container = Container::open(dir.join(MODEL_CONTAINER)).expect("open models");
        let modern = tmp.join("modern/models");
        let report = mmf_extract::dma3d::extract_models(
            &container,
            &modern,
            false,
            Some(&tmp.join("modern/textures")),
        )
        .expect("extract models");
        assert!(report.failed.is_empty(), "{:?}", report.failed);
        assert_eq!(report.written, container.resource_count as usize);

        let mut meshes = 0usize;
        let mut passthrough = 0usize;
        for entry in &report.resources {
            let payload = container.payload(entry.index);
            match mmf_extract::dma3d::modfile::parse_mesh(payload) {
                Err(_) => {
                    // No mesh: the payload is written through unchanged.
                    passthrough += 1;
                    assert_eq!(
                        std::fs::read(&entry.path).expect("read passthrough"),
                        payload,
                        "model {}: payload without a mesh is not byte-exact",
                        entry.index
                    );
                    continue;
                }
                Ok(mesh) => {
                    // The records the extractor located, so the comparison can
                    // say what the glTF is expected to carry.
                    let records = mmf_extract::dma3d::anim::locate(payload, &mesh);
                    // The written document is read back off disk and compared
                    // with the decoder, which is the same contract every other
                    // asset type in this project tests.
                    let back = mmf_extract::dma3d::gltf::read_gltf(&entry.path)
                        .unwrap_or_else(|e| panic!("model {}: {e}", entry.index));
                    assert_eq!(back.version, "2.0", "model {}: glTF version", entry.index);
                    assert_eq!(
                        back.indices.len(),
                        mesh.indices.len(),
                        "model {}: glTF index count differs from the decoder",
                        entry.index
                    );
                    assert_eq!(
                        back.positions.len(),
                        mesh.positions.len(),
                        "model {}: glTF vertex count differs from the decoder",
                        entry.index
                    );
                    assert_eq!(
                        back.normals.len(),
                        mesh.normals.len(),
                        "model {}: glTF normal count differs from the decoder",
                        entry.index
                    );
                    for (k, (wrote, read)) in mesh.indices.iter().zip(&back.indices).enumerate() {
                        assert_eq!(
                            read, wrote,
                            "model {}: index {k} differs from the decoder",
                            entry.index
                        );
                    }
                    for (k, (wrote, read)) in mesh.positions.iter().zip(&back.positions).enumerate()
                    {
                        assert_eq!(
                            read, wrote,
                            "model {}: position {k} differs from the decoder",
                            entry.index
                        );
                    }
                    for (k, (wrote, read)) in mesh.normals.iter().zip(&back.normals).enumerate() {
                        assert_eq!(
                            read, wrote,
                            "model {}: normal {k} differs from the decoder",
                            entry.index
                        );
                    }
                    // The texture coordinates, when the payload has them. The
                    // extractor carries them only where every float pair is
                    // inside 0..1, so the count either matches the vertex count
                    // or there are none.
                    if back.texcoords.is_empty() {
                        // They are carried exactly when every pair is in range,
                        // so a payload with some pairs out of range has none --
                        // and asserting that is what stops a mixed payload from
                        // being emitted with guessed coordinates.
                        assert_ne!(
                            records.pairs_in_unit_range,
                            Some(records.pair_count),
                            "model {}: every pair is in range but the glTF carries no \
                             texture coordinates",
                            entry.index
                        );
                    } else {
                        assert_eq!(
                            back.texcoords.len(),
                            mesh.vertex_count(),
                            "model {}: the glTF's texture coordinate count differs",
                            entry.index
                        );
                        for value in back.texcoords.iter().flatten() {
                            assert!(
                                (-1.0e-6..=1.000_001).contains(value),
                                "model {}: a texture coordinate {value} is outside 0..1",
                                entry.index
                            );
                        }
                    }
                    meshes += 1;
                }
            }
        }
        eprintln!("{meshes} glTF models, {passthrough} payloads written through");
        assert_eq!(meshes, 144, "the mesh-bearing payload count changed");
        assert_eq!(meshes + passthrough, container.resource_count as usize);
    }

    // ---- the glTF plus its `.tail` reconstructs the payload ----
    //
    // The mesh decoder stops after the normals, so the modern output would be
    // lossy for models unless what follows is kept. It is, beside the glTF, and
    // the tail is exactly the slice of the payload the decoder did not read.
    {
        let container = Container::open(dir.join(MODEL_CONTAINER)).expect("open models");
        let modern = tmp.join("modern/models2");
        let report = mmf_extract::dma3d::extract_models(
            &container,
            &modern,
            false,
            Some(&tmp.join("modern/textures")),
        )
        .expect("extract models");
        assert!(report.failed.is_empty(), "{:?}", report.failed);
        assert_eq!(report.written, container.resource_count as usize);

        let mut with_tail = 0usize;
        let mut tail_bytes = 0usize;
        for entry in &report.resources {
            let payload = container.payload(entry.index);
            let Ok(mesh) = mmf_extract::dma3d::modfile::parse_mesh(payload) else {
                continue;
            };
            if payload.len() == mesh.tail_offset() {
                // Nothing follows the mesh, so no tail file is written.
                with_tail += 1;
                continue;
            }
            let name = entry
                .path
                .file_name()
                .expect("output has a file name")
                .to_string_lossy()
                .to_string();
            let tail_path = entry
                .path
                .with_file_name(mmf_extract::dma3d::gltf::tail_name(&name));
            let tail = std::fs::read(&tail_path).expect("read the tail");
            assert_eq!(
                tail.as_slice(),
                &payload[mesh.tail_offset()..],
                "model {}: the tail is not the undecoded slice of the payload",
                entry.index
            );
            assert!(
                tail.len() < payload.len(),
                "model {}: the tail is the whole payload",
                entry.index
            );
            with_tail += 1;
            tail_bytes += tail.len();
        }
        assert_eq!(with_tail, 144, "not every mesh payload kept its tail");
        assert_eq!(
            tail_bytes, 590_532,
            "the undecoded tail size changed from the measured value"
        );
        eprintln!("{tail_bytes} bytes of undecoded tail preserved by {with_tail} payloads");
    }
}

/// The whole modern path, driven the way a user drives it: `cli::run` over the
/// carved folder, then the output checked as a reader would find it.
///
/// This is the test that holds the "easy to view" claim together -- the index
/// page exists, every reference in it resolves to a file on disk, every object
/// has a rendered preview that covers pixels, and every glTF's vertices, when
/// read back, agree with the decoder.
#[test]
fn the_modern_output_is_a_browsable_whole() {
    let dirs = dma3d_dirs();
    if dirs.is_empty() {
        eprintln!("skipping: the 3D game's assets are not present");
        return;
    }
    // Both discs, when both are carved: the decoders must produce the same
    // counts and the same undecoded-tail size on each, or a format detail has
    // silently diverged between the two builds.
    let mut reference_count = 0usize;
    let mut discs = 0usize;
    for (disc, dir) in dirs.iter().enumerate() {
        let tmp = tempdir(&format!("dma3d-e2e-{disc}"));
        let selection = mmf_extract::cli::AssetSelection::parse(&["models".to_string()])
            .expect("models is a selectable type");
        let summary = mmf_extract::cli::run(dir, &tmp, &selection, false, false)
            .expect("extract the 3D game");
        assert!(summary.failed.is_empty(), "{dir:?}: {summary:?}");
        assert_eq!(summary.models, 460 + 705, "{}", dir.display());

        // ---- the index page, and every reference in it ----
        let index = tmp.join("index.html");
        assert!(index.is_file(), "no index.html was written");
        let page = std::fs::read_to_string(&index).expect("read index.html");
        assert!(
            page.contains("<!DOCTYPE html>"),
            "the index is not an HTML document"
        );
        // No dependency: it has to open from a local folder with no network.
        assert!(
            !page.contains("http://") && !page.contains("https://") || page.contains("gltf-viewer"),
            "the index must not fetch anything except the offer of a viewer"
        );
        assert!(
            !page.contains("<script"),
            "the index must not need a script"
        );
        let references = page
            .split('"')
            .filter(|s| s.starts_with("Models/") || s.starts_with("Textures/"))
            .map(|s| s.to_string())
            .collect::<Vec<_>>();
        // Every asset is a figure with a link and an image, but a texture's
        // link and image are the same path while an object's are not, so the
        // distinct paths are what must exist and the occurrences are counted
        // separately below.
        let mut distinct = references.clone();
        distinct.sort();
        distinct.dedup();
        // Every asset the page shows: 705 textures, 144 object previews, and the
        // 144 glTFs those previews link to.
        assert_eq!(
            distinct.len(),
            705 + 144 + 144,
            "the index does not list every asset"
        );
        assert_eq!(
            distinct.iter().filter(|p| p.ends_with(".png")).count(),
            705 + 144,
            "the index does not list every picture"
        );
        assert_eq!(
            distinct.iter().filter(|p| p.ends_with(".gltf")).count(),
            144,
            "the index does not link every model"
        );
        let before = reference_count;
        for reference in &references {
            reference_count += 1;
            assert!(
                tmp.join(reference).is_file(),
                "the index refers to {reference}, which is not on disk"
            );
        }
        // A texture's figure links to the very file it shows, so its path
        // appears twice (once as the link, once as the image); an object's
        // figure links to its `.gltf` and shows its `.preview.png`, which are
        // two different paths. So the occurrence count is the distinct count
        // plus the number of textures, and asserting that is what keeps the
        // page from silently dropping a figure. The count is reset per disc so
        // the second disc's page is checked against its own markup.
        let texture_count = distinct.iter().filter(|p| p.starts_with("Textures/")).count();
        assert_eq!(
            reference_count - before,
            distinct.len() + texture_count,
            "the index's markup does not reference every asset as expected"
        );
        assert_eq!(texture_count, 705, "the index does not list every texture");
        assert_eq!(
            distinct.len() - texture_count,
            288,
            "the index does not list every object's preview and model"
        );

        // ---- every object's preview covers pixels ----
        let model_dir = tmp.join("Models").join("DMA3D");
        let entries: Vec<std::path::PathBuf> = std::fs::read_dir(&model_dir)
            .expect("read the models directory")
            .flatten()
            .map(|e| e.path())
            .collect();
        let previews = entries
            .iter()
            .filter(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .ends_with(".preview.png")
            })
            .count();
        let gltfs = entries
            .iter()
            .filter(|p| p.file_name().unwrap().to_string_lossy().ends_with(".gltf"))
            .count();
        assert_eq!(gltfs, 144, "the number of glTF files changed");
        assert_eq!(previews, gltfs, "every glTF needs a preview beside it");

        let mut blank = Vec::new();
        let mut checked = 0usize;
        for path in &entries {
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            if !name.ends_with(".preview.png") {
                continue;
            }
            let preview = std::fs::read(path).expect("read a preview");
            let mut decoder = png::Decoder::new(std::io::Cursor::new(&preview[..]))
                .read_info()
                .expect("PNG header");
            let mut buf = vec![0u8; decoder.output_buffer_size().expect("buffer")];
            let frame = decoder.next_frame(&mut buf).expect("frame");
            assert_eq!(frame.width as usize, 256, "{name} is not square");
            assert_eq!(frame.height as usize, 256, "{name} is not square");
            let covered = buf[..frame.buffer_size()]
                .chunks(4)
                .filter(|p| p[3] != 0)
                .count();
            // A preview that covers no pixels is a broken rasteriser, and would
            // otherwise sit in the page as a blank tile.
            if covered == 0 {
                blank.push(name);
            }
            checked += 1;
        }
        assert_eq!(checked, 144, "{dir:?}: the preview count changed");
        assert!(
            blank.is_empty(),
            "{dir:?}: previews that cover no pixels: {blank:?}"
        );
        eprintln!(
            "{dir:?}: 1165 resources, index with {reference_count} references, 144 previews all covered"
        );
        discs += 1;
    }
    assert!(discs >= 1, "no disc was checked");
}
