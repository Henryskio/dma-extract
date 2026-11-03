//! Count how many 3D model payloads carry a mesh, and how many do not.
//!
//! Scratch bin. `src/dma3d.rs`'s doc comment claims 87 of the 460 payloads
//! carry the measured mesh chain. This verifies that number directly, because
//! a claim in a doc comment that no test checks is a claim that can drift.

use mmf_extract::container::Container;
use mmf_extract::dma3d::modfile;

fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/dma3d/pc/DMA3D.MMM".into());
    let raw = std::fs::read(&path).expect("read container");
    let c = Container::parse(&path, raw).expect("open Models");
    let n = c.resource_count as usize;
    let mut mesh = 0usize;
    let mut no_mesh = 0usize;
    let mut ui = 0usize;
    let mut mesh_ui = 0usize;
    let mut tail_bytes = 0usize;
    let mut names_no_mesh = Vec::new();
    for i in 0..n {
        let name = c.name(i).to_string();
        let is_ui = name.contains("UI");
        if is_ui {
            ui += 1;
        }
        match modfile::parse_mesh(c.payload(i)) {
            Ok(m) => {
                mesh += 1;
                if is_ui {
                    mesh_ui += 1;
                }
                tail_bytes += c.payload(i).len() - m.tail_offset();
            }
            Err(_) => {
                no_mesh += 1;
                if names_no_mesh.len() < 10 {
                    names_no_mesh.push(format!("{} ({} B)", name, c.payload_len(i)));
                }
            }
        }
    }
    println!("N = {n}");
    println!("payloads carrying a mesh: {mesh}");
    println!("payloads with no mesh:     {no_mesh}");
    println!("UI-named payloads:         {ui}  (of those with a mesh: {mesh_ui})");
    println!("undecoded tail bytes over the mesh payloads: {tail_bytes}");
    println!("sample of the no-mesh payloads: {names_no_mesh:?}");
}
