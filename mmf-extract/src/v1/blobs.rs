//! The first game's `Blobs` payloads — the `goofy 2.6.3` record format.
//!
//! `pack.BLB` holds 305 payloads, 73,061,910 bytes in total. They are the
//! first game's animation scripts: a header naming the objects an animation
//! drives, followed by the per-object record streams that move them. Nothing
//! here is a coded video, despite the size — see `docs/GAME1.md` §6.
//!
//! ## The payload
//!
//! ```text
//!  0        u16  0x000c                       the stamp's length prefix
//!  2        "goofy 2.6.3\0"      (12 bytes)   the writer's version stamp
//! 14        u32  object_count
//! 18        u32  object_bytes  == object_count * 12
//! 22        object_count x 12-byte objects:
//!               u32 kind        6..=0x32      selects the game-object class
//!               u32 parameter                  handed to that class by address
//!               u32 records                    how many records follow it
//! 22+ob     u32  record_bytes     == the bytes that remain
//! 26+ob     record_bytes of 8-byte records, in object order, each object's run
//!           closed by a record whose event is 0
//! ```
//!
//! ```text
//! record (8 bytes):  u16 a; u16 b; u32 event
//! ```
//!
//! Every one of those facts is measured against the game's own reader in
//! `DISNEYMA.EXE` (the first game's executable; `docs/GAME1.md` §1 has the
//! carve table) and holds in all 305 payloads:
//!
//! * the header parser `FUN_004b5c6b` reads the stamp, **compares it** against
//!   the string at `0x512aac`, and only then sets the stream's version field
//!   (`object + 0xa`) to `0x107`; it then reads the two header `u32`s, reads
//!   `object_bytes` bytes into an allocated section, and advances its cursor
//!   by `14 + 4 + 4 + object_bytes + 4` — which is why the records begin at
//!   `26 + object_bytes`;
//! * the object iterator `FUN_004b5ec0` (reached from the frame loop at
//!   `0x4a7e98`) reads one 12-byte object at a time and stores its **third**
//!   `u32` as the stream's record count (`object + 0x16`), resetting the
//!   record index (`object + 0x1a`) to zero. The **first** `u32` goes to
//!   `0x4a7ee0`, which switches on it to construct one of the game's object
//!   classes (cases 6 to `0x32`); the **second** is stored at `session + 0x54`
//!   and passed *by address* to the constructor `0x47b140`, which keeps the
//!   pointer at `this + 0xc8`;
//! * the record reader `FUN_004b5f34` consumes 8 bytes from a 128 KiB rolling
//!   window and returns `0` for an object's **first** record, `1` for a middle
//!   one and `2` for its **last** — so the per-object record count is what
//!   tells the game when an object's list is finished.
//!
//! `decode` checks every one of those relations on the payload it is given and
//! fails rather than emit something it cannot account for, which is what
//! `AGENTS.md` requires. What is *not* decoded is what the coordinates mean in
//! the game's world: `0x4a7bc4` scales them through a 16.16 multiply
//! (`FUN_004a7284`) and adds a stage origin read from the object at
//! `session + 0x3b`, so the stored `u16`s are not raw pixels. The values are
//! written out as stored, plus the reader's `-800` normalisation, so nothing
//! is lost and nothing is guessed.

use crate::container::{Container, ContainerKind};
use anyhow::{Context, Result, bail};
use std::path::Path;

/// The 14-byte stamp that opens every payload: `u16 0x000c` then the string.
pub const STAMP: &[u8] = b"\x00\x0cgoofy 2.6.3\x00";

/// Bytes in one object header. Measured as `object_bytes == count * 12` in all
/// 305 payloads, and the iterator `FUN_004b5ec0` reads exactly that many.
pub const OBJECT_BYTES: usize = 12;

/// Bytes in one record: `u16 a`, `u16 b`, `u32 event` — the 8 bytes
/// `FUN_004b5f34` advances its cursor by per record.
pub const RECORD_BYTES: usize = 8;

/// The `u32` at payload offset 14: how many object headers follow.
pub const OBJECT_COUNT_OFFSET: usize = 14;

/// The `u32` at payload offset 18: `object_count * OBJECT_BYTES`.
pub const OBJECT_BYTES_OFFSET: usize = 18;

