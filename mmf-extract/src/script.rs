//! `MMFW Scripts` resources (`.MMS`).
//!
//! A Scripts container is not event bytecode. It is a serialized **global
//! object database**: every resource is one named object, and the container
//! metadata names that object's class. Each payload starts with
//! `element_count` fixed-size records (the record size comes from the class
//! table), optionally followed by a class-specific string tail.
//!
//! Two output modes are provided:
//!
//! * `--original` writes each resource payload verbatim. Scripts payloads are
//!   stored uncompressed, so this is the exact bytes the runtime reads.
//! * the default mode writes a structured, annotated corpus: `container.json`
//!   (metadata), `types.txt`, `strings.txt`, and one text file per object with
//!   every fixed record and every decoded string. Opaque objects fall back to
//!   an annotated hexdump.
//!
//! See `docs/SCRIPTS.md` for the reverse-engineered layout and measurements.

use anyhow::{bail, Context, Result};
use crate::cli::sanitize;
use crate::container::{read_u16, read_u32, Container, ContainerKind, ScriptMeta};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

/// How a payload is split into records and a tail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Layout {
    /// `count * record_size` bytes and nothing else.
    Records,
    /// `count * record_size` bytes, then the object name as a NUL string.
    Animation,
    /// `count * record_size` bytes, then one NUL-terminated text string.
    CreditsText,
    /// `count * record_size` bytes, then `count` NUL-terminated names.
    BrushNames,
    /// `count * 8` bytes of `(u16 ref, u16 name_offset, u32 flags)`, then a
    /// name blob.
    AnimBrush,
    /// `count * 4` bytes of u32 string offsets, then a shared NUL-string blob.
    TextList,
    /// `count * 10` byte `(u16 id, u32 offset, u32 reserved)` entries plus a
    /// shared string blob.
    StringTable,
    /// Structure not recognised; the payload is written as a hexdump.
    Opaque,
}

impl Layout {
    fn name(self) -> &'static str {
        match self {
            Self::Records => "records",
            Self::Animation => "animation",
            Self::CreditsText => "credits-text",
            Self::BrushNames => "brush-names",
            Self::AnimBrush => "anim-brush",
            Self::TextList => "text-list",
            Self::StringTable => "string-table",
            Self::Opaque => "opaque",
        }
    }
}

/// Extract the scripts of a container, returning how many were written.
///
/// Individual write failures are reported on stderr and skipped; the return
/// value is the number of objects that were actually written.
pub fn extract_container(
    container: &Container,
    out_dir: &Path,
    original: bool,
) -> Result<usize> {
    if container.kind != ContainerKind::Scripts {
        bail!(
            "{}: expected a Scripts container, found {:?}",
            container.path.display(),
            container.kind
        );
    }
    let meta = container
        .script
        .as_ref()
        .with_context(|| format!("{}: missing script metadata", container.path.display()))?;

    std::fs::create_dir_all(out_dir)
        .with_context(|| format!("creating {}", out_dir.display()))?;

    if original {
        write_originals(container, out_dir)
    } else {
        write_modern(container, meta, out_dir)
    }
}

/// Write every payload byte-for-byte.
///
/// Scripts payloads are not LZ77 compressed, so "decompressed payload" and
/// "stored payload" are the same bytes.
fn write_originals(container: &Container, out_dir: &Path) -> Result<usize> {
    let mut written = 0usize;
    let mut failed = 0usize;

    for index in 0..container.resource_count as usize {
        let name = sanitize(container.name(index), index);
        let file = out_dir.join(format!("{index:05}_{name}.bin"));
        match std::fs::write(&file, container.payload(index)) {
            Ok(()) => written += 1,
            Err(e) => {
                failed += 1;
                eprintln!(
                    "warning: could not write {} (object {index} '{}'): {e}",
                    file.display(),
                    container.name(index)
                );
            }
        }
    }

    if failed > 0 {
        eprintln!(
            "warning: {} of {} script object payloads could not be written",
            failed,
            container.resource_count
        );
    }
    Ok(written)
}

