//! Pin the counts that the doc comments state, so they cannot drift.
//!
//! `docs/DMA3D.md` and the module docs quote numbers -- how many payloads
//! carry a mesh, how many are UI records, how much tail is undecoded -- and a
//! number in a comment that no assertion checks is a number that rots. These
//! tests read the shipped containers and assert each figure. They skip when
//! the carved assets are absent, like the rest of the asset-dependent suite.

use mmf_extract::container::Container;
use mmf_extract::dma3d::modfile;
use mmf_extract::dma3d::texfile;

fn dma3d_dir() -> Option<std::path::PathBuf> {
    for candidate in [
        std::env::var("DMA3D_ASSETS").unwrap_or_default(),
        "/tmp/dma3d/pc".into(),
        "/tmp/en/dma3d_en".into(),
    ] {
        let path = std::path::PathBuf::from(candidate);
        if path.join("DMA3D.MMM").exists() && path.join("DMA3D.MMX").exists() {
            return Some(path);
        }
    }
    None
}

/// The two containers' resource counts, which every other figure is derived
/// from.
#[test]
fn the_two_containers_hold_the_measured_resource_counts() {
    let Some(dir) = dma3d_dir() else {
        eprintln!("skipping: the 3D game's assets are not present");
        return;
    };
    let models = Container::open(dir.join("DMA3D.MMM")).expect("open Models");
    let textures = Container::open(dir.join("DMA3D.MMX")).expect("open Textures");
    assert_eq!(models.resource_count, 460);
    assert_eq!(textures.resource_count, 705);
}

/// 144 payloads carry the mesh chain, and none of them is a UI record.
#[test]
fn the_payloads_that_carry_a_mesh_are_counted() {
    let Some(dir) = dma3d_dir() else {
        eprintln!("skipping: the 3D game's assets are not present");
        return;
    };
    let models = Container::open(dir.join("DMA3D.MMM")).expect("open Models");
    let n = models.resource_count as usize;
    let mut meshed = 0usize;
    let mut ui = 0usize;
    let mut ui_named = 0usize;
    let mut meshed_ui = 0usize;
    for i in 0..n {
        let name = models.name(i);
        if name.starts_with("createPropUI") {
            // The 276-byte UI twin records, which hold no mesh (§3.4).
            ui += 1;
        }
        if name.contains("UI") {
            // The wider class the shared state note counts: the UI twin
            // records *plus* the UI preview shapes such as
            // `createModDrawProfileUIoct.mod`, which do carry meshes.
            ui_named += 1;
        }
        if modfile::parse_mesh(models.payload(i)).is_ok() {
            meshed += 1;
            if name.starts_with("createPropUI") {
                meshed_ui += 1;
            }
        }
    }
    assert_eq!(ui, 42, "the createPropUI twin-record count changed");
    assert_eq!(ui_named, 64, "the UI-named payload count changed");
    assert_eq!(meshed, 144, "the mesh-bearing payload count changed");
    assert_eq!(
        meshed_ui, 0,
        "a createPropUI twin record now carries a mesh"
    );
    eprintln!(
        "{n} payloads: {meshed} carry a mesh, {ui} are createPropUI twins, \
         {ui_named} are UI-named"
    );
}

/// Every texture decodes, and no decoded palette is longer than the set of
/// indices it is used for -- the strongest single piece of evidence for the
/// split, because a wrong split leaves dead entries.
#[test]
fn every_texture_decodes_and_no_palette_entry_is_dead() {
    let Some(dir) = dma3d_dir() else {
        eprintln!("skipping: the 3D game's assets are not present");
        return;
    };
    let textures = Container::open(dir.join("DMA3D.MMX")).expect("open Textures");
    let n = textures.resource_count as usize;
    let mut square = 0usize;
    let mut wide = 0usize;
    let mut tall = 0usize;
    for i in 0..n {
        let t = texfile::parse(textures.payload(i))
            .unwrap_or_else(|e| panic!("{}: {e}", textures.name(i)));
        let used = 1 + t.indices.iter().copied().max().unwrap_or(0) as usize;
        assert_eq!(
            used,
            t.palette.len(),
            "{}: {} palette entries but the indices only use {}",
            textures.name(i),
            t.palette.len(),
            used
        );
        let area = t.width * t.height;
        assert!(
            area.is_power_of_two(),
            "{}: {area} is not a power of two",
            textures.name(i)
        );
        match t.axis {
            texfile::Axis::Square => square += 1,
            texfile::Axis::Wide => wide += 1,
            texfile::Axis::Tall => tall += 1,
        }
    }
    eprintln!("{n} textures: {square} square, {wide} wide, {tall} tall");
    assert_eq!(square + wide + tall, n);
    // The 27 rectangles are the ones whose orientation is decided by a measured
    // statistic rather than known outright. If that count moves, the statistic
    // or the format changed and §2.2 of the doc needs rewriting.
    assert_eq!(wide + tall, 27, "the rectangle count changed");
}

