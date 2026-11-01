//! Test the record grammar the loader reads, against every payload.
//!
//! Scratch bin. The `Models` loader at `0x4b2b60` in `DMA3D.exe` reads its
//! stream through the helpers `0x4e87c0` (u16) and `0x4e8810` (u32), and the
//! count it uses for **both** arrays comes from the same `u16` field. So the
//! grammar it expects is:
//!
//! ```text
//! at O        u16   must equal 2
//! at O+2      u16   N, the record count, reused by both arrays
//! at O+4      u16[N]
//! at O+4+2N   u32[N]
//! ```
//!
//! This searches every `.mod` payload for an offset where that grammar fits
//! exactly, and reports where it is and what `N` turns out to be. An offset
//! that recurs across payloads, at a fixed place in the decoded chain, is the
//! format; an offset that wanders is a coincidence.
//!
//! The mesh offsets come from `modfile::parse_mesh`, so the search is anchored
//! to the decoded mesh rather than re-derived.

use clap::Parser;
use mmf_extract::container::Container;
use mmf_extract::dma3d::modfile;

#[derive(Parser, Debug)]
#[command(
    name = "grammar",
    about = "Find where the loader's counted-record grammar fits each .mod payload"
)]
struct Args {
    /// The Models container to read.
    #[arg(value_name = "CONTAINER")]
    path: std::path::PathBuf,

    /// Only payloads whose resource name contains this text.
    #[arg(value_name = "NAME", default_value = "")]
    name: String,

    /// Report every match, not just one per payload.
    #[arg(long)]
    all: bool,
}

/// Does the grammar fit at `at`? Returns the count `N` when it does.
fn fits(payload: &[u8], at: usize) -> Option<usize> {
    if at + 4 > payload.len() {
        return None;
    }
    let version = u16::from_le_bytes([payload[at], payload[at + 1]]);
    if version != 2 {
        return None;
    }
    let n = u16::from_le_bytes([payload[at + 2], payload[at + 3]]) as usize;
    // A count of zero is a section with nothing in it, so it proves nothing.
    if n == 0 {
        return None;
    }
    let u16_end = at + 4 + 2 * n;
    let u32_end = u16_end + 4 * n;
    if u32_end > payload.len() {
        return None;
    }
    Some(n)
}

/// Where in the decoded chain an offset sits.
///
/// The chain is `u32 C` at `mesh_at`, `u16[C]`, `u32 V`, `vertex[V]`,
/// `u32 V`, `normal[V]`, then the tail. An offset that always lands on the same
/// one of those is a section of the format.
fn place(at: usize, mesh: &modfile::ModelMesh) -> &'static str {
    let mesh_at = mesh.index_count_offset;
    let c = mesh.indices.len();
    let v = mesh.vertex_count();
    let after_indices = mesh_at + 4 + 2 * c;
    let after_vertices = after_indices + 4 + 16 * v;
    let after_normals = mesh.normal_array_offset() + 4 + 16 * v;
    if at == mesh_at {
        "the u32 C itself"
    } else if at == mesh_at + 4 {
        "the start of the index list"
    } else if at == after_indices {
        "the u32 V, just past the index list"
    } else if at == after_indices + 4 {
        "the vertex array"
    } else if at == after_vertices {
        "the second u32 V, just past the vertices"
    } else if at == mesh.normal_array_offset() {
        "the normal array"
    } else if at == after_normals {
        "the tail, just past the normal array"
    } else if at == mesh.tail_offset() {
        "the tail"
    } else {
        "elsewhere in the payload"
    }
}

fn main() {
    let args = Args::parse();
    let data = std::fs::read(&args.path).expect("read container");
    let container = Container::parse(&args.path, data).expect("open Models");
    let n = container.resource_count as usize;

    let mut payloads_total = 0usize;
    let mut payloads_with_a_match = 0usize;
    let mut matches = 0usize;
    // Where in the decoded chain each fit landed.
    let mut places: std::collections::BTreeMap<&'static str, usize> =
        std::collections::BTreeMap::new();
    // Whether the count matches the index count, the vertex count, or neither.
    let mut counts: std::collections::BTreeMap<&'static str, usize> =
        std::collections::BTreeMap::new();
    let mut examples: Vec<(String, usize, usize)> = Vec::new();

    for index in 0..n {
        let name = container.name(index);
        if !args.name.is_empty() && !name.contains(&args.name) {
            continue;
        }
        let payload = container.payload(index);
        let Ok(mesh) = modfile::parse_mesh(payload) else {
            continue;
        };
        payloads_total += 1;
        let mut found = 0usize;
        for at in 0..payload.len().saturating_sub(4) {
            if let Some(count) = fits(payload, at) {
                *places.entry(place(at, &mesh)).or_insert(0usize) += 1;
                let kind = if count == mesh.indices.len() {
                    "N == the index count C"
                } else if count == mesh.vertex_count() {
                    "N == the vertex count V"
                } else if count == mesh.indices.len() / 3 {
                    "N == C/3"
                } else {
                    "N == neither"
                };
                *counts.entry(kind).or_insert(0usize) += 1;
                if examples.len() < 10 {
                    examples.push((name.to_string(), at, count));
                }
                matches += 1;
                found += 1;
                if !args.all {
                    break;
                }
            }
        }
        if found > 0 {
            payloads_with_a_match += 1;
        }
    }

    println!(
        "{payloads_with_a_match} of {payloads_total} mesh payloads fit the grammar \
         somewhere ({matches} matches in total)"
    );
    println!("\n  where in the decoded chain the fit landed:");
    for (where_, count) in &places {
        println!("    {count:4}  {where_}");
    }
    println!("\n  what the count turned out to be:");
    for (kind, count) in &counts {
        println!("    {count:4}  {kind}");
    }
    println!("\n  the first few (name, offset, N):");
    for (name, at, count) in &examples {
        println!("    {name:30} {at:#06x}  N={count}");
    }
}