/// Where the object headers start, just past the stamp and the two `u32`s.
pub const OBJECTS_OFFSET: usize = 22;

/// The width of the `u32` length tag between the object headers and the
/// records.
///
/// The tag sits at `22 + object_bytes` and the records follow it immediately.
/// The header parser advances its cursor past it **without reading it** (`add
/// dword [ebx + 0x20027], 4` at `0x4b5df4`), which is why this module treats
/// it as a written-but-unconsumed length tag and validates it against the
/// bytes that actually remain rather than trusting it.
pub const RECORD_BYTES_FIELD: usize = 4;

/// The `-800` the record reader adds (`add word [esi], 0xfce0` at `0x4b5fc7`)
/// when the stream's version field reads `0x107`.
///
/// The header parser sets that field to `0x107` only after the stamp matches,
/// so every first-game blob payload gets it. It is a stage offset in the
/// game's own coordinate space, not a byte-order fix, and it is applied after
/// the optional byte-swap.
pub const VERSION_107: u32 = 0x107;

/// `0xfce0` as a signed 16-bit value: the additive inverse of 800.
pub const STAGE_OFFSET: i32 = -800;

/// The smallest object kind the switch at `0x4a7ee0` handles.
pub const OBJECT_KIND_MIN: u32 = 6;

/// The largest object kind the switch at `0x4a7ee0` handles.
pub const OBJECT_KIND_MAX: u32 = 0x32;

/// One record: a coordinate pair and the event the frame loop dispatches on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Record {
    /// The first `u16`, as stored on the wire.
    pub a: u16,
    /// The second `u16`, as stored on the wire.
    pub b: u16,
    /// The event code. `0` closes each object's run.
    pub event: u32,
}

impl Record {
    /// Read one record from `payload` at `at`.
    ///
    /// All three fields are big-endian on the wire; the reader swaps them in
    /// place only when the stream's own byte-swap flag (`object + 0x22`) says
    /// the file was written in the other order, so one code path serves both
    /// and this decoder only ever sees the wire order.
    pub fn read(payload: &[u8], at: usize) -> Result<Record> {
        let end = at
            .checked_add(RECORD_BYTES)
            .context("record offset overflow")?;
        if end > payload.len() {
            bail!(
                "record at {at:#x} runs off the payload ({} bytes)",
                payload.len()
            );
        }
        Ok(Record {
            a: u16::from_be_bytes([payload[at], payload[at + 1]]),
            b: u16::from_be_bytes([payload[at + 2], payload[at + 3]]),
            event: u32::from_be_bytes([
                payload[at + 4],
                payload[at + 5],
                payload[at + 6],
                payload[at + 7],
            ]),
        })
    }

    /// The two coordinates after the reader's `-800` normalisation.
    ///
    /// This is what the game's own coordinate code sees; it is reported
    /// alongside the stored values rather than replacing them, because the
    /// stored values are what `--original` must reproduce.
    pub fn stage_coords(&self) -> (i32, i32) {
        (self.a as i32 + STAGE_OFFSET, self.b as i32 + STAGE_OFFSET)
    }

    /// Write this record back out, big-endian, as the game wrote it.
    pub fn write(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.a.to_be_bytes());
        out.extend_from_slice(&self.b.to_be_bytes());
        out.extend_from_slice(&self.event.to_be_bytes());
    }
}

/// One object header plus the records that belong to it.
#[derive(Debug, Clone)]
pub struct Object {
    /// The first `u32`: the game-object class, selected by the switch at
    /// `0x4a7ee0`.
    pub kind: u32,
    /// The second `u32`: handed to the constructed object by address and kept
    /// at `this + 0xc8`. Not yet interpreted.
    pub parameter: u32,
    /// The third `u32`: the number of records that follow this object.
    pub record_count: u32,
    /// The records themselves, in wire order, closed by an `event == 0`.
    pub records: Vec<Record>,
}

/// A fully decoded blob payload.
#[derive(Debug, Clone)]
pub struct Blob {
    /// The header's object count, which is also the number of [`Object`]s.
    pub object_count: u32,
    /// The header's `object_count * OBJECT_BYTES`.
    pub object_bytes: u32,
    /// The `u32` length tag between the object headers and the records.
    ///
    /// The header parser skips it without reading it, so it is carried through
    /// unchanged and validated to equal the bytes that actually remain.
    pub record_bytes: u32,
    /// The objects, in wire order.
    pub objects: Vec<Object>,
}

