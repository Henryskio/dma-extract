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