/// Write the structured corpus: metadata, tables, strings and per-object dumps.
fn write_modern(
    container: &Container,
    meta: &ScriptMeta,
    out_dir: &Path,
) -> Result<usize> {
    let objects_dir = out_dir.join("objects");
    std::fs::create_dir_all(&objects_dir)
        .with_context(|| format!("creating {}", objects_dir.display()))?;

    let n = container.resource_count as usize;
    let mut type_counts = vec![0usize; meta.types.len()];
    for &t in &meta.object_types {
        if let Some(count) = type_counts.get_mut(t as usize) {
            *count += 1;
        }
    }

    // Classify every object once; the same decision is used for the JSON
    // coverage block and for the per-object writer.
    let mut layouts = Vec::with_capacity(n);
    let mut layout_counts: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut payload_bytes = 0usize;
    let mut record_bytes = 0usize;
    let mut tail_bytes = 0usize;
    let mut opaque_bytes = 0usize;

    for index in 0..n {
        let layout = classify(container, meta, index);
        layouts.push(layout);
        *layout_counts.entry(layout.name()).or_default() += 1;

        let len = container.payload_len(index);
        payload_bytes += len;
        if layout == Layout::Opaque {
            opaque_bytes += len;
        } else {
            let (count, record_size) = element_info(meta, index);
            let fixed = count * record_size;
            record_bytes += fixed;
            tail_bytes += len - fixed;
        }
    }

    write_container_json(
        container,
        meta,
        out_dir,
        &layouts,
        &type_counts,
        &layout_counts,
        payload_bytes,
        record_bytes,
        tail_bytes,
        opaque_bytes,
    )?;
    write_types_txt(container, meta, out_dir, &type_counts)?;
    write_strings_txt(container, meta, out_dir, &layouts)?;

    let mut written = 0usize;
    let mut failed = 0usize;
    for index in 0..n {
        let name = sanitize(container.name(index), index);
        let file = objects_dir.join(format!("{index:05}_{name}.txt"));
        let result = (|| -> Result<()> {
            let handle = File::create(&file)
                .with_context(|| format!("creating {}", file.display()))?;
            let mut writer = BufWriter::new(handle);
            write_object_dump(&mut writer, container, meta, index, layouts[index])
                .with_context(|| format!("writing {}", file.display()))?;
            writer.flush()?;
            Ok(())
        })();

        match result {
            Ok(()) => written += 1,
            Err(e) => {
                failed += 1;
                eprintln!("warning: script object {index} ('{}'): {e:#}", container.name(index));
            }
        }
    }

    if failed > 0 {
        eprintln!(
            "warning: {} of {} script object dumps could not be written",
            failed, n
        );
    }
    Ok(written)
}

/// Number of fixed records and their size for object `index`.
fn element_info(meta: &ScriptMeta, index: usize) -> (usize, usize) {
    let count = meta.element_counts.get(index).copied().unwrap_or(0) as usize;
    let record_size = meta
        .object_types
        .get(index)
        .and_then(|&t| meta.type_aux.get(t as usize))
        .copied()
        .unwrap_or(0) as usize;
    (count, record_size)
}

/// Type index, type name and record size for object `index`.
fn type_info(meta: &ScriptMeta, index: usize) -> Option<(usize, &str, usize)> {
    let type_index = *meta.object_types.get(index)? as usize;
    let name = meta.types.get(type_index)?;
    let record_size = *meta.type_aux.get(type_index)? as usize;
    Some((type_index, name, record_size))
}