impl Blob {
    /// The stream's version, which is `0x107` for every payload this decoder
    /// accepts, because the stamp is what sets it.
    pub fn version(&self) -> u32 {
        VERSION_107
    }

    /// Every record of every object, in order.
    pub fn records(&self) -> impl Iterator<Item = &Record> {
        self.objects.iter().flat_map(|o| o.records.iter())
    }

    /// Decode one payload.
    ///
    /// Each relation below is measured in `docs/GAME1.md` §6 and asserted here
    /// rather than assumed, so a payload that does not fit is a failure to
    /// report instead of a guess to emit.
    pub fn decode(payload: &[u8]) -> Result<Blob> {
        if payload.len() < STAMP.len() {
            bail!(
                "payload is {} bytes, too short for the {} byte stamp",
                payload.len(),
                STAMP.len()
            );
        }
        if &payload[..STAMP.len()] != STAMP {
            bail!(
                "payload does not start with the goofy stamp (found {:02x?})",
                &payload[..STAMP.len()]
            );
        }

        let object_count = read_u32(payload, OBJECT_COUNT_OFFSET)?;
        let object_bytes = read_u32(payload, OBJECT_BYTES_OFFSET)?;
        if object_count == 0 {
            bail!("the header's object count is zero");
        }
        if object_bytes != object_count * OBJECT_BYTES as u32 {
            bail!(
                "the header's object_bytes is {object_bytes}, expected {} for {object_count} objects",
                object_count * OBJECT_BYTES as u32
            );
        }
        let objects_end = OBJECTS_OFFSET
            .checked_add(object_bytes as usize)
            .context("the object section runs past the payload")?;
        if objects_end + RECORD_BYTES_FIELD + RECORD_BYTES > payload.len() {
            bail!(
                "the object section ends at {objects_end:#x} but the payload is {} bytes",
                payload.len()
            );
        }

        let record_bytes = read_u32(payload, objects_end)?;
        let records_start = objects_end + RECORD_BYTES_FIELD;
        let remaining = payload.len() - records_start;
        if record_bytes as usize != remaining {
            bail!("the header's record_bytes is {record_bytes} but {remaining} bytes remain");
        }
        if remaining % RECORD_BYTES != 0 {
            bail!(
                "the record stream is {remaining} bytes, not a whole number of {RECORD_BYTES}-byte records"
            );
        }

        // The object headers.
        let mut objects = Vec::with_capacity(object_count as usize);
        let mut total_records: usize = 0;
        for index in 0..object_count as usize {
            let at = OBJECTS_OFFSET + index * OBJECT_BYTES;
            let kind = read_u32(payload, at)?;
            let parameter = read_u32(payload, at + 4)?;
            let count = read_u32(payload, at + 8)?;
            if !(OBJECT_KIND_MIN..=OBJECT_KIND_MAX).contains(&kind) {
                bail!(
                    "object {index} has kind {kind}, outside the {OBJECT_KIND_MIN}..={OBJECT_KIND_MAX} the switch at 0x4a7ee0 handles"
                );
            }
            total_records = total_records
                .checked_add(count as usize)
                .context("the object record counts overflow")?;
            objects.push(Object {
                kind,
                parameter,
                record_count: count,
                records: Vec::new(),
            });
        }
        if total_records != remaining / RECORD_BYTES {
            bail!(
                "the object record counts sum to {total_records} but the stream holds {} records",
                remaining / RECORD_BYTES
            );
        }

        // The record stream, partitioned by the objects' own counts.
        let mut at = records_start;
        for object in objects.iter_mut() {
            for _ in 0..object.record_count {
                object.records.push(Record::read(payload, at)?);
                at += RECORD_BYTES;
            }
            let last = object
                .records
                .last()
                .context("an object's record run is empty")?;
            if last.event != 0 {
                bail!(
                    "an object of kind {} ends on event {}, not the 0 terminator",
                    object.kind,
                    last.event
                );
            }
        }
        if at != payload.len() {
            bail!(
                "the record stream ends at {at:#x}, not the payload's {} bytes",
                payload.len()
            );
        }

        Ok(Blob {
            object_count,
            object_bytes,
            record_bytes,
            objects,
        })
    }

