//! Walk the animation region with the grammar the records suggest.
//!
//! Scratch bin. Reading `createBtCameraCenter.mod` past the `[0, 1, V]` header
//! and its `V` float pairs shows this, and it repeats:
//!
//! ```text
//! u32 K            how many index entries follow
//! u16[K]           vertex indices, all < V, with repetition
//! then, per frame:
//!   u32 D          how many distinct vertices this frame names
//!   f32[D]         one value per distinct vertex
//!   vec3[D]        a three-float row per distinct vertex
//! ```
//!
//! This walks that over every payload and reports whether it consumes the
//! payload exactly. Walking to the end is what turns "this shape recurs" into
//! "this is the layout"; a payload the walk cannot finish is a payload the
//! grammar does not describe, and those are counted rather than hidden.
//!
//! Offsets come from `modfile::parse_mesh`.

use clap::Parser;
use mmf_extract::container::Container;
use mmf_extract::dma3d::modfile;

#[derive(Parser, Debug)]
#[command(
    name = "animwalk",
    about = "Walk each .mod payload's animation region with the record grammar"
)]
struct Args {
    /// The Models container to read.
    #[arg(value_name = "CONTAINER")]
    path: std::path::PathBuf,

    /// Only payloads whose resource name contains this text.
    #[arg(value_name = "NAME", default_value = "")]
    name: String,

    /// Walk every payload and report the outcome distribution.
    #[arg(long)]
    all: bool,
}

/// How a payload's animation region walked.
struct Walk {
    /// How many frames were consumed.
    frames: usize,
    /// Whether the walk ended exactly at the payload's end.
    exact: bool,
    /// Where the walk stopped.
    stopped_at: usize,
    /// The payload's length.
    length: usize,
    /// The first frame's distinct count, if the walk got that far.
    first_d: Option<usize>,
}

/// Walk from `start`, or report where the walk failed.
///
/// The vertex count is not needed any more -- the section grammar's counts are
/// read from the stream, not derived from the mesh -- but it is kept in the
/// signature because the walk is anchored to the decoded mesh's offsets and a
/// caller that has them should pass them.
fn walk(payload: &[u8], start: usize, _vertex_count: usize) -> Walk {
    let length = payload.len();
    let mut at = start;
    let mut frames = 0usize;
    let mut first_d = None;
    loop {
        if at + 4 > length {
            return Walk { frames, exact: at == length, stopped_at: at, length, first_d };
        }
        let d = u32::from_le_bytes([payload[at], payload[at + 1], payload[at + 2], payload[at + 3]])
            as usize;
        // A frame with no vertices consumes only its count, and a frame that
        // runs off the end is not a frame.
        if d == 0 || at + 4 + 4 * d + 12 * d > length {
            return Walk { frames, exact: false, stopped_at: at, length, first_d };
        }
        if first_d.is_none() {
            first_d = Some(d);
        }
        at += 4 + 16 * d;
        frames += 1;
    }
}

fn main() {
    let args = Args::parse();
    let data = std::fs::read(&args.path).expect("read container");
    let container = Container::parse(&args.path, data).expect("open Models");
    let n = container.resource_count as usize;

    let mut exact = 0usize;
    let mut inexact = 0usize;
    let mut total = 0usize;
    let mut frame_counts: std::collections::BTreeMap<usize, usize> =
        std::collections::BTreeMap::new();
    let mut examples: Vec<(String, Walk)> = Vec::new();

    for index in 0..n {
        let name = container.name(index);
        if !args.name.is_empty() && !name.contains(&args.name) {
            continue;
        }
        let payload = container.payload(index);
        let Ok(mesh) = modfile::parse_mesh(payload) else {
            continue;
        };
        total += 1;
        let v = mesh.vertex_count();
        // The animation region starts after the `[0, 1, V]` header and the `V`
        // float pairs, when the payload carries that shape. When it does not,
        // the tail offset itself is where the records would start.
        let start = if mesh.tail_header.map(|t| t.2 as usize == v).unwrap_or(false) {
            mesh.tail_offset() + 12 + 8 * v
        } else {
            mesh.tail_offset()
        };
        if start >= payload.len() {
            inexact += 1;
            continue;
        }

        // The index list that names the affected vertices, which precedes the
        // frames.
        let mut at = start;
        let mut named = None;
        if at + 4 <= payload.len() {
            let k = u32::from_le_bytes([
                payload[at],
                payload[at + 1],
                payload[at + 2],
                payload[at + 3],
            ]) as usize;
            if (1..=100_000).contains(&k) && at + 4 + 2 * k <= payload.len() {
                let mut distinct = std::collections::HashSet::new();
                let mut ok = true;
                for j in 0..k {
                    let value = u16::from_le_bytes([payload[at + 4 + 2 * j], payload[at + 5 + 2 * j]]);
                    if usize::from(value) >= v {
                        ok = false;
                        break;
                    }
                    distinct.insert(value);
                }
                if ok {
                    named = Some(distinct.len());
                    at += 4 + 2 * k;
                }
            }
        }

        let result = walk(payload, at, v);
        if result.exact {
            exact += 1;
        } else {
            inexact += 1;
        }
        if let Some(d) = result.first_d {
            *frame_counts.entry(d).or_insert(0usize) += 1;
        }
        if args.all {
            println!(
                "#{index:03} {name:32} named={named:?} {} frames, {}",
                result.frames,
                if result.exact {
                    "walked to the end"
                } else {
                    "stopped short"
                }
            );
        }
        if examples.len() < 10 {
            examples.push((name.to_string(), result));
        }
    }

    println!(
        "\n{total} mesh payloads: {exact} walked exactly to the end, {inexact} did not"
    );
    println!("  the first frame's distinct count, across payloads:");
    for (d, count) in frame_counts.iter().take(12) {
        println!("    D = {d:5}  x{count}");
    }
    println!("\n  how the walk went (name, frames, whether it finished):");
    for (name, result) in &examples {
        println!(
            "    {name:32} {:4} frames, {} bytes of {}",
            result.frames,
            result.stopped_at,
            result.length
        );
    }
}
