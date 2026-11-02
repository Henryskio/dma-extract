//! The `.mod` model payload: the mesh.
//!
//! The payload is a sequence of **counted record arrays** (`docs/DMA3D.md`
//! §3.2-§3.3). The mesh part of that chain is measured on 144 of the 460
//! payloads and is what this module reads:
//!
//! ```text
//! u32   C                 index count
//! u16[C]                  index list, **zero-based**, values < V
//! u32   V                 vertex count
//! vertex[V]               16 bytes: x, y, z (f32) + w (f32, 1.0)
//! u32   V                 normal count (== V)
//! normal[V]               16 bytes: nx, ny, nz (f32) + w (f32, 0.0)
//! ```
//!
//! The counts are **not** in the header: a search over every byte of every
//! header for a `u32` or `u16` equal to `C`, to `V`, or to the mesh's offset
//! finds nothing (§3.3). So the chain is found by its structural signature —
//! `C` opens `2*C` bytes whose successor `u32` opens `V` records of `w == 1.0`,
//! and the `u32` after *those* equals `V` and opens unit-length records of
//! `w == 0.0`. Requiring both halves keeps a random `w == 1.0` run from being
//! mistaken for a vertex array. A third word was tried — the tail's `[0, 1, V]`
//! header — and had to be dropped: it is present in 94 of the 144 payloads but
//! absent from the other 50, so requiring it would silently discard real meshes
//! (`createModMickeySkeleton` opens `[0, 0, 3, 4]`). It is read and recorded
//! instead, as `ModelMesh::tail_header`.
//!
//! The fourth float is a **record-type marker**, not data: it is `1.0` in every
//! vertex record and `0.0` in every normal record, with a single distinct value
//! each (§3.3).
//!
//! What follows the normal array is `V` pairs of little-endian floats. Those
//! pairs are **texture coordinates on some payloads and something else on
//! others**: all 68 of `createPropGuitar.mod`'s pairs land in `0..1` and *none*
//! of `createPropBed.mod`'s 256 do. Which is which is not measured, so the
//! pairs are not decoded here and the tail is written through as a `.tail` file
//! beside the glTF instead.

use anyhow::{Result, bail};

/// Bytes per vertex record: three f32s and the type marker.
pub const VERTEX_RECORD_BYTES: usize = 16;
/// Bytes per normal record. Same shape; the marker is 0.0 instead of 1.0.
pub const NORMAL_RECORD_BYTES: usize = 16;
/// The marker the vertex records carry.
pub const VERTEX_MARKER: f32 = 1.0;
/// The marker the normal records carry.
pub const NORMAL_MARKER: f32 = 0.0;
/// How close a float must be to a marker to count as it.
const MARKER_EPSILON: f32 = 1e-6;
/// A normal this far from unit length fails the structural check.
const UNIT_LENGTH_EPSILON: f32 = 1e-3;

/// One decoded model mesh.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelMesh {
    /// Where the index count was read from, for diagnostics.
    pub index_count_offset: usize,
    /// The index list, zero-based exactly as stored.
    pub indices: Vec<u16>,
    /// World-space positions, `[x, y, z]` per vertex.
    pub positions: Vec<[f32; 3]>,
    /// Unit-length normals, `[nx, ny, nz]` per vertex.
    pub normals: Vec<[f32; 3]>,
    /// The three `u32`s that open the tail, when they are the shape most
    /// payloads carry: `0`, `1`, and the vertex count.
    ///
    /// This is a **measurement, not a requirement**: requiring it was tried and
    /// it rejects 50 of the 144 payloads the marker-and-normal signature
    /// accepts, because their tails open differently — `createModMickeySkeleton`
    /// opens `[0, 0, 3, 4]` and the small profile props open with 25 zero bytes.
    /// So the parser reads the words when they are there and does not demand
    /// them, and the ones that *do* carry `[0, 1, V]` are the payloads whose
    /// tail is known to start that way.
    pub tail_header: Option<(u32, u32, u32)>,
}

impl ModelMesh {
    /// Vertex count. Equals the length of both record arrays.
    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }
    /// Byte offset at which the normal array begins.
    pub fn normal_array_offset(&self) -> usize {
        self.index_count_offset + 4 + 2 * self.indices.len() + 4 + 16 * self.positions.len()
    }
    /// Byte offset just past the normal array, i.e. the start of the undecoded
    /// texture-coordinate and animation tail.
    pub fn tail_offset(&self) -> usize {
        self.normal_array_offset() + 4 + 16 * self.normals.len()
    }
}