/// The undecoded tail of the 144 mesh payloads, in bytes.
///
/// This is the size of the hole in the modern output: the animation and the
/// texture coordinates live in it. It is asserted so that shrinking it is
/// visible as a change in this number rather than an unremarked edit.
#[test]
fn the_undecoded_tail_of_the_mesh_payloads_is_sized() {
    let Some(dir) = dma3d_dir() else {
        eprintln!("skipping: the 3D game's assets are not present");
        return;
    };
    let models = Container::open(dir.join("DMA3D.MMM")).expect("open Models");
    let mut tail = 0usize;
    let mut meshed = 0usize;
    for i in 0..models.resource_count as usize {
        if let Ok(mesh) = modfile::parse_mesh(models.payload(i)) {
            meshed += 1;
            tail += models.payload(i).len() - mesh.tail_offset();
        }
    }
    assert_eq!(meshed, 144);
    assert_eq!(tail, 590_532, "the undecoded tail size changed");
    eprintln!("540532-byte check: {tail} bytes of tail over {meshed} payloads");
}

/// The tail header, measured rather than assumed.
///
/// The doc once said the tail opens `u32 0, u32 1, u32 V` "in all 42 static
/// props". That is wrong, and narrowing the parser to *require* the shape is
/// what proved it: the requirement silently dropped 144 payloads to 94, so 50
/// real meshes were being discarded. The parser now reads the words and records
/// them (`ModelMesh::tail_header`) without demanding them, and this test pins
/// the real distribution so the doc cannot drift back.
///
/// The 50 that lack it are the small tool shapes — the draw profiles, magnets,
/// knobs and sliders — whose tails are almost entirely zeros.
#[test]
fn the_tail_header_is_a_measurement_not_a_rule() {
    let Some(dir) = dma3d_dir() else {
        eprintln!("skipping: the 3D game's assets are not present");
        return;
    };
    let models = Container::open(dir.join("DMA3D.MMM")).expect("open Models");
    let mut shape = std::collections::BTreeMap::new();
    for i in 0..models.resource_count as usize {
        let payload = models.payload(i);
        let Ok(mesh) = modfile::parse_mesh(payload) else {
            continue;
        };
        let key = match mesh.tail_header {
            Some((0, 1, v)) if v as usize == mesh.vertex_count() => "[0, 1, V]",
            Some((0, 0, 0)) => "[0, 0, 0]",
            Some(_) => "other",
            None => "too short",
        };
        *shape.entry(key).or_insert(0usize) += 1;
    }
    eprintln!("tail shapes over the mesh payloads: {shape:?}");
    assert_eq!(shape["[0, 1, V]"], 94, "the [0, 1, V] count changed");
    assert_eq!(shape["[0, 0, 0]"], 49, "the [0, 0, 0] count changed");
    assert_eq!(shape["other"], 1, "the third shape count changed");
    assert_eq!(
        shape.values().sum::<usize>(),
        144,
        "the mesh-bearing payload count changed"
    );
}

/// The orientation of the 21 rectangles whose width and height could be
/// established independently.
///
/// These come from `/workspace/.tmp/TEXTURE-ORIENTATION.md`, where each is
/// decided by at least two agreeing measurements — the connected-component
/// count (at the correct width an icon is one object; at the wrong one its rows
/// have been concatenated and it is sliced), the zero-cut count, the mean
/// vertical index difference, and the rendered picture. `introAxSplash` is
/// decided beyond doubt because its 256×128 rendering spells the game title.
///
/// The rule in `texfile::decide` reproduces every one of them, and this test is
/// what keeps it that way. Five are open and are *not* listed here, because
/// listing a guess as a fact is exactly what the whole orientation question is
/// about avoiding.
#[test]
fn the_measured_orientations_are_reproduced() {
    let Some(dir) = dma3d_dir() else {
        eprintln!("skipping: the 3D game's assets are not present");
        return;
    };
    let textures = Container::open(dir.join("DMA3D.MMX")).expect("open Textures");

    let measured: &[(&str, usize, usize)] = &[
        ("CreationTex.TGA", 64, 32),
        ("DuplicateTex.TGA", 64, 32),
        ("NewTex.TGA", 64, 32),
        ("beewings.1.TGA", 32, 16),
        ("beewings.2.TGA", 32, 16),
        ("beewings.3.TGA", 32, 16),
        ("createBtUndo.1.TGA", 32, 64),
        ("createBtUndo.2.TGA", 32, 64),
        ("createBtUndo.3.TGA", 32, 64),
        ("createModMickeyPartArm.TGA", 64, 128),
        ("createModMickeyPartArmL.TGA", 64, 128),
        ("createModMickeyPartArmR.TGA", 64, 128),
        ("createModMickeyPartHead.TGA", 64, 128),
        ("createModMickeyPartHeadS.TGA", 64, 128),
        ("createModMickeyPartLegL.TGA", 64, 128),
        ("createModMickeyPartLegR.TGA", 64, 128),
        ("createModMickeyPartTorso.TGA", 64, 128),
        ("createModMickeyParts.TGA", 64, 128),
        // Also the only one with an independent external check: the mesh that
        // shares its name is flat with an aspect of 0.6221, which is closer to
        // 1:2 tall than to 2:1.
        ("createModMickeySlider.TGA", 64, 128),
        // Its 256×128 rendering spells the game title; the 128×256 one is
        // unreadable. The strongest single piece of evidence in the report.
        ("introAxSplash.TGA", 256, 128),
        ("shrBkLeft1.TGA", 128, 256),
    ];

    let mut checked = 0usize;
    for (name, expect_w, expect_h) in measured.iter() {
        let found = (0..textures.resource_count as usize)
            .find(|&i| textures.name(i) == *name)
            .unwrap_or_else(|| panic!("{name} is not in the Textures container"));
        let texture = texfile::parse(textures.payload(found))
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(
            (texture.width, texture.height),
            (*expect_w, *expect_h),
            "{name}: measured {expected}x{expected_h}, decoder says {width}x{height}",
            expected = expect_w,
            expected_h = expect_h,
            width = texture.width,
            height = texture.height
        );
        assert!(!texture.undecidable, "{name}: the decoder reports it undecidable");
        checked += 1;
    }
    assert_eq!(checked, 21, "not every measured orientation was checked");
}

