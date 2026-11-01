//! Print what the mesh parser located and what lies after it.
//!
//! Scratch bin. The mesh decoder is measured (`src/dma3d/modfile.rs`); the bytes
//! from `tail_offset()` onward are not, and this prints them so a candidate
//! layout can be checked against the shipped payloads instead of against one
//! file remembered by hand.
//!
//! ```text
//! modinfo <DMA3D.MMM> [name-substring] [--tail N]
//! modinfo <DMA3D.MMM> --tile            test counted-record chains
//! ```
//!
//! `--tile` asks one specific question: for which record size `r` does the
//! chain `[u32 count][count * r]` walk from `tail_offset()` to the end of the
//! payload exactly? A size that tiles every mesh payload is a layout; one that
//! tiles only some is a coincidence.
//!
//! Offsets come from `modfile::parse_mesh` rather than being re-derived here,
//! so a change to the mesh decoder cannot silently desynchronise this bin.

use mmf_extract::container::Container;
use mmf_extract::dma3d::modfile;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("usage: modinfo <DMA3D.MMM> [name] [--tail N] | --tile");
    let mut filter = String::new();
    let mut tail_bytes = 96usize;
    let mut tile = false;
    for arg in args {
        if let Some(n) = arg.strip_prefix("--tail ") {
            tail_bytes = n.parse().expect("--tail takes a byte count");
        } else if arg == "--tile" {
            tile = true;
        } else if arg.starts_with("--") {
            continue;
        } else {
            filter = arg;
        }
    }

    let data = std::fs::read(&path).expect("read container");
    let container = Container::parse(&path, data).expect("open Models");
    let n = container.resource_count as usize;

    for index in 0..n {
        let name = container.name(index);
        if !filter.is_empty() && !name.contains(&filter) {
            continue;
        }
        let payload = container.payload(index);
        let Ok(mesh) = modfile::parse_mesh(payload) else {
            continue;
        };
        let tail = mesh.tail_offset();
        println!(
            "#{index:03} {name}  len={}  C={}  V={}  mesh@{:#x}  tail@{:#x} ({} bytes)",
            payload.len(),
            mesh.indices.len(),
            mesh.vertex_count(),
            mesh.index_count_offset,
            tail,
            payload.len() - tail
        );

        // The tail as four readings side by side, because the record kind is
        // what is unknown.
        let region = &payload[tail..(tail + tail_bytes).min(payload.len())];
        let hex = region
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join(" ");
        println!("     hex  {hex}");
        let u32s = region
            .chunks_exact(4)
            .take(8)
            .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join(" ");
        println!("     u32  {u32s}");
        let f32s = region
            .chunks_exact(4)
            .take(8)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .map(|v| format!("{v:.5}"))
            .collect::<Vec<_>>()
            .join(" ");
        println!("     f32  {f32s}");
        let u16s = region
            .chunks_exact(2)
            .take(12)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join(" ");
        println!("     u16  {u16s}");
        println!();
    }

    // ---- `--tile`: which record size walks the whole tail ----
    if tile {
        // Record sizes to test, in bytes. The animation classes in
        // `DMA3D.exe`'s RTTI (`MM3DFunctionCurve`, `MM3DLinearFunctionCurve`,
        // `MM3DBezierSpline`, `MM3DJointAnimation`, `MM3DSRTAnimation`) say a
        // curve's samples are `f32`, so these are one, two, three, four and
        // six floats, plus a size in u16s.
        let sizes = [
            ("1 f32", 4usize),
            ("2 f32", 8),
            ("3 f32", 12),
            ("4 f32", 16),
            ("6 f32", 24),
            ("2 u16", 4),
            ("4 u16", 8),
        ];
        let mut meshes = 0usize;
        // How many payloads each size tiles exactly to the end.
        let mut exact = vec![0usize; sizes.len()];
        let mut near = vec![0usize; sizes.len()];
        for index in 0..n {
            let payload = container.payload(index);
            let Ok(mesh) = modfile::parse_mesh(payload) else {
                continue;
            };
            meshes += 1;
            let tail = &payload[mesh.tail_offset()..];
            for (k, (label, size)) in sizes.iter().enumerate() {
                let mut at = 0usize;
                let mut blocks = 0usize;
                let mut ok = true;
                while at < tail.len() {
                    if at + 4 > tail.len() {
                        ok = false;
                        break;
                    }
                    let count = u32::from_le_bytes([
                        tail[at],
                        tail[at + 1],
                        tail[at + 2],
                        tail[at + 3],
                    ]) as usize;
                    // A count that overruns the tail, or a run of empty
                    // records that consumes nothing, is not a chain.
                    if count == 0 || at + 4 + count * size > tail.len() {
                        ok = false;
                        break;
                    }
                    at += 4 + count * size;
                    blocks += 1;
                }
                if ok && at == tail.len() {
                    exact[k] += 1;
                } else if ok {
                    near[k] += 1;
                }
                let _ = (label, blocks);
            }
        }
        println!("{meshes} mesh payloads, tails tiled by [u32 count][count * r]:");
        for (k, (label, size)) in sizes.iter().enumerate() {
            println!(
                "  r = {size:3} bytes ({label:5})  exact {exact}/{total}  stopped-early {near}",
                exact = exact[k],
                near = near[k],
                total = meshes
            );
        }
    }
}