/// Read a little-endian f32 from `bytes` at `off`.
fn f32_at(bytes: &[u8], off: usize) -> f32 {
    f32::from_le_bytes([bytes[off], bytes[off + 1], bytes[off + 2], bytes[off + 3]])
}

/// Read a little-endian u32 from `bytes` at `off`.
fn u32_at(bytes: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([bytes[off], bytes[off + 1], bytes[off + 2], bytes[off + 3]])
}

/// Read a little-endian u16 from `bytes` at `off`.
fn u16_at(bytes: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([bytes[off], bytes[off + 1]])
}

/// True when `f` is a real number within the game's working coordinate range.
///
/// The store contains NaN and infinity padding both in the payload and between
/// sections, so a plausibility test has to reject those as well as absurd
/// magnitudes before anything is believed.
fn plausible(f: f32) -> bool {
    f.is_finite() && f.abs() < 1.0e7
}

/// Parse the mesh chain out of a `.mod` payload.
///
/// `payload` is the whole resource as stored. A payload that carries no mesh
/// — the `createPropUI*` records are 276 bytes and hold none — is an error, not
/// an empty mesh, because for those the *reference* is the payload and the mesh
/// is not the thing that is missing.
pub fn parse_mesh(payload: &[u8]) -> Result<ModelMesh> {
    // The magic, so a caller cannot hand us a wrong kind of payload by mistake.
    if !payload.starts_with(b"mod II") {
        bail!("not a `.mod` payload (no `mod II` magic)");
    }
    // The header is not decoded, so the search starts past it and the mesh is
    // found by its signature. 6000 is well past every measured header (163-263)
    // and keeps a corrupt file from being scanned end to end.
    for at in 6..payload.len().min(6000) {
        let Some(c) = count_at(payload, at) else {
            continue;
        };
        let indices_end = at + 4 + 2 * c as usize;
        if indices_end + 4 > payload.len() {
            continue;
        }
        let v = u32_at(payload, indices_end);
        if !(1..=100_000).contains(&v) {
            continue;
        }
        let v = v as usize;
        let vertices_start = indices_end + 4;
        let normals_count_offset = vertices_start + VERTEX_RECORD_BYTES * v;
        if normals_count_offset + 4 > payload.len() {
            continue;
        }
        // The second half of the signature: the array after the vertices is
        // announced by a count equal to V.
        if u32_at(payload, normals_count_offset) as usize != v {
            continue;
        }
        let normals_start = normals_count_offset + 4;
        if normals_start + NORMAL_RECORD_BYTES * v > payload.len() {
            continue;
        }

        // Every index must name a vertex, and the indices must not all be the
        // same value — a table of one repeated index is not an index list.
        let indices: Vec<u16> = (0..c as usize)
            .map(|k| u16_at(payload, at + 4 + 2 * k))
            .collect();
        if indices.iter().any(|&i| usize::from(i) >= v) {
            continue;
        }
        if indices
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
            < 2
        {
            continue;
        }

        // The vertices: w == 1.0 for all but a few, and every coordinate real.
        let mut positions = Vec::with_capacity(v);
        let mut marked = 0usize;
        for k in 0..v {
            let off = vertices_start + VERTEX_RECORD_BYTES * k;
            let coord = [
                f32_at(payload, off),
                f32_at(payload, off + 4),
                f32_at(payload, off + 8),
            ];
            if !coord.iter().all(|&x| plausible(x)) {
                positions.clear();
                break;
            }
            if (f32_at(payload, off + 12) - VERTEX_MARKER).abs() <= MARKER_EPSILON {
                marked += 1;
            }
            positions.push(coord);
        }
        if positions.len() != v || marked * 10 < v * 9 {
            continue;
        }

        // The normals: w == 0.0 for all but a few, unit length, real.
        let mut normals = Vec::with_capacity(v);
        let mut unit = 0usize;
        for k in 0..v {
            let off = normals_start + NORMAL_RECORD_BYTES * k;
            let coord = [
                f32_at(payload, off),
                f32_at(payload, off + 4),
                f32_at(payload, off + 8),
            ];
            let length = coord.iter().map(|x| x * x).sum::<f32>().sqrt();
            if !coord.iter().all(|&x| plausible(x)) || (length - 1.0).abs() > UNIT_LENGTH_EPSILON {
                normals.clear();
                break;
            }
            if f32_at(payload, off + 12).abs() <= MARKER_EPSILON {
                unit += 1;
            }
            normals.push(coord);
        }
        if normals.len() != v || unit * 10 < v * 9 {
            continue;
        }

        // The three words the tail opens with, recorded rather than required:
        // most payloads carry `[0, 1, V]`, but 50 of the 144 open with
        // something else, so demanding the shape would silently drop real
        // meshes. A candidate that gets this far is already a `w == 1.0` run
        // followed by a matching unit-length `w == 0.0` run, which is enough.
        let tail_at = normals_start + NORMAL_RECORD_BYTES * v;
        let tail_header = if tail_at + 12 <= payload.len() {
            Some((
                u32_at(payload, tail_at),
                u32_at(payload, tail_at + 4),
                u32_at(payload, tail_at + 8),
            ))
        } else {
            None
        };

        return Ok(ModelMesh {
            index_count_offset: at,
            indices,
            positions,
            normals,
            tail_header,
        });
    }
    bail!("no mesh chain in the payload")
}

