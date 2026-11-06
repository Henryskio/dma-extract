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
        // Where the 3D game's containers are carved out of its disc image by
        // `lean/test/carve3d.py`: the game ships a disc, so its containers are
        // runs of bytes inside it rather than files in a folder.
        "/workspace/.tmp/dma3d/assets",
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
    // The count comes from the table's payload boundaries, not from the header's
    // `N`. The Italian `DMA3D.MMX`'s table holds 782 entries of which 76 (indices
    // 503..578) were overwritten with values such as `0xffffffff`; reading them
    // as boundaries reported 76 resources as failures whose payloads "ran past
    // the end of the file". They are not boundaries: dropping them leaves 706,
    // which is 705 resources — exactly the header's own count — spanning the
    // payload region contiguously (`docs/DMA3D.md` §11.7).
    assert_eq!(textures.resource_count, 705);
    assert_eq!(textures.declared_count, 705);
    assert_eq!(textures.offsets.len(), 706);
    assert_eq!(textures.offsets.len(), 706);
    assert_eq!(
        *textures.offsets.last().unwrap() as usize,
        textures.data.len(),
        "the walked count must make the last offset the container's own length"
    );
    // The names table follows the walked count, so the first name is the real
    // one; reading the header's count instead put the names 304 bytes early and
    // produced garbage for the first 38 slots.
    assert_eq!(textures.name(0), "CreationSlotTex.TGA");
    assert_eq!(textures.names.len(), textures.resource_count as usize);
}

/// A corrupt offsets entry must be reported, not panic.
///
/// The Italian disc's `DMA3D.MMX` holds 76 offsets entries that name a range the
/// file does not hold: 38 run backwards (`start > end`) and 74 run past the end
/// of the file, with 36 doing both. `Container::payload` used to index
/// `self.data[s..e]` for them, which is a Rust panic that took the whole
/// extraction down with it and lost every other resource in the container. This
/// is the regression test: a range outside the file answers with a reason and
/// an empty payload, so the resource fails its own layout check and is reported
/// instead — the rule `AGENTS.md` states, and the same rule the Lean core's
/// `Bytes.slice` already enforced.
#[test]
fn a_corrupt_offsets_entry_is_reported_not_panicked() {
    let Some(dir) = dma3d_dir() else {
        eprintln!("skipping: the 3D game's assets are not present");
        return;
    };
    for name in [MODEL_CONTAINER, TEXTURE_CONTAINER] {
        let container = Container::open(dir.join(name)).expect("open container");
        let mut outside = 0usize;
        for i in 0..container.resource_count as usize {
            let start = container.offsets[i] as usize;
            let end = container.offsets[i + 1] as usize;
            if start > end || end > container.data.len() {
                // The entry names a range the file does not hold, so both halves
                // must agree it is one: no payload, and a reason.
                outside += 1;
                let payload = container.payload(i);
                assert!(
                    payload.is_empty(),
                    "{name} resource {i}: an out-of-range offsets entry handed back bytes"
                );
                let why = container
                    .payload_result(i)
                    .expect_err(&format!(
                        "{name} resource {i}: the offsets entry names a range outside the file, \
                         which must be an error rather than a payload"
                    ))
                    .to_string();
                assert!(
                    why.contains("offsets entry"),
                    "{name} resource {i}: {why} does not name what is wrong"
                );
            }
        }
        eprintln!("{name}: {outside} offsets entries outside the file, all reported");
        // The Italian `DMA3D.MMX` is the disc this was measured on; a disc with
        // none of them still passes, because zero is what it reports.
    }
}