    /// Re-emit the payload byte for byte.
    ///
    /// The stamp, the two header counts, the object headers, the record length
    /// tag and every record are written back in wire order, so this is the
    /// inverse of [`Blob::decode`] and is what the round-trip test asserts.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(
            OBJECTS_OFFSET
                + self.object_bytes as usize
                + RECORD_BYTES_FIELD
                + self
                    .objects
                    .iter()
                    .map(|o| o.records.len() * RECORD_BYTES)
                    .sum::<usize>(),
        );
        out.extend_from_slice(STAMP);
        out.extend_from_slice(&self.object_count.to_be_bytes());
        out.extend_from_slice(&self.object_bytes.to_be_bytes());
        for object in &self.objects {
            out.extend_from_slice(&object.kind.to_be_bytes());
            out.extend_from_slice(&object.parameter.to_be_bytes());
            out.extend_from_slice(&object.record_count.to_be_bytes());
        }
        out.extend_from_slice(&self.record_bytes.to_be_bytes());
        for object in &self.objects {
            for record in &object.records {
                record.write(&mut out);
            }
        }
        out
    }
}

/// Read a big-endian `u32` at `off`.
fn read_u32(payload: &[u8], off: usize) -> Result<u32> {
    let end = off.checked_add(4).context("offset overflow")?;
    if end > payload.len() {
        bail!("u32 at {off:#x} runs off the payload");
    }
    Ok(u32::from_be_bytes([
        payload[off],
        payload[off + 1],
        payload[off + 2],
        payload[off + 3],
    ]))
}

/// Extract a first-game `Blobs` container.
///
/// `--original` writes every payload byte for byte, which needs no decoding at
/// all and is the archival safety net. The modern mode writes a JSON dump per
/// payload — complete enough to rebuild the payload from — plus a manifest of
/// what was measured.
pub fn extract_container(container: &Container, out_dir: &Path, original: bool) -> Result<usize> {
    if container.kind != ContainerKind::Blobs {
        bail!(
            "{}: expected a Blobs container, found {:?}",
            container.path.display(),
            container.kind
        );
    }
    if container.version != 1 {
        bail!(
            "{}: expected a version-1 container, found version {}",
            container.path.display(),
            container.version
        );
    }

    let n = container.resource_count as usize;
    if original {
        return write_originals(container, out_dir);
    }

    std::fs::create_dir_all(out_dir).with_context(|| format!("creating {}", out_dir.display()))?;
    let payloads = out_dir.join("payloads");
    std::fs::create_dir_all(&payloads)
        .with_context(|| format!("creating {}", payloads.display()))?;

    // Aggregate over the whole container for the manifest.
    let mut objects = 0usize;
    let mut records = 0usize;
    let mut events: std::collections::BTreeMap<u32, usize> = std::collections::BTreeMap::new();
    let mut kinds: std::collections::BTreeMap<u32, usize> = std::collections::BTreeMap::new();
    let mut written = 0usize;

    for index in 0..n {
        let name = container.name(index);
        let payload = container.payload(index);
        let blob = match Blob::decode(payload) {
            Ok(b) => b,
            Err(e) => {
                // Per-resource failure, not a fatal one: the safe-net is the
                // --original mode, and the rest of the container still
                // extracts.
                eprintln!(
                    "warning: {}: resource {index} '{name}': {e}",
                    container.path.display()
                );
                continue;
            }
        };
        objects += blob.objects.len();
        for object in &blob.objects {
            *kinds.entry(object.kind).or_insert(0) += 1;
            for record in &object.records {
                *events.entry(record.event).or_insert(0) += 1;
            }
        }
        records += blob.records().count();

        let safe = safe_name(name, index);
        let file = payloads.join(format!("{index:05}_{safe}.json"));
        std::fs::write(&file, blob_json(&blob, index, name))
            .with_context(|| format!("writing {}", file.display()))?;
        written += 1;
    }

    let mut manifest = String::new();
    manifest.push_str("# First-game Blobs container: the goofy 2.6.3 payloads\n\n");
    manifest.push_str(&format!("container: {}\n", container.path.display()));
    manifest.push_str(&format!("version: {}\n", container.version));
    manifest.push_str(&format!("payloads: {n}, decoded: {written}\n"));
    manifest.push_str(&format!("objects: {objects}\n"));
    manifest.push_str(&format!("records: {records}\n\n"));
    manifest.push_str("payload layout (docs/GAME1.md section 6):\n");
    manifest.push_str("  14-byte stamp, u32 object_count, u32 object_bytes == count * 12,\n");
    manifest.push_str("  object_count x 12-byte objects {u32 kind, u32 parameter, u32 records},\n");
    manifest.push_str("  u32 record_bytes, then the 8-byte records in object order.\n\n");
    manifest.push_str("object kinds (the switch at FUN_004a7ee0):\n");
    for (kind, how_many) in &kinds {
        manifest.push_str(&format!("  kind {kind:>3}: {how_many:>6} objects\n"));
    }
    manifest.push_str("\nrecord events:\n");
    for (event, how_many) in &events {
        manifest.push_str(&format!("  event {event:>3}: {how_many:>7} records\n"));
    }
    manifest.push_str(
        "\nEvent 0 closes each object's run; the record reader FUN_004b5f34\n\
         returns 0/1/2 for an object's first, middle and last record. The\n\
         coordinates are u16s that the reader adds -800 to (the stream's\n\
         version field is 0x107), and that FUN_004a7bc4 then scales through a\n\
         16.16 multiply and offsets by a stage origin, so they are not raw\n\
         pixels. Each JSON carries both the stored and the -800 values.\n",
    );

    std::fs::write(out_dir.join("manifest.txt"), manifest)
        .with_context(|| format!("writing {}", out_dir.join("manifest.txt").display()))?;
    Ok(written)
}