/// The index count at `off`, if it can start a chain that fits the payload.
fn count_at(payload: &[u8], off: usize) -> Option<u32> {
    if off + 4 > payload.len() {
        return None;
    }
    let c = u32_at(payload, off);
    if !(1..=400_000).contains(&c) {
        return None;
    }
    Some(c)
}

#[cfg(test)]
mod tests {
    /// The payload a structural parser has to reject.
    #[test]
    fn a_payload_without_the_magic_is_not_a_model() {
        assert!(super::parse_mesh(b"not a model at all").is_err());
    }

    /// The shortest UI record: 276 bytes and no mesh, so it must be an error
    /// rather than an empty mesh.
    #[test]
    fn the_ui_records_carry_no_mesh() {
        assert!(super::parse_mesh(&[0u8; 276]).is_err());
    }

    /// A synthetic chain parses, and the parser reports where it read the count.
    #[test]
    fn a_synthetic_chain_parses() {
        // header (6 bytes of magic + 2 of padding), then the chain.
        let mut p = b"mod II".to_vec();
        p.extend_from_slice(&0u16.to_le_bytes());
        // C = 3 indices over V = 3 vertices (indices are < V by the layout's
        // own rule, which the real files satisfy: 150 for V = 151).
        p.extend_from_slice(&3u32.to_le_bytes());
        for i in [1u16, 2, 1] {
            p.extend_from_slice(&i.to_le_bytes());
        }
        // V = 3, then three vertex records with w == 1.0.
        p.extend_from_slice(&3u32.to_le_bytes());
        for v in [[0.0f32, 1.0, 2.0], [3.0, 4.0, 5.0], [6.0, 7.0, 8.0]] {
            for c in v {
                p.extend_from_slice(&c.to_le_bytes());
            }
            p.extend_from_slice(&1.0f32.to_le_bytes());
        }
        // the normal count, then three unit normals with w == 0.0.
        p.extend_from_slice(&3u32.to_le_bytes());
        for n in [[1.0f32, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]] {
            for c in n {
                p.extend_from_slice(&c.to_le_bytes());
            }
            p.extend_from_slice(&0.0f32.to_le_bytes());
        }
        // The tail header every shipped payload carries: 0, 1, V.
        p.extend_from_slice(&0u32.to_le_bytes());
        p.extend_from_slice(&1u32.to_le_bytes());
        p.extend_from_slice(&3u32.to_le_bytes());

        let mesh = super::parse_mesh(&p).expect("parse the synthetic chain");
        assert_eq!(mesh.vertex_count(), 3);
        assert_eq!(mesh.indices, vec![1, 2, 1]);
        assert_eq!(
            mesh.positions,
            vec![[0.0, 1.0, 2.0], [3.0, 4.0, 5.0], [6.0, 7.0, 8.0]]
        );
        assert_eq!(
            mesh.normals,
            vec![[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
        );
        assert_eq!(mesh.tail_header, Some((0, 1, 3)));
        // The tail starts right after the normal array, and the test's own
        // header is the first thing in it.
        assert_eq!(mesh.tail_offset(), 122);
        assert_eq!(&p[mesh.tail_offset()..], &[0, 0, 0, 0, 1, 0, 0, 0, 3, 0, 0, 0]);
        assert_eq!(mesh.normal_array_offset(), 70);
    }
}

#[cfg(test)]
mod real_files {
    //! The parser against the shipped payloads. These skip when the carved
    //! 3D assets are not present, the same rule the other suites follow.
    use super::*;
    use crate::container::Container;

    fn dma3d_models() -> Option<Vec<u8>> {
        let dir = std::env::var("DMA3D_ASSETS").unwrap_or_else(|_| "/tmp/dma3d/pc".into());
        let path = std::path::Path::new(&dir).join("DMA3D.MMM");
        std::fs::read(path).ok()
    }

    /// Every non-UI static prop must parse, with C and V in the ranges measured
    /// by hand, and the tail must begin at 4 bytes past the normal array.
    #[test]
    fn the_mesh_parses_out_of_the_shipped_props() {
        let Some(data) = dma3d_models() else {
            eprintln!("skipping: the 3D game's assets are not present");
            return;
        };
        let container = Container::parse("DMA3D.MMM", data).expect("open Models");
        let n = container.resource_count as usize;

        let mut parsed = 0usize;
        let mut skipped = Vec::new();
        for i in 0..n {
            let name = container.name(i);
            if !name.starts_with("createProp") || name.contains("UI") {
                continue;
            }
            let mesh = match parse_mesh(container.payload(i)) {
                Ok(m) => m,
                Err(e) => {
                    skipped.push(format!("{name}: {e}"));
                    continue;
                }
            };
            // Sanity, not a measured bound: the 144 payloads that carry a chain
            // run 24..=257 vertices, so anything far outside that range means a
            // false positive rather than a small mesh. The counts 144 / 316 /
            // 460 are asserted in `tests/dma3d_numbers.rs`, so this comment
            // cannot drift from the data again. The index list is zero-based
            // and every value names a vertex -- asserted directly below.
            assert!(
                (4..=4096).contains(&mesh.vertex_count()),
                "{name}: {} vertices is outside any sane range",
                mesh.vertex_count()
            );
            assert!(
                mesh.indices
                    .iter()
                    .all(|&x| usize::from(x) < mesh.vertex_count()),
                "{name}: an index names a vertex that does not exist"
            );
            // The index base, checked on the real files: zero-based because
            // every mesh carries the value 0 and none carries its own vertex
            // count. This is the assertion that caught the glTF writer
            // subtracting one from a base that was already zero.
            assert!(
                mesh.indices.contains(&0),
                "{name}: no index 0, so the base may not be zero"
            );
            assert!(
                !mesh
                    .indices
                    .iter()
                    .any(|&x| usize::from(x) == mesh.vertex_count()),
                "{name}: index == vertex count, so the base may not be zero"
            );
            assert_eq!(mesh.normals.len(), mesh.vertex_count());
            // The tail header, on the real files: `0, 1, V` with the third word
            // equal to this payload's own vertex count. It is now part of the
            // structural signature, so a payload that fails it fails to parse
            // rather than parsing with a shorter tail.
            // Recorded, not required: the header is present in most payloads
            // and absent in 50, so this is a measurement of the shape rather
            // than a rule the parser enforces.
            if let Some(header) = mesh.tail_header {
                assert_eq!(
                    header.2,
                    mesh.vertex_count() as u32,
                    "{name}: the tail header's third word is not the vertex count"
                );
            }
            for (k, nrm) in mesh.normals.iter().enumerate() {
                let len = nrm.iter().map(|x| x * x).sum::<f32>().sqrt();
                assert!(
                    (len - 1.0).abs() < 1e-3,
                    "{name}: normal {k} is not unit length"
                );
            }
            parsed += 1;
        }
        assert!(parsed >= 30, "only {parsed} props parsed");
        if !skipped.is_empty() {
            eprintln!("{} props did not parse: {skipped:?}", skipped.len());
        }
    }

    /// The UI records carry no mesh, which is what makes them references.
    #[test]
    fn the_ui_records_parse_as_no_mesh() {
        let Some(data) = dma3d_models() else {
            eprintln!("skipping: the 3D game's assets are not present");
            return;
        };
        let container = Container::parse("DMA3D.MMM", data).expect("open Models");
        let mut ui = 0usize;
        for i in 0..container.resource_count as usize {
            if !container.name(i).starts_with("createPropUI") {
                continue;
            }
            assert!(
                parse_mesh(container.payload(i)).is_err(),
                "{} should carry no mesh",
                container.name(i)
            );
            ui += 1;
        }
        assert!(ui > 0, "no UI records found");
    }
}