/// Decide how an object's payload is laid out. Anything unexpected becomes
/// `Opaque` so extraction can still continue.
fn classify(container: &Container, meta: &ScriptMeta, index: usize) -> Layout {
    let Some((_, type_name, record_size)) = type_info(meta, index) else {
        return Layout::Opaque;
    };
    let (count, _) = element_info(meta, index);
    let payload = container.payload(index);

    if record_size == 0 {
        return if payload.is_empty() {
            Layout::Records
        } else {
            Layout::Opaque
        };
    }
    let fixed = match count.checked_mul(record_size) {
        Some(fixed) if fixed <= payload.len() => fixed,
        _ => return Layout::Opaque,
    };
    let tail = &payload[fixed..];

    match type_name {
        "Animation" => {
            if nul_strings_exact(tail, 1).is_some() {
                Layout::Animation
            } else {
                Layout::Opaque
            }
        }
        "creditsTextEntry" => {
            if nul_strings_exact(tail, 1).is_some() {
                Layout::CreditsText
            } else {
                Layout::Opaque
            }
        }
        "StdBrush" => {
            if nul_strings_exact(tail, count).is_some() {
                Layout::BrushNames
            } else {
                Layout::Opaque
            }
        }
        "AnimBrush" => {
            if record_size == 8
                && anim_brush_entries(payload, count, container.big_endian).is_some()
                && nul_strings_exact(tail, count).is_some()
            {
                Layout::AnimBrush
            } else {
                Layout::Opaque
            }
        }
        "TextList" => {
            if text_list_entries(payload, count, container.big_endian).is_some() {
                Layout::TextList
            } else {
                Layout::Opaque
            }
        }
        "StringTable" => {
            if !tail.is_empty() && count > 0 {
                Layout::StringTable
            } else {
                Layout::Opaque
            }
        }
        _ => {
            if fixed == payload.len() {
                Layout::Records
            } else {
                Layout::Opaque
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Decoders shared by the dump and string-table writers
// ---------------------------------------------------------------------------

/// Parse exactly `expected` NUL-terminated strings that consume all of `data`.
fn nul_strings_exact(data: &[u8], expected: usize) -> Option<Vec<&[u8]>> {
    let mut pos = 0usize;
    let mut strings = Vec::with_capacity(expected);
    for _ in 0..expected {
        let rest = data.get(pos..)?;
        let end = rest.iter().position(|&b| b == 0)?;
        strings.push(&rest[..end]);
        pos += end + 1;
    }
    if pos == data.len() {
        Some(strings)
    } else {
        None
    }
}

/// One `(id, offset, reserved)` StringTable record plus the shared blob.
fn string_table_entries<'a>(
    payload: &'a [u8],
    count: usize,
    big_endian: bool,
) -> Option<(Vec<(u16, u32, u32)>, &'a [u8])> {
    let fixed = count.checked_mul(10)?;
    if count == 0 || fixed > payload.len() {
        return None;
    }
    let mut entries = Vec::with_capacity(count);
    for j in 0..count {
        let record = &payload[j * 10..j * 10 + 10];
        entries.push((
            read_u16(record, 0, big_endian),
            read_u32(record, 2, big_endian),
            read_u32(record, 6, big_endian),
        ));
    }
    Some((entries, &payload[fixed..]))
}

/// Read a string at `offset` inside a shared NUL-string blob.
fn string_at_offset(blob: &[u8], offset: u32) -> Option<&[u8]> {
    let start = offset as usize;
    if start > blob.len() {
        return None;
    }
    let rest = &blob[start..];
    let end = rest.iter().position(|&b| b == 0)?;
    Some(&rest[..end])
}

/// AnimBrush payload: `count` entries of `(u16 ref, u16 name_offset, u32
/// flags)` followed by a shared string blob. Returns the entries and the blob.
fn anim_brush_entries(
    payload: &[u8],
    count: usize,
    big_endian: bool,
) -> Option<(Vec<(u16, u16, u32)>, &[u8])> {
    let fixed = count.checked_mul(8)?;
    if count == 0 || fixed > payload.len() {
        return None;
    }
    let blob = &payload[fixed..];
    let mut entries = Vec::with_capacity(count);
    for j in 0..count {
        let record = &payload[j * 8..j * 8 + 8];
        let name_offset = read_u16(record, 2, big_endian);
        string_at_offset(blob, name_offset as u32)?;
        entries.push((
            read_u16(record, 0, big_endian),
            name_offset,
            read_u32(record, 4, big_endian),
        ));
    }
    Some((entries, blob))
}

/// TextList payload: `count` u32 offsets relative to a shared NUL-string blob
/// that follows the offset array. Every offset must resolve inside the blob.
fn text_list_entries<'a>(
    payload: &'a [u8],
    count: usize,
    big_endian: bool,
) -> Option<(Vec<u32>, &'a [u8])> {
    let fixed = count.checked_mul(4)?;
    if count == 0 || fixed > payload.len() {
        return None;
    }

    let blob = &payload[fixed..];
    let mut offsets = Vec::with_capacity(count);
    for j in 0..count {
        let offset = read_u32(payload, j * 4, big_endian);
        string_at_offset(blob, offset)?;
        offsets.push(offset);
    }
    Some((offsets, blob))
}

// ---------------------------------------------------------------------------
// container.json
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn write_container_json(
    container: &Container,
    meta: &ScriptMeta,
    out_dir: &Path,
    layouts: &[Layout],
    type_counts: &[usize],
    layout_counts: &BTreeMap<&'static str, usize>,
    payload_bytes: usize,
    record_bytes: usize,
    tail_bytes: usize,
    opaque_bytes: usize,
) -> Result<()> {
    let path = out_dir.join("container.json");
    let handle = File::create(&path)
        .with_context(|| format!("creating {}", path.display()))?;
    let mut w = BufWriter::new(handle);

    writeln!(w, "{{")?;
    writeln!(w, "  \"format\": \"MMFW Scripts\",")?;
    writeln!(
        w,
        "  \"source\": \"{}\",",
        json_escape(&container.path.display().to_string())
    )?;
    writeln!(
        w,
        "  \"endian\": \"{}\",",
        if container.big_endian { "big" } else { "little" }
    )?;
    writeln!(w, "  \"container_version\": {},", container.version)?;
    writeln!(w, "  \"resource_count\": {},", container.resource_count)?;
    writeln!(w, "  \"script_metadata\": {{")?;
    writeln!(w, "    \"version\": {},", meta.version)?;
    writeln!(
        w,
        "    \"unknown_fields\": [{}, {}, {}],",
        meta.unknown_1, meta.unknown_2, meta.unknown_3
    )?;
    writeln!(w, "    \"type_count\": {},", meta.type_count)?;
    writeln!(w, "    \"offset\": {},", meta.metadata_start)?;
    writeln!(w, "    \"end\": {},", meta.metadata_end)?;
    writeln!(
        w,
        "    \"bytes\": {}",
        meta.metadata_end - meta.metadata_start
    )?;
    writeln!(w, "  }},")?;

    writeln!(w, "  \"coverage\": {{")?;
    writeln!(w, "    \"payload_bytes\": {payload_bytes},")?;
    writeln!(w, "    \"record_bytes\": {record_bytes},")?;
    writeln!(w, "    \"tail_bytes\": {tail_bytes},")?;
    writeln!(w, "    \"opaque_bytes\": {opaque_bytes},")?;
    writeln!(w, "    \"layouts\": {{")?;
    let layout_len = layout_counts.len();
    for (i, (name, count)) in layout_counts.iter().enumerate() {
        let comma = if i + 1 == layout_len { "" } else { "," };
        writeln!(w, "      \"{name}\": {count}{comma}")?;
    }
    writeln!(w, "    }}")?;
    writeln!(w, "  }},")?;

    writeln!(w, "  \"types\": [")?;
    for (index, name) in meta.types.iter().enumerate() {
        let comma = if index + 1 == meta.types.len() { "" } else { "," };
        writeln!(
            w,
            "    {{\"index\": {index}, \"name\": \"{}\", \"record_size\": {}, \"object_count\": {}}}{comma}",
            json_escape(name),
            meta.type_aux.get(index).copied().unwrap_or(0),
            type_counts.get(index).copied().unwrap_or(0)
        )?;
    }
    writeln!(w, "  ],")?;

    let n = container.resource_count as usize;
    writeln!(w, "  \"objects\": [")?;
    for index in 0..n {
        let (type_index, type_name, record_size) = type_info(meta, index).unwrap_or((0, "", 0));
        let (count, _) = element_info(meta, index);
        let safe = sanitize(container.name(index), index);
        let comma = if index + 1 == n { "" } else { "," };
        writeln!(
            w,
            "    {{\"index\": {index}, \"name\": \"{}\", \"type_index\": {type_index}, \"type\": \"{}\", \"record_size\": {record_size}, \"elements\": {count}, \"payload_offset\": {}, \"payload_size\": {}, \"layout\": \"{}\", \"file\": \"objects/{index:05}_{safe}.txt\"}}{comma}",
            json_escape(container.name(index)),
            json_escape(type_name),
            container.offsets[index],
            container.payload_len(index),
            layouts.get(index).copied().unwrap_or(Layout::Opaque).name()
        )?;
    }
    writeln!(w, "  ]")?;
    writeln!(w, "}}")?;
    w.flush()?;
    Ok(())
}

// ---------------------------------------------------------------------------
// types.txt / strings.txt
// ---------------------------------------------------------------------------

fn write_types_txt(
    container: &Container,
    meta: &ScriptMeta,
    out_dir: &Path,
    type_counts: &[usize],
) -> Result<()> {
    let path = out_dir.join("types.txt");
    let handle = File::create(&path)
        .with_context(|| format!("creating {}", path.display()))?;
    let mut w = BufWriter::new(handle);

    writeln!(w, "# Scripts type table for {}", container.path.display())?;
    writeln!(w, "# idx  record_size  objects  name")?;
    for (index, name) in meta.types.iter().enumerate() {
        writeln!(
            w,
            "{:>3}  {:>11}  {:>7}  {}",
            index,
            meta.type_aux.get(index).copied().unwrap_or(0),
            type_counts.get(index).copied().unwrap_or(0),
            name
        )?;
    }
    w.flush()?;
    Ok(())
}

fn write_strings_txt(
    container: &Container,
    meta: &ScriptMeta,
    out_dir: &Path,
    layouts: &[Layout],
) -> Result<()> {
    let path = out_dir.join("strings.txt");
    let handle = File::create(&path)
        .with_context(|| format!("creating {}", path.display()))?;
    let mut w = BufWriter::new(handle);

    writeln!(w, "# Strings decoded from {}", container.path.display())?;
    writeln!(w, "# Escapes: \\\" \\\\ \\0 \\n \\r \\t, other bytes as \\xNN")?;
    writeln!(w)?;
    writeln!(w, "[type names]")?;
    for (index, name) in meta.types.iter().enumerate() {
        writeln!(w, "type[{index:3}] \"{}\"", escape_bytes(name.as_bytes()))?;
    }

    writeln!(w)?;
    writeln!(w, "[object names]")?;
    for index in 0..container.resource_count as usize {
        writeln!(
            w,
            "object[{index:04}] \"{}\"",
            escape_bytes(container.name(index).as_bytes())
        )?;
    }

    writeln!(w)?;
    writeln!(w, "[payload strings]")?;
    let mut total = 0usize;
    let n = container.resource_count as usize;
    for index in 0..n {
        let payload = container.payload(index);
        let (count, _) = element_info(meta, index);
        let name = escape_bytes(container.name(index).as_bytes());

        match layouts.get(index).copied().unwrap_or(Layout::Opaque) {
            Layout::StringTable => {
                let Some((entries, blob)) =
                    string_table_entries(payload, count, container.big_endian)
                else {
                    continue;
                };
                for (id, offset, _) in entries {
                    if matches!(offset, 0xfffd..=0xffff) {
                        continue;
                    }
                    if let Some(s) = string_at_offset(blob, offset) {
                        writeln!(
                            w,
                            "StringTable[{index:04}] \"{name}\" id={id} offset=0x{offset:04x} \"{}\"",
                            escape_bytes(s)
                        )?;
                        total += 1;
                    }
                }
            }
            Layout::CreditsText => {
                let (count, record_size) = element_info(meta, index);
                let fixed = count * record_size;
                if let Some(strings) = nul_strings_exact(&payload[fixed..], 1) {
                    writeln!(
                        w,
                        "creditsTextEntry[{index:04}] \"{name}\" \"{}\"",
                        escape_bytes(strings[0])
                    )?;
                    total += 1;
                }
            }
            Layout::TextList => {
                if let Some((offsets, blob)) =
                    text_list_entries(payload, count, container.big_endian)
                {
                    for (k, offset) in offsets.iter().enumerate() {
                        if let Some(s) = string_at_offset(blob, *offset) {
                            writeln!(
                                w,
                                "TextList[{index:04}] \"{name}\" [{k}] offset=0x{offset:04x} \"{}\"",
                                escape_bytes(s)
                            )?;
                            total += 1;
                        }
                    }
                }
            }
            Layout::BrushNames => {
                let (count, record_size) = element_info(meta, index);
                let fixed = count * record_size;
                if let Some(strings) = nul_strings_exact(&payload[fixed..], count) {
                    for (k, s) in strings.iter().enumerate() {
                        writeln!(
                            w,
                            "StdBrush[{index:04}] \"{name}\" [{k}] \"{}\"",
                            escape_bytes(s)
                        )?;
                        total += 1;
                    }
                }
            }
            Layout::AnimBrush => {
                if let Some((entries, blob)) =
                    anim_brush_entries(payload, count, container.big_endian)
                {
                    for (k, (_, offset, _)) in entries.iter().enumerate() {
                        if let Some(s) = string_at_offset(blob, *offset as u32) {
                            writeln!(
                                w,
                                "AnimBrush[{index:04}] \"{name}\" [{k}] offset=0x{offset:04x} \"{}\"",
                                escape_bytes(s)
                            )?;
                            total += 1;
                        }
                    }
                }
            }
            _ => {}
        }
    }
    writeln!(w)?;
    writeln!(w, "# {total} payload strings decoded")?;
    w.flush()?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Per-object dump
// ---------------------------------------------------------------------------

fn write_object_dump<W: Write>(
    w: &mut W,
    container: &Container,
    meta: &ScriptMeta,
    index: usize,
    layout: Layout,
) -> Result<()> {
    let payload = container.payload(index);
    let (type_index, type_name, record_size) = type_info(meta, index).unwrap_or((0, "?", 0));
    let (count, _) = element_info(meta, index);
    let fixed = count * record_size;

    writeln!(w, "# Object {index}: {}", container.name(index))?;
    writeln!(w, "# Type: {type_name} (index {type_index})")?;
    writeln!(w, "# Elements: {count} x {record_size} bytes = {fixed}")?;
    writeln!(
        w,
        "# Payload: {} bytes at container offset 0x{:x}",
        payload.len(),
        container.offsets[index]
    )?;
    writeln!(w, "# Layout: {}", layout.name())?;

    if layout == Layout::Opaque {
        writeln!(w, "# No structural decode; full verbatim hexdump follows.")?;
        write_hexdump(w, payload, 0)?;
        return Ok(());
    }

    match layout {
        Layout::Records => {
            write_records(w, payload, count, record_size, type_name, container.big_endian)?;
        }
        Layout::Animation => {
            write_records(w, payload, count, record_size, type_name, container.big_endian)?;
            write_nul_tail(
                w,
                &payload[fixed..],
                1,
                "object name",
                fixed,
            )?;
        }
        Layout::CreditsText => {
            write_records(w, payload, count, record_size, type_name, container.big_endian)?;
            write_nul_tail(w, &payload[fixed..], 1, "credits text", fixed)?;
        }
        Layout::BrushNames => {
            write_records(w, payload, count, record_size, type_name, container.big_endian)?;
            write_nul_tail(w, &payload[fixed..], count, "brush names", fixed)?;
        }
        Layout::AnimBrush => {
            write_anim_brush(w, payload, count, fixed, container.big_endian)?;
        }
        Layout::TextList => {
            write_text_list(w, payload, count, container.big_endian)?;
        }
        Layout::StringTable => {
            write_string_table(w, payload, count, container.big_endian)?;
        }
        Layout::Opaque => unreachable!(),
    }

    writeln!(
        w,
        "# All {} payload bytes are represented above.",
        payload.len()
    )?;
    Ok(())
}

fn write_records<W: Write>(
    w: &mut W,
    payload: &[u8],
    count: usize,
    record_size: usize,
    type_name: &str,
    big_endian: bool,
) -> std::io::Result<()> {
    writeln!(w, "## Fixed records ({count} x {record_size} bytes)")?;
    writeln!(w, "# [index] raw bytes | decoded fields")?;
    for j in 0..count {
        let start = j * record_size;
        let record = &payload[start..start + record_size];
        write!(w, "  [{j:4}] {}", hex_bytes(record))?;
        write_record_fields(w, record, type_name, big_endian)?;
        writeln!(w)?;
    }
    Ok(())
}

fn write_record_fields<W: Write>(
    w: &mut W,
    record: &[u8],
    type_name: &str,
    big_endian: bool,
) -> std::io::Result<()> {
    match record.len() {
        2 => {
            let a = read_u16(record, 0, big_endian);
            write!(w, " | u16 {a} (i16 {})", a as i16)?;
        }
        4 => {
            let a = read_u32(record, 0, big_endian);
            write!(w, " | u32 {a} (i32 {})", a as i32)?;
        }
        8 => {
            let a0 = read_u16(record, 0, big_endian);
            let a1 = read_u16(record, 2, big_endian);
            let a2 = read_u16(record, 4, big_endian);
            let a3 = read_u16(record, 6, big_endian);
            let b0 = read_u32(record, 0, big_endian);
            let b1 = read_u32(record, 4, big_endian);
            write!(
                w,
                " | u16 {a0} {a1} {a2} {a3} | u32 {b0} {b1}"
            )?;
        }
        10 if type_name == "Animation" => {
            let op = read_u16(record, 0, big_endian);
            let a0 = read_u16(record, 2, big_endian);
            let a1 = read_u16(record, 4, big_endian);
            let a2 = read_u16(record, 6, big_endian);
            let a3 = read_u16(record, 8, big_endian);
            write!(w, " | op {op} args {a0} {a1} {a2} {a3}")?;
        }
        10 => {
            let a0 = read_u16(record, 0, big_endian);
            let a1 = read_u16(record, 2, big_endian);
            let a2 = read_u16(record, 4, big_endian);
            let a3 = read_u16(record, 6, big_endian);
            let a4 = read_u16(record, 8, big_endian);
            write!(w, " | u16 {a0} {a1} {a2} {a3} {a4}")?;
        }
        12 => {
            let a0 = read_u16(record, 0, big_endian);
            let a1 = read_u16(record, 2, big_endian);
            let a2 = read_u16(record, 4, big_endian);
            let a3 = read_u16(record, 6, big_endian);
            let a4 = read_u16(record, 8, big_endian);
            let a5 = read_u16(record, 10, big_endian);
            let b0 = read_u32(record, 0, big_endian);
            let b1 = read_u32(record, 4, big_endian);
            let b2 = read_u32(record, 8, big_endian);
            write!(
                w,
                " | u16 {a0} {a1} {a2} {a3} {a4} {a5} | u32 {b0} {b1} {b2}"
            )?;
        }
        len if len % 2 == 0 => {
            write!(w, " | u16")?;
            for k in 0..len / 2 {
                write!(w, " {}", read_u16(record, 2 * k, big_endian))?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn write_nul_tail<W: Write>(
    w: &mut W,
    tail: &[u8],
    expected: usize,
    label: &str,
    base: usize,
) -> std::io::Result<()> {
    writeln!(
        w,
        "## Tail: {label} ({expected} NUL-terminated string(s), {} bytes)",
        tail.len()
    )?;
    let mut pos = 0usize;
    for k in 0..expected {
        let rest = &tail[pos..];
        let end = rest.iter().position(|&b| b == 0).unwrap_or(rest.len());
        writeln!(
            w,
            "  [{k}] +0x{:04x} len {:3} \"{}\"",
            pos,
            end,
            escape_bytes(&rest[..end])
        )?;
        pos += end + 1;
    }
    if pos < tail.len() {
        writeln!(w, "  trailing {} raw bytes:", tail.len() - pos)?;
        write_hexdump(w, &tail[pos..], base + pos)?;
    }
    Ok(())
}

fn write_anim_brush<W: Write>(
    w: &mut W,
    payload: &[u8],
    count: usize,
    fixed: usize,
    big_endian: bool,
) -> std::io::Result<()> {
    let Some((entries, blob)) = anim_brush_entries(payload, count, big_endian) else {
        writeln!(w, "# could not decode AnimBrush entries")?;
        return write_hexdump(w, payload, 0);
    };

    writeln!(w, "## AnimBrush entries ({count} x 8 bytes)")?;
    writeln!(
        w,
        "# [index] raw bytes | ref, name_offset and flags"
    )?;
    for (j, (reference, offset, flags)) in entries.iter().enumerate() {
        let start = j * 8;
        write!(
            w,
            "  [{j:4}] {} | ref={reference} name_offset=0x{offset:04x} flags=0x{flags:x}",
            hex_bytes(&payload[start..start + 8])
        )?;
        if let Some(name) = string_at_offset(blob, *offset as u32) {
            write!(w, " -> \"{}\"", escape_bytes(name))?;
        } else {
            write!(w, " -> <outside name blob>")?;
        }
        writeln!(w)?;
    }

    writeln!(w, "## Name blob ({fixed} = 0x{fixed:x}, {} bytes)", blob.len())?;
    write_hexdump(w, blob, fixed)?;
    Ok(())
}

fn write_text_list<W: Write>(
    w: &mut W,
    payload: &[u8],
    count: usize,
    big_endian: bool,
) -> std::io::Result<()> {
    let Some((offsets, blob)) = text_list_entries(payload, count, big_endian) else {
        writeln!(w, "# could not decode TextList")?;
        return write_hexdump(w, payload, 0);
    };

    let fixed = count * 4;
    writeln!(
        w,
        "## String offsets ({count} x 4 bytes; relative to the string blob)"
    )?;
    for (k, offset) in offsets.iter().enumerate() {
        write!(
            w,
            "  [{k}] {} | offset=0x{offset:04x}",
            hex_bytes(&payload[k * 4..k * 4 + 4])
        )?;
        match string_at_offset(blob, *offset) {
            Some(s) => write!(w, " -> \"{}\"", escape_bytes(s))?,
            None => write!(w, " -> <outside string blob>")?,
        }
        writeln!(w)?;
    }

    writeln!(
        w,
        "## Shared string blob ({fixed} = 0x{fixed:x}, {} bytes; raw hexdump)",
        blob.len()
    )?;
    write_hexdump(w, blob, fixed)?;
    Ok(())
}

fn write_string_table<W: Write>(
    w: &mut W,
    payload: &[u8],
    count: usize,
    big_endian: bool,
) -> std::io::Result<()> {
    let Some((entries, blob)) = string_table_entries(payload, count, big_endian) else {
        writeln!(w, "# could not decode StringTable")?;
        return write_hexdump(w, payload, 0);
    };

    writeln!(
        w,
        "## StringTable entries ({count} x 10 bytes; id, offset, reserved)"
    )?;
    for (j, (id, offset, reserved)) in entries.iter().enumerate() {
        let start = j * 10;
        write!(
            w,
            "  [{j:3}] {} | id={id} offset=0x{offset:04x} reserved={reserved}",
            hex_bytes(&payload[start..start + 10])
        )?;
        match offset {
            0xffff => write!(w, " -> <sentinel 0xffff: absent>")?,
            0xfffe => write!(w, " -> <sentinel 0xfffe: absent>")?,
            0xfffd => write!(w, " -> <sentinel 0xfffd: absent>")?,
            _ => match string_at_offset(blob, *offset) {
                Some(s) => write!(w, " -> \"{}\"", escape_bytes(s))?,
                None => write!(w, " -> <offset outside blob>")?,
            },
        }
        writeln!(w)?;
    }

    writeln!(
        w,
        "## Shared string blob ({} bytes, raw hexdump)",
        blob.len()
    )?;
    write_hexdump(w, blob, count * 10)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Small formatting helpers
// ---------------------------------------------------------------------------

fn hex_bytes(data: &[u8]) -> String {
    let mut s = String::with_capacity(data.len() * 3);
    for (i, b) in data.iter().enumerate() {
        if i > 0 {
            s.push(' ');
        }
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn write_hexdump<W: Write>(w: &mut W, data: &[u8], base: usize) -> std::io::Result<()> {
    if data.is_empty() {
        return Ok(());
    }
    for (line, chunk) in data.chunks(16).enumerate() {
        write!(w, "  {:08x}  ", base + line * 16)?;
        for i in 0..16 {
            match chunk.get(i) {
                Some(b) => write!(w, "{b:02x} ")?,
                None => write!(w, "   ")?,
            }
        }
        write!(w, " |")?;
        for &b in chunk {
            let c = if (0x20..0x7f).contains(&b) {
                b as char
            } else {
                '.'
            };
            write!(w, "{c}")?;
        }
        for _ in chunk.len()..16 {
            write!(w, " ")?;
        }
        writeln!(w, "|")?;
    }
    Ok(())
}

/// Escape arbitrary bytes for a human-readable one-line string.
fn escape_bytes(data: &[u8]) -> String {
    let mut s = String::with_capacity(data.len());
    for &b in data {
        match b {
            b'\\' => s.push_str("\\\\"),
            b'"' => s.push_str("\\\""),
            0 => s.push_str("\\0"),
            b'\n' => s.push_str("\\n"),
            b'\r' => s.push_str("\\r"),
            b'\t' => s.push_str("\\t"),
            0x20..=0x7e => s.push(b as char),
            _ => s.push_str(&format!("\\x{b:02x}")),
        }
    }
    s
}

/// Escape a Rust string for JSON output.
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
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
