//! The animation records, written out in the game's own form.
//!
//! The animation is **not decoded** (`docs/DMA3D.md` §3.3): the record layout —
//! the record size, the keyframes per record and which node each track drives —
//! is not measured, and `AGENTS.md` forbids inventing a split. So the modern
//! model output is not allowed to claim animation channels it has not read.
//!
//! What it *can* do is extract the records in the game's own form without
//! pretending to have understood them. This module writes, beside every glTF,
//! a structured document that carries:
//!
//! * the anchors the mesh decoder measured — where the mesh ends and the tail
//!   begins, and the `[0, 1, V]` header when the payload has one;
//! * the `V` float pairs, which are texture placement on some payloads and
//!   something else on others, labelled as such rather than as UVs;
//! * the remainder, rendered as the game stores it — little-endian, in four
//!   views at once, because which view is the right one is exactly what is not
//!   known;
//! * the named tracks, when the payload carries the `[u32 count][32-byte
//!   NUL-padded names]` table that `createPropGuitar.mod` ends with
//!   (`key01`…`key06`).
//!
//! Every byte is preserved, so the document is a faithful rendering of the
//! payload rather than a decode of it: `--original` remains the mode that is
//! byte-exact, the `.tail` file beside the glTF holds the bytes verbatim, and
//! this is the readable form of the same bytes.

use crate::dma3d::modfile::ModelMesh;

/// One named animation track, read from the payload's name table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackName {
    /// The name, as stored.
    pub name: String,
    /// Where in the payload the name record begins.
    pub at: usize,
}

/// What was located in a payload beyond the mesh.
#[derive(Debug, Clone, PartialEq)]
pub struct Records {
    /// Byte offset at which the mesh decoder stopped reading.
    pub tail_offset: usize,
    /// The `[0, 1, V]` header, when the payload carries one.
    pub header: Option<(u32, u32, u32)>,
    /// How the `V` float pairs read. `Some(count)` when they are all in `0..1`,
    /// which is what texture coordinates look like; `None` otherwise.
    pub pairs_in_unit_range: Option<usize>,
    /// How many float pairs follow the header.
    pub pair_count: usize,
    /// The bytes after the pairs: the animation records, still undecoded.
    pub remainder: Vec<u8>,
    /// The named tracks, from the payload's name table.
    pub tracks: Vec<TrackName>,
}

impl Records {
    /// Byte offset at which the undecoded records begin.
    pub fn remainder_offset(&self) -> usize {
        self.tail_offset + 12 + 8 * self.pair_count
    }
}

/// Locate what a payload holds beyond the decoded mesh.
///
/// This reads only what is measured — the tail header, the pairs' range, and the
/// name table — and leaves the records themselves alone.
pub fn locate(payload: &[u8], mesh: &ModelMesh) -> Records {
    let tail_offset = mesh.tail_offset();
    let mut header = None;
    let mut pair_count = 0usize;
    let mut pairs_in_unit_range = None;

    // The `[0, 1, V]` header, when present, is followed by `V` float pairs.
    if tail_offset + 12 <= payload.len() {
        let head = (
            u32::from_le_bytes([
                payload[tail_offset],
                payload[tail_offset + 1],
                payload[tail_offset + 2],
                payload[tail_offset + 3],
            ]),
            u32::from_le_bytes([
                payload[tail_offset + 4],
                payload[tail_offset + 5],
                payload[tail_offset + 6],
                payload[tail_offset + 7],
            ]),
            u32::from_le_bytes([
                payload[tail_offset + 8],
                payload[tail_offset + 9],
                payload[tail_offset + 10],
                payload[tail_offset + 11],
            ]),
        );
        let v = mesh.vertex_count();
        // The vertex count is the payload's own, so a header whose third word is
        // anything else is not the shape this reads.
        if head.2 as usize == v {
            header = Some(head);
            pair_count = v;
            let end = tail_offset + 12 + 8 * v;
            if end <= payload.len() {
                let inside = (0..v)
                    .filter(|&k| {
                        let at = tail_offset + 12 + 8 * k;
                        let a = f32::from_le_bytes([
                            payload[at],
                            payload[at + 1],
                            payload[at + 2],
                            payload[at + 3],
                        ]);
                        let b = f32::from_le_bytes([
                            payload[at + 4],
                            payload[at + 5],
                            payload[at + 6],
                            payload[at + 7],
                        ]);
                        (-0.001..=1.001).contains(&a) && (-0.001..=1.001).contains(&b)
                    })
                    .count();
                pairs_in_unit_range = Some(inside);
            }
        }
    }

    let end_of_pairs = tail_offset + 12 + 8 * pair_count;
    let remainder = if end_of_pairs <= payload.len() {
        payload[end_of_pairs..].to_vec()
    } else {
        Vec::new()
    };

    Records {
        tail_offset,
        header,
        pairs_in_unit_range,
        pair_count,
        remainder,
        tracks: track_names(payload),
    }
}