/// Write every payload byte for byte.
fn write_originals(container: &Container, out_dir: &Path) -> Result<usize> {
    std::fs::create_dir_all(out_dir).with_context(|| format!("creating {}", out_dir.display()))?;
    let mut written = 0usize;
    for index in 0..container.resource_count as usize {
        let name = container.name(index);
        let safe = safe_name(name, index);
        let file = out_dir.join(format!("{index:05}_{safe}.blb"));
        std::fs::write(&file, container.payload(index))
            .with_context(|| format!("writing {}", file.display()))?;
        written += 1;
    }
    Ok(written)
}

/// One payload as JSON: complete enough to rebuild it with [`Blob::encode`].
fn blob_json(blob: &Blob, index: usize, name: &str) -> String {
    let mut s = String::new();
    s.push_str("{\n");
    s.push_str(&format!("  \"format\": \"goofy 2.6.3 blob\",\n"));
    s.push_str(&format!("  \"index\": {index},\n"));
    s.push_str(&format!("  \"name\": \"{}\",\n", json_escape(name)));
    s.push_str(&format!(
        "  \"stamp\": \"{}\",\n",
        json_escape(&String::from_utf8_lossy(&STAMP[2..]))
    ));
    s.push_str(&format!("  \"version\": {},\n", blob.version()));
    s.push_str(&format!("  \"objects\": [\n"));
    for (oi, object) in blob.objects.iter().enumerate() {
        s.push_str("    {\n");
        s.push_str(&format!("      \"kind\": {},\n", object.kind));
        s.push_str(&format!("      \"parameter\": {},\n", object.parameter));
        s.push_str(&format!("      \"records\": {},\n", object.record_count));
        s.push_str("      \"path\": [");
        for (ri, record) in object.records.iter().enumerate() {
            let (sa, sb) = record.stage_coords();
            s.push_str(&format!(
                "[{},{},{},{},{}]",
                record.a, record.b, record.event, sa, sb
            ));
            if ri + 1 < object.records.len() {
                s.push(',');
            }
        }
        s.push_str("]\n");
        s.push_str("    }");
        if oi + 1 < blob.objects.len() {
            s.push(',');
        }
        s.push('\n');
    }
    s.push_str("  ]\n}\n");
    s
}

/// Quote a string for JSON.
fn json_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// A filename-safe resource name, falling back to the index.
fn safe_name(name: &str, index: usize) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let cleaned = cleaned.trim_matches('.').to_string();
    if cleaned.is_empty() {
        format!("payload{index:05}")
    } else {
        cleaned
    }
}
