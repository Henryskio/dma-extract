//! Print what the mesh parser located and what lies after it.
//!
//! Scratch bin. The mesh decoder is measured (`src/dma3d/modfile.rs`); the bytes
//! from `tail_offset()` onward are not, and this prints them so a candidate
//! layout can be checked against the shipped payloads instead of against one
//! file remembered by hand.
//!
//! ```text
//! modinfo <DMA3D.MMM> [NAME] [--tail N]
//! modinfo <DMA3D.MMM> --tile            test counted-record chains
//! ```
//!
//! `--tile` asks one specific question: for which record size `r` does the
//! chain `[u32 count][count * r]` walk from **after** the header to the end of
//! the payload exactly? It starts past `[0, 1, V]` and the `V` float pairs that
//! follow it, because starting at `tail_offset()` itself always fails: the head
//! word is `0`, and a zero count is not a record. A size that tiles every mesh
//! payload is a layout; one that tiles only some is a coincidence.
//!
//! Offsets come from `modfile::parse_mesh` rather than being re-derived here,
//! so a change to the mesh decoder cannot silently desynchronise this bin.
//!
//! The arguments are parsed by `clap`, the same crate the binary and the other
//! scratch bins use: an earlier version of this bin walked `std::env::args` by
//! hand, and reading `--tail 420` as one word silently turned the count into a
//! name filter that matched nothing.

use clap::Parser;
use mmf_extract::container::Container;
use mmf_extract::dma3d::modfile;

#[derive(Parser, Debug)]
#[command(
    name = "modinfo",
    about = "Print where the .mod mesh parser finds the mesh, and what lies after it",
    long_about = "For each payload that carries the measured mesh chain, prints the \
index count, the vertex count, where the chain starts and where the undecoded tail \
begins, then the tail as hex, u32, f32 and u16 side by side. Offsets come from \
modfile::parse_mesh, so they cannot drift from the decoder."
)]
struct Args {
    /// The Models container to read.
    #[arg(value_name = "CONTAINER")]
    path: std::path::PathBuf,

    /// Only payloads whose resource name contains this text.
    #[arg(value_name = "NAME", default_value = "")]
    name: String,

    /// How many bytes of each tail to print.
    #[arg(long, value_name = "N", default_value = "96")]
    tail: usize,

    /// Instead of printing payloads, test which record size tiles every tail.
    #[arg(long)]
    tile: bool,
}

fn main() {
    let args = Args::parse();
    let path = args.path;
    let filter = args.name;
    let tail_bytes = args.tail;
    let tile = args.tile;

    let data = std::fs::read(&path).expect("read container");
    let container = Container::parse(&path, data).expect("open Models");
    let n = container.resource_count as usize;

    for index in 0..n {
        let name = container.name(index);
        if !filter.is_empty() && !name.contains(&filter) {
            continue;
        }
        if tile {
            // `--tile` is its own report; the per-resource dump would bury it.
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

    // ---- `--tile`: what the section after the pairs looks like ----
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
        let mut empties = 0usize;
        // How many payloads each size tiles exactly to the end of the payload.
        let mut exact = vec![0usize; sizes.len()];
        let mut stopped = vec![0usize; sizes.len()];
        // The first `u32` after the `V` pairs, which is where an animation
        // record count would live. Its distribution is the measurement: on
        // `createPropGuitar.mod` it is 3, and the payload names three tracks'
        // worth of keyframes and six track names.
        let mut first_word: std::collections::BTreeMap<u32, usize> =
            std::collections::BTreeMap::new();
        for index in 0..n {
            let payload = container.payload(index);
            let Ok(mesh) = modfile::parse_mesh(payload) else {
                continue;
            };
            meshes += 1;
            let head = mesh.tail_offset() + 12 + 8 * mesh.vertex_count();
            if head >= payload.len() {
                empties += 1;
                continue;
            }
            let tail = &payload[head..];
            let count = u32::from_le_bytes([tail[0], tail[1], tail[2], tail[3]]);
            *first_word.entry(count).or_insert(0usize) += 1;
            for (k, (_, size)) in sizes.iter().enumerate() {
                let mut at = 0usize;
                let mut ok = true;
                while at < tail.len() {
                    if at + 4 > tail.len() {
                        ok = false;
                        break;
                    }
                    let n = u32::from_le_bytes([
                        tail[at],
                        tail[at + 1],
                        tail[at + 2],
                        tail[at + 3],
                    ]) as usize;
                    // A count that overruns the tail, or a run of empty records
                    // that consumes nothing, is not a chain.
                    if n == 0 || at + 4 + n * size > tail.len() {
                        ok = false;
                        break;
                    }
                    at += 4 + n * size;
                }
                if ok && at == tail.len() {
                    exact[k] += 1;
                } else if ok {
                    stopped[k] += 1;
                }
            }
        }
        println!(
            "{meshes} mesh payloads, {empties} of them with nothing after the pairs; \\
             the walk starts after [0,1,V] and the V float pairs"
        );
        for (k, (label, size)) in sizes.iter().enumerate() {
            println!(
                "  r = {size:3} bytes ({label:5})  exact {}/{}  stopped-early {}",
                exact[k], meshes, stopped[k]
            );
        }
        println!("\n  no record size tiles any tail, so a flat counted array is not the");
        println!("  layout. The first u32 after the V pairs, across all payloads:");
        for (word, count) in &first_word {
            println!("    {word:6}  x{count}");
        }
    }
}