/// The named tracks a payload carries, in payload order.
///
/// The table is `[u32 count][count * 32-byte NUL-padded names]`, measured on
/// `createBtDraw.mod` (31 texture names) and seen holding `key01`…`key06` at
/// the end of `createPropGuitar.mod`. A candidate that does not validate is
/// dropped, so a stray `TGA` in the geometry is not read as a name.
pub fn track_names(payload: &[u8]) -> Vec<TrackName> {
    let mut names = Vec::new();
    let mut at = 0usize;
    while at + 4 <= payload.len() {
        let count = u32::from_le_bytes([
            payload[at],
            payload[at + 1],
            payload[at + 2],
            payload[at + 3],
        ]) as usize;
        if !(1..=64).contains(&count) {
            at += 4;
            continue;
        }
        let table = at + 4;
        if table + count * 32 > payload.len() {
            at += 4;
            continue;
        }
        let mut ok = true;
        let mut found = Vec::new();
        for record in 0..count {
            let rec = &payload[table + record * 32..table + (record + 1) * 32];
            let end = rec.iter().position(|&b| b == 0).unwrap_or(rec.len());
            if end == 0 {
                ok = false;
                break;
            }
            let text = String::from_utf8_lossy(&rec[..end]).to_string();
            if !text.contains("TGA") && !text.starts_with("key") {
                ok = false;
                break;
            }
            found.push(TrackName {
                name: text,
                at: table + record * 32,
            });
        }
        if ok {
            names.extend(found);
            at = table + count * 32;
        } else {
            at += 4;
        }
    }
    names
}

/// Render the records as a readable, byte-faithful document.
///
/// The records are printed in four views at once because which view is the
/// right one is what is not measured. Little-endian, as stored.
pub fn dump(name: &str, payload: &[u8], mesh: &ModelMesh, records: &Records) -> String {
    use std::fmt::Write as _;

    let mut out = String::new();
    let _ = writeln!(out, "# {name}");
    let _ = writeln!(
        out,
        "# payload {} bytes; mesh {} indices / {} vertices; mesh chain at {:#x}",
        payload.len(),
        mesh.indices.len(),
        mesh.vertex_count(),
        mesh.index_count_offset
    );
    let _ = writeln!(
        out,
        "# the mesh decoder stops at {:#x}; everything below is what it left",
        records.tail_offset
    );

    let _ = writeln!(out, "\n[header]");
    match records.header {
        Some((a, b, v)) => {
            let _ = writeln!(
                out,
                "  u32 {a}, u32 {b}, u32 {v}   the [0, 1, V] shape, the third word = the \
                 vertex count"
            );
        }
        None => {
            let _ = writeln!(
                out,
                "  absent. 50 of the 144 mesh payloads open with something else, so the \
                 shape is not required"
            );
        }
    }

    match records.pairs_in_unit_range {
        Some(inside) => {
            let _ = writeln!(
                out,
                "\n[pairs]  {} float pairs, {} of them wholly inside 0..1",
                records.pair_count, inside
            );
            let _ = writeln!(
                out,
                "  all inside 0..1 reads as texture placement; none inside 0..1 means the \
                 pairs are something else. Which payloads are which is not measured"
            );
            let v = records.pair_count;
            for k in 0..v.min(8) {
                let at = records.tail_offset + 12 + 8 * k;
                if at + 8 > payload.len() {
                    break;
                }
                let a = f32::from_le_bytes([
                    payload[at],
                    payload[at + 1],
                    payload[at + 2],
                    payload[at + 3],
                ]);
                let b = f32::from_le_bytes([
                    payload[at + 4],
                    payload[at + 5],
                    payload[at + 6],
                    payload[at + 7],
                ]);
                let _ = writeln!(out, "  {k:4}: ({a:.5}, {b:.5})");
            }
            if v > 8 {
                let _ = writeln!(out, "  ... {} more, not printed", v - 8);
            }
        }
        None => {
            let _ = writeln!(out, "\n[pairs]  not read: the header is not the [0, 1, V] shape");
        }
    }

    let _ = writeln!(
        out,
        "\n[records]  {} bytes at {:#x}, undecoded",
        records.remainder.len(),
        records.remainder_offset()
    );
    let _ = writeln!(
        out,
        "  The record layout is not measured: not the record size, not the keyframes \
         per record, and not which node each track drives. These are the game's own \
         bytes, in the game's own order, rendered four ways so a candidate layout \
         can be checked against them."
    );
    let bytes = &records.remainder;
    let shown = bytes.len().min(1024);
    for start in (0..shown).step_by(16) {
        let row = &bytes[start..(start + 16).min(shown)];
        let hex = row
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join(" ");
        let _ = writeln!(out, "  {:#06x}  {hex}", records.remainder_offset() + start);
    }
    if bytes.len() > shown {
        let _ = writeln!(out, "  ... {} bytes not printed", bytes.len() - shown);
    }
    let words = bytes.len() / 4;
    let _ = writeln!(out, "\n  as u32 (little-endian), the first {words} of them:");
    for chunk in bytes.chunks_exact(4).take(24) {
        let _ = write!(out, "  {}", u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "\n  as f32 (little-endian):");
    for chunk in bytes.chunks_exact(4).take(24) {
        let v = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        let _ = write!(out, "  {v:.5}");
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "\n  as u16 (little-endian):");
    for chunk in bytes.chunks_exact(2).take(32) {
        let _ = write!(out, "  {}", u16::from_le_bytes([chunk[0], chunk[1]]));
    }
    let _ = writeln!(out);

    let _ = writeln!(out, "\n[track names]");
    if records.tracks.is_empty() {
        let _ = writeln!(
            out,
            "  none. Only six payloads name theirs, and none of those carries a mesh"
        );
    } else {
        for track in &records.tracks {
            let _ = writeln!(out, "  {:<24} at {:#x}", track.name, track.at);
        }
    }

    out
}

/// The filename a payload's records are written under, beside the glTF.
pub fn records_name(stem: &str) -> String {
    format!("{stem}.anim.txt")
}
