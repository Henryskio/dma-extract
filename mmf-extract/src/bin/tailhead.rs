//! What the mesh tails open with, across every payload that carries a chain.
//!
//! Scratch bin. `modfile::parse_mesh` reads the three words after the normal
//! array when they are there, but does not require them, because an earlier
//! version of the parser did require the `[0, 1, V]` shape and silently
//! dropped 50 of the 144 payloads. This counts what is actually there, so the
//! claim in `docs/DMA3D.md` §3.3 is a measurement rather than a hope.

use mmf_extract::container::Container;
use mmf_extract::dma3d::modfile;

fn main() {
    let path = std::env::args().nth(1).expect("usage: tailhead <DMA3D.MMM>");
    let data = std::fs::read(&path).expect("read container");
    let container = Container::parse(&path, data).expect("open Models");
    let n = container.resource_count as usize;

    let mut with = 0usize;
    let mut without = 0usize;
    let mut mismatched_v = 0usize;
    let mut other: Vec<(String, (u32, u32, u32), usize)> = Vec::new();
    for index in 0..n {
        let payload = container.payload(index);
        let Ok(mesh) = modfile::parse_mesh(payload) else {
            continue;
        };
        match mesh.tail_header {
            Some((0, 1, v)) if v as usize == mesh.vertex_count() => with += 1,
            Some(triple) => {
                without += 1;
                if triple.2 as usize != mesh.vertex_count() {
                    mismatched_v += 1;
                }
                if other.len() < 12 {
                    other.push((container.name(index).to_string(), triple, mesh.vertex_count()));
                }
            }
            None => without += 1,
        }
    }
    println!("payloads with a mesh: {}", with + without);
    println!("  tail opens [0, 1, V] with V == their vertex count: {with}");
    println!("  tail opens something else, or too short to hold it:      {without}");
    println!("  of those, the third word is not the vertex count:        {mismatched_v}");
    // The distinct shapes the other tails open with, so the 50 are grouped
    // rather than listed.
    let mut shapes: std::collections::BTreeMap<(u32, u32, u32), usize> =
        std::collections::BTreeMap::new();
    for index in 0..n {
        let payload = container.payload(index);
        let Ok(mesh) = modfile::parse_mesh(payload) else {
            continue;
        };
        let key = match mesh.tail_header {
            Some((0, 1, v)) if v as usize == mesh.vertex_count() => continue,
            other => other.unwrap_or((u32::MAX, u32::MAX, u32::MAX)),
        };
        *shapes.entry(key).or_insert(0usize) += 1;
    }
    println!("\nthe distinct shapes the other tails open with:");
    for (shape, count) in &shapes {
        println!("  {shape:?}  x{count}");
    }
    println!("\nthe other shapes (name, first three words, V):");
    for (name, triple, v) in &other {
        println!("  {name:32} {triple:?}  V={v}");
    }
}