/// The metadata region of each new kind is `4*N + 100` / `6*N + 2`, which is
/// what pins the boundary between the names table and the first payload.
#[test]
fn the_metadata_regions_are_measured() {
    let Some(dir) = dma3d_dir() else {
        eprintln!("skipping: the 3D game's assets are not present");
        return;
    };

    // The `Models` container is the control: its header's count, its offset
    // table's extent and its names table all agree, so the region between the
    // names and the first payload is exactly `4*N + 100` for the N the header
    // declares. This is what `docs/DMA3D.md` §2.1 measured on the English disc.
    {
        let data = std::fs::read(dir.join(MODEL_CONTAINER)).expect("read models");
        let n = u16::from_be_bytes([data[0x22], data[0x23]]) as usize;
        assert_eq!(n, 460, "{MODEL_CONTAINER}: resource count");
        let first = u32::from_be_bytes([
            data[0x24],
            data[0x25],
            data[0x26],
            data[0x27],
        ]) as usize;
        let at = 0x24 + 4 * n;
        assert_eq!(
            u32::from_be_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]]) as usize,
            data.len(),
            "{MODEL_CONTAINER}: the offset table must end at EOF"
        );
        let names_end = 0x24 + 4 * (n + 1) + 2 + 32 * n;
        let region = first - names_end;
        assert_eq!(region, mmf_extract::dma3d::models_meta_len(n));
    }

    // The `Textures` container is where the two readings of the *metadata region*
    // part. The count itself is no longer in dispute — the payload boundaries
    // give 705, the header's own — but the region's length and the names'
    // position cannot both be satisfied (`docs/DMA3D.md` §11.7):
    //
    // * read at the header's 705, the names end at 2862 and the region to the
    //   first payload is `29654 - 2862 - 32*705` = 4232, which is exactly
    //   `6*705 + 2` -- but the names at 2862 are garbage; the real ones start at
    //   3166, 304 bytes later;
    // * the names the shipped bytes actually carry start at 3166 and, taken
    //   32 bytes at a time, run to 28158, where a `6*705 + 2` region would have
    //   to reach 32090 -- 2436 bytes past the payload base at 29654.
    //
    // So the region does not fit under either reading of where the names sit,
    // which is why the container records the reason rather than throwing. The
    // payload base -- what the decode reads -- does not depend on the choice, so
    // both readings are asserted and the test fails if either stops matching.
    {
        let data = std::fs::read(dir.join(TEXTURE_CONTAINER)).expect("read textures");
        let declared = u16::from_be_bytes([data[0x22], data[0x23]]) as usize;
        let walked = Container::open(dir.join(TEXTURE_CONTAINER))
            .expect("open textures")
            .resource_count as usize;
        assert_eq!(declared, 705, "the header's declared count");
        // The count the table's boundaries give is the header's own, so the two
        // readings of the *region* are what remain in dispute, not the count
        // (`docs/DMA3D.md` §11.7).
        assert_eq!(walked, 705, "the count the payload boundaries give");
        // The last *boundary* is the container's own length, which is what makes
        // the payload region complete — even though its table index (781) is past
        // the count (705), because 76 entries between them are not boundaries.
        let opened = Container::open(dir.join(TEXTURE_CONTAINER)).expect("open textures");
        assert_eq!(
            *opened.offsets.last().expect("has boundaries") as usize,
            data.len(),
            "the last boundary must be the container's own length"
        );
        assert_eq!(opened.offsets.len(), 706, "706 boundaries for 705 resources");
        let first = u32::from_be_bytes([
            data[0x24],
            data[0x25],
            data[0x26],
            data[0x27],
        ]) as usize;

        // Reading one: the documented region, at the header's count.
        let names_at_declared = 0x24 + 4 * (declared + 1) + 2;
        let region_at_declared = first - (names_at_declared + 32 * declared);
        assert_eq!(
            region_at_declared,
            mmf_extract::dma3d::textures_meta_len(declared),
            "the documented region holds at the header's count"
        );

        // Reading two: the names where they actually are. The shipped bytes put
        // them 304 bytes past the header's reading, which is the 782-entry
        // position — the reading whose count the payload boundaries reject.
        let names_at_782 = 0x24 + 4 * 782 + 2;
        assert_eq!(names_at_782, 3166, "the 782-entry reading puts the names at 3166");
        let name = |at: usize| {
            let end = data[at..at + 24].iter().position(|&b| b == 0).unwrap_or(24);
            String::from_utf8_lossy(&data[at..at + end]).to_string()
        };
        assert_eq!(name(names_at_782), "CreationSlotTex.TGA");
        assert_eq!(name(names_at_782 + 32), "CreationTex.TGA");

        // And the region does not fit under either reading, which is what the
        // container parser records instead of failing. At the header's count the
        // names sit at 2862 (garbage, but the arithmetic closes); at 3166, where
        // the real names are, a `6*705 + 2` region overruns the payload base.
        let region_at_705 = 6 * 705 + 2;
        assert!(
            names_at_782 + 32 * 705 + region_at_705 > first,
            "the names' own reading's region must overrun the payload base"
        );
        assert!(
            names_at_782 < names_at_782 + 32 * 705 + region_at_705,
            "region evaluated"
        );
        let container = Container::open(dir.join(TEXTURE_CONTAINER)).expect("open textures");
        assert!(
            container.metadata_reason.is_some(),
            "the container must record why its metadata region was not read"
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
        // Every payload the container describes is written byte for byte. The
        // Italian `DMA3D.MMX` holds 38 offsets entries that name a range past
        // the end of the file: those are reported as failures rather than
        // written, because a payload outside the container is not a payload.
        // That is the measurement `docs/DMA3D.md` §8.1 records.
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
        // A texture whose index bytes are not a power of two is reported rather
        // than factored into a width no texture in this engine can have, which
        // is the rule `texfile::parse` states.
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
        // Both discs are measured and neither count is a target. The English
        // disc carries 144 mesh-bearing payloads; the Italian disc's carry 17,
        // now that the chain is located structurally rather than by the magic
        // at offset 0 (`docs/DMA3D.md` §11.9).
        // none at all (`docs/DMA3D.md` §10), so a run on it writes every
        // payload through and reports zero glTFs. What must hold on either is
        // the accounting: every payload is either a glTF or a passthrough.
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
        // The Italian disc's 17 mesh payloads preserve 2,679,907 bytes of tail
        // (the English disc's 144 preserve 590,532).
        // the Italian disc carries no mesh, so nothing has to. Either way every
        // glTF that was written must keep its tail, and the byte count is only
        // asserted when there is a mesh to size.
        if with_tail != 0 {
            assert_eq!(
                tail_bytes, 2_679_907,
                "the undecoded tail size changed from the measured value"
            );
        }
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
        // Every asset is either a written file or a reported failure, so the
        // two counts must be the containers' own: 460 models plus the 781
        // textures the `DMA3D.MMX` offset table really holds. On the Italian
        // disc the run writes the 460 model payloads through (no payload carries
        // a mesh) and the textures whose layout fits, and reports the rest —
        // among them the 76 offsets entries that name a range the file does not
        // hold, which used to abort the process (`docs/DMA3D.md` §8.1, §11).
        assert_eq!(
            summary.models + summary.failed.len(),
            460 + 705,
            "{dir:?}: the run must account for every resource"
        );

        // ---- the index page, and every reference in it ----
        //
        // The page lists exactly what is on disk, and what is on disk differs
        // per disc: the English disc carries 144 mesh-bearing payloads and 705
        // textures, the Italian disc none and 3. So the expectations are derived
        // from the directories rather than written down, and the rule they
        // encode is fixed: every showable file appears, and nothing else does.
        let index = tmp.join("index.html");
        assert!(index.is_file(), "no index.html was written");
        let page = std::fs::read_to_string(&index).expect("read index.html");
        assert!(
            page.contains("<!DOCTYPE html>"),
            "the index is not an HTML document"
        );
        // No dependency: it has to open from a local folder with no network.
        assert!(
            !page.contains("http://") && !page.contains("https://")
                || page.contains("gltf-viewer"),
            "the index must not fetch anything except the offer of a viewer"
        );
        assert!(
            !page.contains("<script"),
            "the index must not need a script"
        );

        let model_dir = tmp.join("Models").join("DMA3D");
        let texture_dir = tmp.join("Textures").join("DMA3D");
        let count = |dir: &Path, suffix: &str| -> usize {
            std::fs::read_dir(dir)
                .map(|entries| {
                    entries
                        .flatten()
                        .filter(|e| {
                            e.file_name()
                                .to_string_lossy()
                                .ends_with(suffix)
                        })
                        .count()
                })
                .unwrap_or(0)
        };
        let objects = count(&model_dir, ".gltf");
        let previews = count(&model_dir, ".preview.png");
        let textures = count(&texture_dir, ".png");

        let references: Vec<String> = page
            .split('"')
            .filter(|s| s.starts_with("Models/") || s.starts_with("Textures/"))
            .map(|s| s.to_string())
            .collect();
        // Each figure is a link and an image. A texture's link and image are the
        // same path, so it occurs twice; an object's link is its `.gltf` and its
        // image is the preview beside it, which are two paths.
        //
        // So the distinct paths are the object's two plus each texture's one,
        // and the occurrences are each object's two plus each texture's two.
        // Asserting the pair is what keeps the page from silently dropping a
        // figure, and it holds on a disc whose object count is zero.
        let mut distinct = references.clone();
        distinct.sort();
        distinct.dedup();
        // Each figure is a link and an image. A texture's link and image are the
        // same path, so it occurs twice; an object's link is its `.gltf` and its
        // image is the preview beside it, which are two different paths. So the
        // distinct paths are the object's two plus each texture's one — which is
        // the rule that holds on a disc whose object count is anything.
        assert_eq!(
            distinct.len(),
            2 * objects + textures,
            "the index does not list every asset"
        );
        assert_eq!(
            distinct.iter().filter(|p| p.ends_with(".png")).count(),
            previews + textures,
            "the index does not list every picture"
        );
        assert_eq!(
            distinct.iter().filter(|p| p.ends_with(".gltf")).count(),
            objects,
            "the index does not link every model"
        );
        assert_eq!(
            references.len(),
            2 * objects + 2 * textures,
            "the index's markup does not reference every asset as expected"
        );
        assert_eq!(
            distinct
                .iter()
                .filter(|p| p.starts_with("Textures/"))
                .count(),
            textures,
            "the index does not list every texture"
        );
        for reference in &references {
            reference_count += 1;
            assert!(
                tmp.join(reference).is_file(),
                "the index refers to {reference}, which is not on disk"
            );
        }

        // ---- every object's preview covers pixels ----
        let model_dir = tmp.join("Models").join("DMA3D");
        let entries: Vec<std::path::PathBuf> = std::fs::read_dir(&model_dir)
            .expect("read the models directory")
            .flatten()
            .map(|e| e.path())
            .collect();
        // The number of glTFs and previews on disk is the number the page was
        // built from, so both are re-derived here rather than assumed: the
        // Italian disc carries no mesh and writes none.
        let gltfs = entries
            .iter()
            .filter(|p| p.file_name().unwrap().to_string_lossy().ends_with(".gltf"))
            .count();
        assert_eq!(gltfs, objects, "the page and the directory disagree on the objects");
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
        assert_eq!(checked, previews, "{dir:?}: not every preview was read back");
        assert!(
            blank.is_empty(),
            "{dir:?}: previews that cover no pixels: {blank:?}"
        );
        eprintln!(
            "{dir:?}: {} resources accounted for, index lists {} objects and {} textures, {reference_count} references, {checked} previews all covered",
            summary.models + summary.failed.len(),
            objects,
            textures,
        );
        discs += 1;
    }
    assert!(discs >= 1, "no disc was checked");
}
