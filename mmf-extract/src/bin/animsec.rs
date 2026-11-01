//! Walk the animation region as a sequence of counted sections.
//!
//! Scratch bin. Reading `createBtCameraCenter.mod` past the `[0, 1, V]` header
//! shows a shape that explains everything measured so far:
//!
//! ```text
//! 0x2365  [0, 1, 255]   a kind and a subtype, read once
//! 0x2371  255 x (2 f32)  the pairs
//! 0x2b69  [u32 177]     177 x u16   vertex indices
//! 0x2ccf  [u32 131]     131 x 16B   a float and a vec3 each
//! 0x3503  [u32 131]     131 x 16B
//! 0x3d37  [0, 1, 131]   ... and the header appears again
//! ```
//!
//! So the region may be a chain of sections, each `[u32 n][n * s]` with a
//! section-specific record size. This tries every record size at each section,
//! with backtracking, and reports whether any assignment consumes the payload
//! exactly. An assignment that closes the payload is a layout; none closing is
//! the evidence that the model is still wrong.
//!
//! Offsets come from `modfile::parse_mesh`.

use clap::Parser;
use mmf_extract::container::Container;
use mmf_extract::dma3d::modfile;

#[derive(Parser, Debug)]
#[command(
    name = "animsec",
    about = "Walk each .mod payload's animation region as counted sections of a fixed record size"
)]
struct Args {
    /// The Models container to read.
    #[arg(value_name = "CONTAINER")]
    path: std::path::PathBuf,

    /// Only payloads whose resource name contains this text.
    #[arg(value_name = "NAME", default_value = "")]
    name: String,
}

/// Record sizes to try, in bytes.
const SIZES: [usize; 6] = [2, 4, 8, 12, 16, 24];

/// Read the `u32` at `at`.
fn word(payload: &[u8], at: usize) -> Option<usize> {
    if at + 4 > payload.len() {
        return None;
    }
    Some(u32::from_le_bytes([
        payload[at],
        payload[at + 1],
        payload[at + 2],
        payload[at + 3],
    ]) as usize)
}

/// Walk from `start`, trying every record size at each section.
///
/// Returns the sections consumed when the walk reaches the end exactly, or
/// `None` if no assignment does. `steps` bounds the search so a payload whose
/// sections are deeply nested cannot run forever.
fn walk(payload: &[u8], start: usize, steps: usize) -> Option<Vec<(usize, usize, usize)>> {
    fn recurse(
        payload: &[u8],
        at: usize,
        steps: usize,
        found: &mut Vec<(usize, usize, usize)>,
    ) -> bool {
        if at == payload.len() {
            return true;
        }
        if at > payload.len() || steps == 0 {
            return false;
        }
        let n = match word(payload, at) {
            Some(n) if n <= payload.len() / 2 => n,
            // A count larger than half the payload cannot be a record count of
            // anything that follows, so it is not a section head.
            _ => return false,
        };
        for &size in SIZES.iter() {
            let end = at + 4 + n * size;
            if end > payload.len() {
                continue;
            }
            found.push((at, n, size));
            if recurse(payload, end, steps - 1, found) {
                return true;
            }
            found.pop();
        }
        false
    }
    let mut found = Vec::new();
    if recurse(payload, start, steps, &mut found) {
        Some(found)
    } else {
        None
    }
}

fn main() {
    let args = Args::parse();
    let data = std::fs::read(&args.path).expect("read container");
    let container = Container::parse(&args.path, data).expect("open Models");
    let n = container.resource_count as usize;

    let mut closed = 0usize;
    let mut total = 0usize;
    // The record sizes each closed walk used, in order, so a repeating cycle
    // can be seen.
    let mut cycles: std::collections::BTreeMap<Vec<usize>, usize> =
        std::collections::BTreeMap::new();
    let mut examples: Vec<(String, Vec<(usize, usize, usize)>)> = Vec::new();

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
        // The animation region starts after the `[0, 1, V]` header and its `V`
        // float pairs. A payload without that shape has no known start.
        let start = mesh.tail_offset() + 12 + 8 * v;
        if start > payload.len() {
            continue;
        }
        if let Some(sections) = walk(payload, start, 4096) {
            closed += 1;
            let sizes: Vec<usize> = sections.iter().map(|(_, _, s)| *s).collect();
            *cycles.entry(sizes).or_insert(0usize) += 1;
            if examples.len() < 6 {
                examples.push((name.to_string(), sections));
            }
        }
    }

    println!("{closed} of {total} mesh payloads walk exactly to the end as counted sections");
    println!("\n  the record-size sequences a closed walk used:");
    let mut ranked: Vec<(Vec<usize>, usize)> = cycles.into_iter().collect();
    ranked.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
    for (sizes, count) in ranked.iter().take(8) {
        println!("    {count:4}  {sizes:?}");
    }
    println!("\n  one example's sections (offset, count, record size):");
    if let Some((name, sections)) = examples.first() {
        println!("    {name}");
        for (at, count, size) in sections.iter().take(24) {
            println!("      {at:#06x}  count={count:5}  {size:3} bytes");
        }
    }
}
