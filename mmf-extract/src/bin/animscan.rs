//! Find the record grammar in the animation region.
//!
//! Scratch bin. The mesh decoder stops after the normals; what follows holds the
//! animation, and `docs/DMA3D.md` §3.3 records that its record layout is not
//! measured. Reading `createPropGuitar.mod`'s records by hand shows a `u32`
//! count followed by a run of `u16` values that are all vertex indices, so this
//! searches every payload for that shape and reports where it fits.
//!
//! The test is deliberately narrow: a `u32` at offset O that equals N, followed
//! by N `u16`s that are **all** less than the payload's own vertex count, and
//! where the array ends inside the payload. A `u32` that happens to sit inside
//! float data will not survive it, because its value will not match the length
//! of the index run that follows.
//!
//! Offsets come from `modfile::parse_mesh`, so the search is anchored to the
//! decoded mesh rather than re-derived.

use clap::Parser;
use mmf_extract::container::Container;
use mmf_extract::dma3d::modfile;

#[derive(Parser, Debug)]
#[command(
    name = "animscan",
    about = "Search each .mod payload's animation region for a counted u16 index array"
)]
struct Args {
    /// The Models container to read.
    #[arg(value_name = "CONTAINER")]
    path: std::path::PathBuf,

    /// Only payloads whose resource name contains this text.
    #[arg(value_name = "NAME", default_value = "")]
    name: String,

    /// Report every match in each payload, not just the first.
    #[arg(long)]
    all: bool,
}

/// Does a `u32 N` followed by `N` vertex indices fit at `at`?
///
/// Returns how many of the indices are distinct, so a run of one repeated value
/// can be told from a real list.
fn index_array_at(payload: &[u8], at: usize, vertex_count: usize) -> Option<usize> {
    if at + 4 > payload.len() {
        return None;
    }
    let n = u32::from_le_bytes([payload[at], payload[at + 1], payload[at + 2], payload[at + 3]])
        as usize;
    // A count of zero proves nothing, and a count larger than the payload can
    // hold is not a length.
    if n == 0 || n > payload.len() {
        return None;
    }
    let end = at + 4 + 2 * n;
    if end > payload.len() {
        return None;
    }
    let mut distinct = std::collections::HashSet::new();
    for k in 0..n {
        let value = u16::from_le_bytes([payload[at + 4 + 2 * k], payload[at + 5 + 2 * k]]);
        if usize::from(value) >= vertex_count {
            return None;
        }
        distinct.insert(value);
    }
    Some(distinct.len())
}

fn main() {
    let args = Args::parse();
    let data = std::fs::read(&args.path).expect("read container");
    let container = Container::parse(&args.path, data).expect("open Models");
    let n = container.resource_count as usize;

    let mut total = 0usize;
    let mut with_a_match = 0usize;
    let mut matches = 0usize;
    // Where the match sits relative to the mesh decoder's stopping point.
    let mut places: std::collections::BTreeMap<&'static str, usize> =
        std::collections::BTreeMap::new();
    let mut examples: Vec<(String, usize, usize, usize)> = Vec::new();

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
        // The animation region the mesh decoder left. It starts after the
        // `[0, 1, V]` header and the `V` float pairs that follow it, because
        // those are measured; a match inside the pair block is a `u32` that
        // happens to sit inside float data, not a record count.
        let start = mesh.tail_offset() + 12 + 8 * v;
        if start >= payload.len() {
            continue;
        }
        let mut found = 0usize;
        for at in start..payload.len().saturating_sub(6) {
            if let Some(distinct) = index_array_at(payload, at, v) {
                let count = u32::from_le_bytes([
                    payload[at],
                    payload[at + 1],
                    payload[at + 2],
                    payload[at + 3],
                ]) as usize;
                let kind = if at == start {
                    "at the tail offset itself"
                } else if at == start + 12 {
                    "just past the [0,1,V] header"
                } else if at == start + 12 + 8 * v {
                    "just past the V float pairs"
                } else if at < start + 1000 {
                    "near the start of the tail"
                } else {
                    "later in the payload"
                };
                *places.entry(kind).or_insert(0usize) += 1;
                if examples.len() < 12 {
                    examples.push((name.to_string(), at, count, distinct));
                }
                matches += 1;
                found += 1;
                if !args.all {
                    break;
                }
            }
        }
        if found > 0 {
            with_a_match += 1;
        }
    }

    println!(
        "{with_a_match} of {total} mesh payloads have a counted u16 vertex-index array \\
         in their animation region ({matches} matches in total)"
    );
    println!("\n  where in the animation region the array sits:");
    for (kind, count) in &places {
        println!("    {count:4}  {kind}");
    }
    println!("\n  the first few (name, offset, count, distinct values):");
    for (name, at, count, distinct) in &examples {
        println!("    {name:28} {at:#06x}  count={count:5}  distinct={distinct:5}");
    }
}