/// The rectangle count and its shapes, pinned.
///
/// An earlier version of `axis_of` considered every power of two that divided
/// the area, and so read `beestripes` as 8×64 and `createBtFileIo` as 8×256 —
/// 1:8 and 1:32, which contradicted the premise that the engine's rectangles
/// are 1:2. All 21 independently-established orientations are 1:2, so the
/// candidate set is that pair, and this test is what keeps it.
#[test]
fn every_rectangle_is_one_to_two() {
    let Some(dir) = dma3d_dir() else {
        eprintln!("skipping: the 3D game's assets are not present");
        return;
    };
    let textures = Container::open(dir.join("DMA3D.MMX")).expect("open Textures");
    let mut rectangles = 0usize;
    let mut shapes: std::collections::BTreeMap<(usize, usize), usize> =
        std::collections::BTreeMap::new();
    for i in 0..textures.resource_count as usize {
        let texture = texfile::parse(textures.payload(i))
            .unwrap_or_else(|e| panic!("{}: {e}", textures.name(i)));
        if texture.axis == texfile::Axis::Square {
            continue;
        }
        rectangles += 1;
        *shapes
            .entry((texture.width, texture.height))
            .or_insert(0usize) += 1;
        let (wide, tall) = if texture.width > texture.height {
            (texture.width, texture.height)
        } else {
            (texture.height, texture.width)
        };
        assert_eq!(
            wide,
            tall * 2,
            "{}: {}x{} is not 1:2",
            textures.name(i),
            texture.width,
            texture.height
        );
    }
    assert_eq!(rectangles, 27, "the rectangle count changed");
    eprintln!("the 27 rectangles by shape: {shapes:?}");
}

/// The one texture the payload cannot orient, reported rather than guessed.
///
/// `createBtFileIo.TGA` has `K == 1`: one palette entry and 2048 identical index
/// bytes. Every statistic is degenerate at every width, so no reading differs
/// from any other and any choice would be a guess wearing a measurement's
/// clothes. It is flagged, not hidden.
#[test]
fn the_undecidable_texture_is_flagged() {
    let Some(dir) = dma3d_dir() else {
        eprintln!("skipping: the 3D game's assets are not present");
        return;
    };
    let textures = Container::open(dir.join("DMA3D.MMX")).expect("open Textures");
    let found = (0..textures.resource_count as usize)
        .find(|&i| textures.name(i) == "createBtFileIo.TGA")
        .expect("createBtFileIo.TGA is in the Textures container");
    let texture = texfile::parse(textures.payload(found)).expect("parse");
    assert_eq!(texture.palette.len(), 1, "K is not 1 any more");
    assert!(
        texture.undecidable,
        "an all-identical index block must be reported as undecidable"
    );

    // And no other texture is: the flag is for the case where the data says
    // nothing, not for the case where the rule is unsure.
    let mut flagged = Vec::new();
    for i in 0..textures.resource_count as usize {
        let texture = texfile::parse(textures.payload(i))
            .unwrap_or_else(|e| panic!("{}: {e}", textures.name(i)));
        if texture.undecidable {
            flagged.push(textures.name(i).to_string());
        }
    }
    assert_eq!(flagged, vec!["createBtFileIo.TGA".to_string()]);
}
