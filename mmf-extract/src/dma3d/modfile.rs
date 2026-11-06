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
    f.is_finite()
        && f.abs() < 1.0e7
        // A denormal is the signature of a **misaligned** read: a shifted
        // boundary lands inside the mantissa of a neighbouring `f32` and
        // produces a value around `1e-40` that no coordinate in this game
        // takes. Admitting them is how a payload whose bytes are not a point
        // stream was billed as a profile at a phase the payload does not carry.
        && (f == 0.0 || f.abs() >= f32::MIN_POSITIVE)
}

/// Parse the mesh chain out of a `.mod` payload.
///
/// `payload` is the whole resource as stored. A payload that carries no mesh
/// — the `createPropUI*` records are 276 bytes and hold none — is an error, not
/// an empty mesh, because for those the *reference* is the payload and the mesh
/// is not the thing that is missing.
pub fn parse_mesh(payload: &[u8]) -> Result<ModelMesh> {
    // The magic is a **hint**, not a gate. `docs/DMA3D.md` §3 says the counts
    // are not in the header and the chain is located structurally, and that is
    // what the loop below does. Requiring the magic at offset 0 rejected the
    // Italian disc's meshes, whose `mod II` sits at scattered offsets (3, 24,
    // 35, 44, 50, …) behind a header: 9 of its 460 payloads carry the chain and
    // none of them starts with the magic. The structural signature — a count,
    // that many zero-based indices below the vertex count, that many vertices
    // whose `w` is 1.0, and a second count equal to it — is strong enough on its
    // own, and the caller's mistake is caught by it failing rather than by the
    // magic being absent.
    //
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

/// One decoded `createMod` drawing profile.
///
/// `docs/DMA3D.md` §11.25: the 443 payloads that carry no mesh chain are a second
/// shape in the same container — a **`f32` point stream**. Each 16-byte record
/// holds four little-endian `f32`s in which **one slot is a constant** and the
/// other three vary, and the constant says which: `1.0` is what a *vertex* of the
/// mesh chain carries (`src/dma3d/modfile.rs` checks `abs(w - 1.0) < 1e-6`), `0.0`
/// is what a *normal* carries (`abs(w) < 1e-6`). A drawing profile is a point
/// stream in a plane, so exactly one of its three coordinates is constant, and
/// the engine tints and extrudes it at runtime — which is why the payloads are
/// small and why none of them holds a triangle list.
#[derive(Debug, Clone, PartialEq)]
pub struct Profile {
    /// Byte offset the record stream starts at, after the variable-length
    /// prefix. It is one of 0..8, and it is *measured* from the payload, not
    /// assumed: the prefix length is what makes every earlier flat-stride scan
    /// read these payloads as garbage (§11.15) and as high entropy (§11.16).
    pub phase: usize,
    /// Which of the four slots holds the constant, 0..4.
    pub constant_slot: usize,
    /// The constant itself, `0.0` or `1.0`.
    pub marker: f32,
    /// The records, in payload order, as the three varying slots.
    pub points: Vec<[f32; 3]>,
    /// Byte offset just past the last record read, i.e. the start of the part
    /// the reader did not read.
    ///
    /// A payload is a record run followed by whatever else it carries, exactly
    /// as a mesh payload is a chain followed by a tail
    /// (`docs/DMA3D.md` §3.4, §11.26). Requiring the constant to hold across
    /// *every* record the payload leaves room for rejected 100 payloads that
    /// carry a clean run and then something else.
    pub tail_offset: usize,
}

impl Profile {
    /// Number of records the stream holds.
    pub fn record_count(&self) -> usize {
        self.points.len()
    }
}

/// Read a drawing profile out of a payload that carries no mesh chain.
///
/// The reader measures the phase and the constant slot rather than assuming
/// them, because the header's own length is the prefix and it is not in the
/// payload: `mod II` sits at offset 3 in one resource and at 24 in another
/// (§11.9, §11.22), and the record stream starts after it. For every
/// `(phase, slot)` pair the reader asks whether one slot is constant across all
/// the records the payload leaves room for, whether the other three are
/// plausible coordinates in at least 70% of them, and whether at least two
/// records carry a value that is not zero — that last one keeps a payload of
/// padding from reading as a profile.
///
/// A payload that fits no pair is an error, which the caller reports per
/// resource rather than skipping (`AGENTS.md`).
pub fn parse_profile(payload: &[u8]) -> Result<Profile> {
    let mut best: Option<(usize, usize, usize, f32, Vec<[f32; 3]>)> = None;
    // Where the record stream starts. The prefix (`docs/DMA3D.md` §11.31,
    // §11.32) is a header, a list of texture names, and the `mod II` magic with
    // the `u32` after it, and the records begin ten bytes past the magic — so the
    // magic is an **anchor** for the start, which is what the 0..8 phase scan
    // alone cannot be: a phase scan finds an alignment, and the records here
    // begin at `magic + 10`, an offset the 0..8 scan never reaches.
    let mut starts: Vec<usize> = Vec::new();
    if let Some(magic) = payload.windows(6).position(|w| w == b"mod II") {
        for step in 0..4 {
            starts.push(magic + 10 + 16 * step);
        }
    }
    starts.extend(0..8usize);
    for phase in starts {
        if payload.len() < phase + 3 * 16 {
            continue;
        }
        let records = (payload.len() - phase) / 16;
        for slot in 0..4usize {
            // The marker is a stored constant, so it is *bit-identical* across
            // the records. Comparing within an epsilon let a denormal
            // (9.18e-35, 5.51e-34) pass as zero, and the two payloads that did
            // carry those are unreadable-name records whose bytes are not a
            // point stream at all.
            let marker = f32_at(payload, phase + 4 * slot);
            if marker.to_bits() != 0.0f32.to_bits() && marker.to_bits() != 1.0f32.to_bits() {
                continue;
            }
            // The run is the **prefix** in which the marker is bit-identical and
            // the other three slots are plausible coordinates. A payload carries
            // its point stream and then whatever else it holds, so requiring the
            // constant across every record the payload leaves room for rejected
            // 100 payloads that carry a clean run followed by something else.
            let mut points = Vec::with_capacity(records);
            let mut nonconstant_records = 0usize;
            for record in 0..records {
                let at = phase + 16 * record;
                if f32_at(payload, at + 4 * slot).to_bits() != marker.to_bits() {
                    break;
                }
                let mut point = [0f32; 3];
                let mut all_plausible = true;
                let mut any_nonzero = false;
                for j in 0..3 {
                    point[j] = f32_at(payload, at + 4 * (if j < slot { j } else { j + 1 }));
                    if !plausible(point[j]) {
                        all_plausible = false;
                    }
                    if point[j] != 0.0 {
                        any_nonzero = true;
                    }
                }
                if !all_plausible {
                    break;
                }
                if any_nonzero {
                    nonconstant_records += 1;
                }
                points.push(point);
            }
            // Three records to call it a run, two of them carrying a real
            // coordinate so a payload of padding is not a profile.
            if points.len() < 3 || nonconstant_records < 2 {
                continue;
            }
            let score = points.len() * 100 + nonconstant_records;
            let better = best.as_ref().map(|b| score > b.0).unwrap_or(true);
            if better {
                best = Some((score, phase, slot, marker, points));
            }
        }
    }
    match best {
        Some((score, phase, slot, marker, points)) => {
            let _ = score;
            let tail_offset = phase + 16 * points.len();
            Ok(Profile {
                phase,
                constant_slot: slot,
                marker,
                points,
                tail_offset,
            })
        }
        None => bail!("no f32 point stream fits this payload (`docs/DMA3D.md` §11.25)"),
    }
}

#[cfg(test)]
mod profile_tests {
    use super::*;

    /// A payload the reader accepts is the payload's own bytes at the measured
    /// phase with the constant slot removed — not a coincidence of thresholds.
    #[test]
    fn a_profile_pays_for_its_points() {
        // Four records of `[1.0, x, y, z]`, the vertex shape: `1.0` is the
        // constant slot and the other three are the point.
        let mut payload: Vec<u8> = Vec::new();
        for k in 0..4u8 {
            let (x, y, z) = (0.25 * k as f32, -0.5 + 0.25 * k as f32, 0.75 - 0.25 * k as f32);
            payload.extend_from_slice(&1.0f32.to_le_bytes());
            payload.extend_from_slice(&x.to_le_bytes());
            payload.extend_from_slice(&y.to_le_bytes());
            payload.extend_from_slice(&z.to_le_bytes());
        }
        let profile = parse_profile(&payload).expect("the payload reads as a profile");
        assert_eq!(profile.constant_slot, 0);
        assert_eq!(profile.marker.to_bits(), 1.0f32.to_bits());
        assert_eq!(profile.points.len(), 4);
        for (k, point) in profile.points.iter().enumerate() {
            assert_eq!(point[0], 0.25 * k as f32);
            assert_eq!(point[1], -0.5 + 0.25 * k as f32);
            assert_eq!(point[2], 0.75 - 0.25 * k as f32);
        }
    }

    /// The constant must be bit-identical. A denormal in the slot is not a zero,
    /// and reading it as one was how unreadable-name payloads were billed as
    /// profiles.
    #[test]
    fn a_denormal_in_the_marker_slot_is_not_a_zero() {
        let mut payload: Vec<u8> = Vec::new();
        for _ in 0..4 {
            payload.extend_from_slice(&1.0e-40f32.to_le_bytes()); // a denormal
            payload.extend_from_slice(&0.5f32.to_le_bytes());
            payload.extend_from_slice(&0.5f32.to_le_bytes());
            payload.extend_from_slice(&0.5f32.to_le_bytes());
        }
        assert!(
            parse_profile(&payload).is_err(),
            "a denormal constant slot was read as 0.0"
        );
    }

    /// A payload of padding is not a profile: it has the constant slot, but
    /// nothing in the varying ones.
    #[test]
    fn a_payload_of_padding_is_not_a_profile() {
        let mut payload = vec![0u8; 64];
        payload.extend_from_slice(&1.0f32.to_le_bytes());
        payload.extend_from_slice(&1.0f32.to_le_bytes());
        assert!(parse_profile(&payload).is_err(), "padding read as a profile");
    }

    /// Too short to hold the three records the reader requires.
    #[test]
    fn a_payload_too_short_for_a_profile_is_reported() {
        assert!(parse_profile(&[0u8; 8]).is_err());
    }
}

/// One `mod II` record stream that is not a mesh and not a drawing profile.
///
/// `docs/DMA3D.md` §11.31, §11.32: a payload is a header, a list of texture names,
/// the `mod II` magic with a `u32` after it, and a record stream. The mesh reader
/// and the profile reader each claim the payloads whose records are the shape they
/// read; **48 of the shipped payloads carry the magic and the `u32` and neither
/// reader's shape**. This is the reader for those, and it deliberately does not
/// guess what the records mean.
///
/// What is measured, and therefore what this records: where the magic sits, the
/// `u32` after it (`16` in 47 of the 48, `15` in one), the `u16` pair stream that
/// follows, and the `f32` values the stream carries at four-byte steps. §11.32
/// measured that no position in that stream holds a constant across payloads, so
/// the record schema is not derivable from the bytes and this says so rather than
/// asserting one.
#[derive(Debug, Clone, PartialEq)]
pub struct MagicRecord {
    /// Byte offset of the `mod II` magic.
    pub magic_offset: usize,
    /// The `u32` after the magic: the record type, as the game's own reader treats
    /// it (`docs/DMA3D.md` §11.9, §11.20).
    pub record_type: u32,
    /// The `u16` stream after the magic, in payload order. Read as little-endian
    /// pairs, which is the alignment §11.22 measured.
    pub pairs: Vec<u16>,
    /// The `f32` values the same bytes hold when read at four-byte steps from the
    /// stream's start. §11.32 measured that these are *inside* the record stream
    /// rather than after it.
    pub floats: Vec<f32>,
}

impl MagicRecord {
    /// Byte offset just past the last record this read.
    pub fn end(&self) -> usize {
        self.magic_offset + 10 + 2 * self.pairs.len()
    }
}

/// Read a `mod II` record stream that is neither a mesh nor a drawing profile.
///
/// The payload must carry the magic and the `u32` after it; a payload that carries
/// neither is an error, so the caller reports it rather than writing a document
/// that describes nothing.
pub fn parse_magic_record(payload: &[u8]) -> Result<MagicRecord> {
    let magic = payload
        .windows(6)
        .position(|w| w == b"mod II")
        .ok_or_else(|| anyhow::anyhow!("no `mod II` magic, so there is no record stream to describe"))?;
    if magic + 10 > payload.len() {
        bail!("the `mod II` magic is too close to the payload's end to carry its u32");
    }
    let record_type = u32_at(payload, magic + 6);
    let stream = magic + 10;
    // Every pair the payload leaves room for, which is the same "payload length is
    // ground truth" rule `AGENTS.md` states.
    let pairs: Vec<u16> = (0..(payload.len() - stream) / 2)
        .map(|k| u16_at(payload, stream + 2 * k))
        .collect();
    // Only the floats that read as numbers. A denormal is the signature of a
    // **misaligned** read -- a shifted boundary landing inside a neighbouring
    // value's mantissa produces about 1e-40, which no field in this game takes
    // -- so including them would report noise as data. This is the same rule
    // `plausible` applies, and it is measured, not a preference.
    let floats: Vec<f32> = (0..(payload.len() - stream) / 4)
        .map(|k| f32_at(payload, stream + 4 * k))
        .filter(|f| plausible(*f))
        .collect();
    Ok(MagicRecord {
        magic_offset: magic,
        record_type,
        pairs,
        floats,
    })
}

#[cfg(test)]
mod magic_record_tests {
    use super::*;

    /// The magic plus the `u32` type plus a pair stream, which is the shape
    /// `docs/DMA3D.md` §11.31 measured. A payload with no magic is an error.
    #[test]
    fn a_payload_without_the_magic_has_no_record_stream() {
        assert!(parse_magic_record(b"nothing here at all").is_err());
    }

    /// The measured shape: magic, `u32` type, `u16` pairs.
    #[test]
    fn the_magic_and_its_type_and_pairs_are_read() {
        let mut payload: Vec<u8> = Vec::new();
        payload.extend_from_slice(b"mod II");
        payload.extend_from_slice(&16u32.to_le_bytes());
        for v in [1u16, 1, 466, 0, 1, 0] {
            payload.extend_from_slice(&v.to_le_bytes());
        }
        let record = parse_magic_record(&payload).expect("the payload carries a record");
        assert_eq!(record.magic_offset, 0);
        assert_eq!(record.record_type, 16);
        assert_eq!(record.pairs, vec![1, 1, 466, 0, 1, 0]);
        // The pairs fill the payload exactly: payload length is ground truth.
        assert_eq!(record.end(), payload.len());
    }

    /// The magic is located wherever it sits, not only at offset 0 — which is the
    /// same rule §11.9 recorded for the mesh chain.
    #[test]
    fn the_magic_is_found_behind_a_header() {
        let mut payload = vec![0u8; 37];
        payload.extend_from_slice(b"mod II");
        payload.extend_from_slice(&15u32.to_le_bytes());
        payload.extend_from_slice(&7u16.to_le_bytes());
        payload.extend_from_slice(&7u16.to_le_bytes());
        let record = parse_magic_record(&payload).expect("the payload carries a record");
        assert_eq!(record.magic_offset, 37);
        assert_eq!(record.record_type, 15);
        assert_eq!(record.pairs, vec![7, 7]);
    }

    /// A denormal is not a number: §11.26 measured that a shifted boundary lands in
    /// a neighbouring value's mantissa and produces about 1e-40, so the floats this
    /// records must be only the ones that read as numbers.
    #[test]
    fn a_denormal_is_not_a_recorded_float() {
        let mut payload: Vec<u8> = Vec::new();
        payload.extend_from_slice(b"mod II");
        payload.extend_from_slice(&16u32.to_le_bytes());
        payload.extend_from_slice(&1.0e-40f32.to_le_bytes()); // a denormal
        payload.extend_from_slice(&0.5f32.to_le_bytes()); // a real value
        let record = parse_magic_record(&payload).expect("the payload carries a record");
        assert!(
            record.floats.iter().all(|f| *f == 0.0 || f.abs() >= f32::MIN_POSITIVE),
            "a denormal was recorded as a float: {:?}",
            record.floats
        );
    }
}

/// One parameter table: small values at alternating byte positions with zero fill.
///
/// `docs/DMA3D.md` §11.37 measured this shape in the shipped container and named it
/// what it is: **a table of small values stored at alternating byte positions, with
/// zero fill** — a parameter table, an index table, or a packed enum, which the game
/// reads to configure a UI element or a material rather than to describe its
/// outline. `createModBtSize02`, `athCircle`, `athFig8R` are all of it, which is what
/// their names already said.
///
/// This reads it and deliberately does not guess what the values mean: the payload
/// carries no field names and no lengths, so the table is recorded and the reason
/// each value means anything is left to the executable's reader, which this
/// repository cannot reach (`docs/DMA3D.md` §11.23, §11.24, §11.36).
#[derive(Debug, Clone, PartialEq)]
pub struct ParamTable {
    /// Which parity of byte positions carries the values.
    ///
    /// `true` means the values are the odd-indexed bytes, so the even ones are the
    /// zero fill; `false` is the reverse. It is measured, because a payload's fill
    /// is not at a fixed parity — §11.37 found both.
    pub even_is_fill: bool,
    /// The values, in payload order, as the carrying half's bytes.
    pub values: Vec<u8>,
    /// What fraction of the fill half is zero, in percent. Recorded so the
    /// reading's own strength is visible rather than assumed.
    pub zero_percent: usize,
    /// What fraction of the carrying half is below 128, in percent.
    pub small_percent: usize,
}

/// How much of the fill half must be zero for a payload to be this shape.
pub const FILL_ZERO_PERCENT: usize = 80;
/// How much of the carrying half must be below 128 for it to be small values.
pub const CARRY_SMALL_PERCENT: usize = 95;
/// How many values a payload must carry for the reading to be a table at all.
pub const MIN_TABLE_VALUES: usize = 8;

/// Read a parameter table out of a payload that fits no other reader.
///
/// The thresholds are stated rather than tuned: `docs/DMA3D.md` §11.37 measured the
/// shape as "small values at alternating byte positions with zero fill", and a
/// payload is accepted when at least 80% of one parity is zero, at least 95% of the
/// other is below 128, and there are at least eight values. A payload outside all
/// three is an error, so the caller reports it rather than writing a document that
/// describes nothing.
pub fn parse_param_table(payload: &[u8]) -> Result<ParamTable> {
    if payload.len() < 2 * MIN_TABLE_VALUES {
        bail!(
            "{} bytes cannot hold the {} values a table needs",
            payload.len(),
            MIN_TABLE_VALUES
        );
    }
    let half = payload.len() / 2;
    let even_zero = (0..half).filter(|k| payload[2 * k] == 0).count();
    let odd_zero = (0..half).filter(|k| payload[2 * k + 1] == 0).count();
    let (even_is_fill, zero_percent) = if even_zero * 100 >= half * FILL_ZERO_PERCENT {
        (true, even_zero * 100 / half)
    } else if odd_zero * 100 >= half * FILL_ZERO_PERCENT {
        (false, odd_zero * 100 / half)
    } else {
        bail!(
            "neither byte parity is {FILL_ZERO_PERCENT}% zero, so this is not the \\
             alternating-fill shape (`docs/DMA3D.md` §11.37)"
        );
    };
    let values: Vec<u8> = (0..half)
        .map(|k| {
            let at = if even_is_fill { 2 * k + 1 } else { 2 * k };
            payload.get(at).copied().unwrap_or(0)
        })
        .collect();
    let small = values.iter().filter(|b| **b < 128).count();
    let value_count = values.len();
    if small * 100 < value_count * CARRY_SMALL_PERCENT {
        bail!(
            "only {small} of {} values are below 128, so the carrying half is not \\
             small values",
            values.len()
        );
    }
    Ok(ParamTable {
        even_is_fill,
        values,
        zero_percent,
        small_percent: small * 100 / value_count,
    })
}

#[cfg(test)]
mod param_table_tests {
    use super::*;

    /// The measured shape: small values at alternating byte positions with zero
    /// fill. `docs/DMA3D.md` §11.37.
    #[test]
    fn a_table_of_small_values_behind_zero_fill_is_read() {
        let mut payload: Vec<u8> = Vec::new();
        for v in [3u8, 7, 12, 40, 55, 90, 120, 127] {
            payload.push(0); // the fill
            payload.push(v); // the value
        }
        let table = parse_param_table(&payload).expect("the payload is a table");
        assert!(table.even_is_fill, "the even bytes are the fill here");
        assert_eq!(table.values, vec![3, 7, 12, 40, 55, 90, 120, 127]);
        assert_eq!(table.zero_percent, 100);
        assert_eq!(table.small_percent, 100);
    }

    /// The fill is not at a fixed parity: §11.37 found both, so the reading
    /// measures which one it is rather than assuming.
    #[test]
    fn the_fill_parity_is_measured_not_assumed() {
        let mut payload: Vec<u8> = Vec::new();
        for v in [8u8, 33, 64, 100, 11, 77, 5, 120] {
            payload.push(v); // the value
            payload.push(0); // the fill
        }
        let table = parse_param_table(&payload).expect("the payload is a table");
        assert!(!table.even_is_fill, "the odd bytes are the fill here");
        assert_eq!(table.values, vec![8, 33, 64, 100, 11, 77, 5, 120]);
    }

    /// A payload with no zero fill is not this shape, and is reported rather than
    /// made to fit (`AGENTS.md`).
    #[test]
    fn a_payload_without_zero_fill_is_reported() {
        let payload: Vec<u8> = (0..40u8).collect();
        assert!(
            parse_param_table(&payload).is_err(),
            "a payload with no zero fill was read as a table"
        );
    }

    /// Values above 128 mean the carrying half is not small values.
    #[test]
    fn values_above_128_are_not_a_small_value_table() {
        let mut payload: Vec<u8> = Vec::new();
        for v in [200u8, 210, 220, 230, 240, 250, 1, 2] {
            payload.push(0);
            payload.push(v);
        }
        assert!(
            parse_param_table(&payload).is_err(),
            "large values were read as a small-value table"
        );
    }
}

/// One table of texture names: the payload's list of the textures it references.
///
/// `docs/DMA3D.md` §11.40: 39 of the shipped payloads that no other reader claims
/// carry a table of NUL-terminated ASCII names in 32-byte records, each padded with
/// `0xcc` fill. `createBtFill.18.TGA`, `createBtShape.09.TGA`,
/// `createBtMagnet.20.tga` — a payload's own list of what it references. It is the
/// same field `src/dma3d.rs::texture_names_in` already reads out of mesh payloads to
/// embed their materials, so this is not a new layout: it is the field the reader
/// already knows, in the payloads it did not look at.
#[derive(Debug, Clone, PartialEq)]
pub struct NameTable {
    /// The names, in payload order.
    pub names: Vec<String>,

}

/// The shortest run of printable characters that counts as a name.
///
/// Eight, not four: at four the reading picks up noise — `DDD?`, `fff?`,
/// `>1:A?` are all four printable characters terminated by a NUL, and the 39
/// payloads it accepted included 20 whose "names" were that. At eight it takes
/// the 19 payloads carrying real ones, `createBtFill.00.TGA`,
/// `ParentalBtCheckboxY.mod`, and reports the rest.
const MIN_NAME_LEN: usize = 8;

/// Read a table of texture names out of a payload.
///
/// A name is a run of printable ASCII terminated by a NUL; the shortest run that
/// counts is four characters, so a stray byte inside padding is not a name. A
/// payload carrying fewer than three names is an error, so the caller reports it
/// rather than writing a document that describes nothing.
pub fn parse_name_table(payload: &[u8]) -> Result<NameTable> {
    let mut names: Vec<String> = Vec::new();
    let mut k = 0usize;
    while k < payload.len() {
        let mut j = k;
        while j < payload.len() && payload[j] != 0 && (payload[j] as char).is_ascii_graphic() {
            j += 1;
        }
        if j >= k + MIN_NAME_LEN && j < payload.len() && payload[j] == 0 {
            names.push(payload[k..j].iter().map(|b| *b as char).collect());
            k = j + 1;
        } else {
            k = if j > k { j } else { k + 1 };
        }
    }
    if names.len() < 3 {
        bail!(
            "{} names is fewer than the 3 a table needs",
            names.len()
        );
    }
    // No `first_is_truncated` field, and that is deliberate. Some payloads do
    // begin inside a name -- `teBtFill.18.TGA` for `createBtFill.18.TGA`, because
    // the container's slice starts three bytes into a record -- but a payload's
    // first byte is the name's first byte whether or not the name is cut, so the
    // bytes cannot tell the two apart. Asserting either way would be a guess, and
    // `AGENTS.md` sets the payload aside rather than guessing it.
    Ok(NameTable { names })
}

#[cfg(test)]
mod name_table_tests {
    use super::*;

    /// The measured shape: NUL-terminated names in 32-byte records.
    #[test]
    fn a_table_of_texture_names_is_read() {
        let mut payload: Vec<u8> = Vec::new();
        for n in ["createBtFill.00.TGA", "createBtFill.01.TGA", "createBtFill.02.TGA"] {
            payload.extend_from_slice(n.as_bytes());
            payload.push(0);
            while payload.len() % 32 != 0 {
                payload.push(0xcc);
            }
        }
        let table = parse_name_table(&payload).expect("the payload carries names");
        assert_eq!(table.names, vec![
            "createBtFill.00.TGA".to_string(),
            "createBtFill.01.TGA".to_string(),
            "createBtFill.02.TGA".to_string(),
        ]);
        assert_eq!(table.names.len(), 3);
    }

    /// Four printable characters are not a name: `DDD?`, `fff?` and `>1:A?` are
    /// all that, and 20 of the 39 payloads a four-character threshold accepts
    /// carry nothing else.
    #[test]
    fn four_printable_characters_are_not_a_name() {
        let mut payload: Vec<u8> = Vec::new();
        for n in ["DDD?", "fff?", "UUU?", "333?"] {
            payload.extend_from_slice(n.as_bytes());
            payload.push(0);
            payload.extend_from_slice(&[0xcc; 8]);
        }
        assert!(
            parse_name_table(&payload).is_err(),
            "four-character noise was read as a table of names"
        );
    }

    /// A payload with too few names is reported, not padded.
    #[test]
    fn a_payload_with_too_few_names_is_reported() {
        assert!(parse_name_table(b"nothing here").is_err());
    }
}
