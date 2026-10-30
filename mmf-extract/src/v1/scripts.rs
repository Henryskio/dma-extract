//! The first game's `Script` containers — layout measured, objects not yet.
//!
//! Version-1 Scripts containers were the one container kind this extractor
//! could not open at all. The reason was structural, not semantic: the shared
//! parser looked for the versions-2/3 shape — a 32-byte names table and then a
//! serialized object database that must end exactly at `offsets[0]` — and the
//! first game has **neither**. `container.rs` now knows that, and this module
//! holds everything that is measured about the region it finds instead.
//!
//! ## The container
//!
//! ```text
//! 0x00  16 bytes  "MMFW " + "Script", NUL padded
//! 0x10   2 bytes  endian marker ("MM")
//! 0x12   2 bytes  u16 version, 1
//! 0x14   6 bytes  file id
//! 0x1A   8 bytes  timestamp (f64); its low u16 is the resource count at 0x1E
//! 0x1E   2 bytes  u16 resource count N
//! 0x20  4*(N+1)  u32 offsets; resource i spans [off[i], off[i+1])
//!         2 bytes u16, measured as **4** in every shipped container
//!        …        payloads, concatenated, starting at offsets[0]
//! ```
//!
//! There is **no names table** and **no metadata region**. Two independent
//! checks pin this down on both shipped containers: the first payload starts
//! exactly at `0x20 + 4*(N+1) + 2`, and the last offset equals the file
//! length — `HD.mms` 1218 / 75889 and `pack.mms` 3990 / 574861. The `u16` is
//! called a *version* rather than padding because it is a constant 4 across
//! both containers, and `AGENTS.md` makes an unmeasured guess a failure, so a
//! container that reads anything else is rejected rather than assumed to be
//! padding.
//!
//! ## What is measured about a payload
//!
//! Each payload starts with a `u32` that reads as an element count (its
//! distribution is strongly peaked — `pack.mms` has 211 payloads whose lead is
//! 14 and they are all exactly 152 bytes), and ends with a NUL-terminated
//! ASCII string that is the **object name**: `char606`, `char1a`, `rdy53`,
//! `sky333`. That is 127 of 295 `HD.mms` payloads and 740 of 988 `pack.mms`
//! payloads; the rest carry no trailing printable run, which is consistent
//! with the games-2/3 behaviour where "some classes append string data after
//! that fixed prefix" (`docs/SCRIPTS.md`) — i.e. some classes append none.
//!
//! **Open:** the fixed-size record between the count and the name. Reading it
//! as `count` records of one size fails almost everywhere (only 120 of 295
//! `HD.mms` payloads fit a 10-byte record), because the record length varies
//! by class, exactly as it does via `type_aux` in the shared format. The
//! per-class record size is the next thing to measure, and it is what stands
//! between this and a structured extraction.
//!
//! `--original` is fully measured and byte-exact: a payload is never compressed
//! here, so the stored bytes are the payload and the reverse test in
//! `tests/game1.rs` asserts exactly that.

use crate::container::{Container, ContainerKind};
use anyhow::{bail, Context, Result};
use std::path::Path;

/// The `u16` between the offsets table and the first payload.
///
/// Measured as 4 in both shipped containers (`HD.mms`, `pack.mms`). A container
/// that reads otherwise is rejected: this field is too little observed to be
/// interpreted, and treating it as padding would be a guess.
pub const V1_SCRIPT_VERSION: u16 = 4;

/// Everything measured about a first-game script container.
#[derive(Debug, Default)]
pub struct V1Scripts {
    /// The `u16` after the offsets table; [`V1_SCRIPT_VERSION`] in every
    /// shipped container.
    pub version: u16,
    /// Just past the offsets table and that `u16`.
    pub header_end: usize,
    /// `offsets[0]`: where the payloads start.
    pub payload_start: usize,
    /// Per-object names, resource order, read from each payload's trailing
    /// string. Empty for an object whose payload carries none — see the module
    /// comment on why that is an inference and not a measurement.
    pub names: Vec<String>,
}

/// Parse the version-1 script layout of `container`.
///
/// `table_end` is `0x20 + 4 * (N + 1)`, the offset just past the offsets table.
pub fn parse(
    path: &Path,
    data: &[u8],
    container: &Container,
    table_end: usize,
) -> Result<V1Scripts> {
    if container.kind != ContainerKind::Scripts {
        bail!("{}: not a Script container", path.display());
    }
    let n = container.resource_count as usize;
    let payload_start = container.offsets.first().copied().unwrap_or(0) as usize;

    let version = crate::container::read_u16(data, table_end, container.big_endian);
    if version != V1_SCRIPT_VERSION {
        bail!(
            "{}: version-1 script marker at {table_end:#x} is {version}, expected {V1_SCRIPT_VERSION}",
            path.display()
        );
    }

    // The payloads must start exactly two bytes after the marker.
    if payload_start != table_end + 2 {
        bail!(
            "{}: version-1 script payloads start at {payload_start} but the marker ends at {}",
            path.display(),
            table_end + 2
        );
    }

    let mut names = Vec::with_capacity(n);
    let mut named = 0usize;
    for index in 0..n {
        let payload = container.payload(index);
        let name = trailing_name(payload);
        if !name.is_empty() {
            named += 1;
        }
        names.push(name);
    }

    eprintln!(
        "note: {}: {} of {} script objects carry a name in their payload's string tail",
        path.display(),
        named,
        n
    );

    Ok(V1Scripts {
        version,
        header_end: table_end + 2,
        payload_start,
        names,
    })
}

/// The trailing printable run of a payload, or the empty string.
///
/// A first-game script payload ends with the object's name as a NUL-terminated
/// ASCII string — `char606`, `char1a`, `sky333`. Taking the *last* printable
/// run of at least three bytes is the measured shape; a payload with no such
/// run carries no name at all, which the shared format also allows (`docs/
/// SCRIPTS.md`: only some classes append a string tail).
fn trailing_name(payload: &[u8]) -> String {
    let mut end = payload.len();
    while end > 0 && payload[end - 1] == 0 {
        end -= 1;
    }
    let mut start = end;
    while start > 0 && is_printable(payload[start - 1]) {
        start -= 1;
    }
    if end - start < 3 {
        return String::new();
    }
    String::from_utf8_lossy(&payload[start..end]).into_owned()
}

fn is_printable(byte: u8) -> bool {
    byte.is_ascii_graphic() || byte == b' '
}

/// Extract every object of a first-game script container.
///
/// `--original` writes the payload byte for byte, which is fully measured: a
/// first-game script payload is never compressed, so the stored bytes *are*
/// the payload, and `tests/game1.rs` asserts the equality against the
/// container's own bytes.
///
/// The modern mode writes the payloads and a measurement manifest, but not a
/// structured dump: the per-class record size between each payload's count and
/// its name is still unmeasured, and emitting a decoded object from a guess
/// would be exactly what `AGENTS.md` forbids.
pub fn extract_container(container: &Container, out_dir: &Path, original: bool) -> Result<usize> {
    if container.kind != ContainerKind::Scripts {
        bail!(
            "{}: expected a Scripts container, found {:?}",
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

    std::fs::create_dir_all(out_dir).with_context(|| format!("creating {}", out_dir.display()))?;
    let scripts = parse(
        &container.path,
        &container.data,
        container,
        0x20 + 4 * (container.resource_count as usize + 1),
    )?;

    if original {
        write_originals(container, &scripts, out_dir)
    } else {
        write_measurements(container, &scripts, out_dir)
    }
}

/// Write every payload byte-for-byte, named by index and object name.
fn write_originals(container: &Container, scripts: &V1Scripts, out_dir: &Path) -> Result<usize> {
    let mut written = 0usize;
    let mut failed = 0usize;
    for index in 0..container.resource_count as usize {
        let name = safe_name(&scripts.names[index], index);
        let file = out_dir.join(format!("{index:05}_{name}.bin"));
        match std::fs::write(&file, container.payload(index)) {
            Ok(()) => written += 1,
            Err(e) => {
                failed += 1;
                eprintln!(
                    "warning: could not write {} (object {index}): {e}",
                    file.display()
                );
            }
        }
    }
    if failed > 0 {
        eprintln!(
            "warning: {} of {} script objects could not be written",
            failed,
            container.resource_count
        );
    }
    Ok(written)
}

/// Write the payloads and a manifest of what is measured.
fn write_measurements(
    container: &Container,
    scripts: &V1Scripts,
    out_dir: &Path,
) -> Result<usize> {
    let objects = out_dir.join("objects");
    std::fs::create_dir_all(&objects)
        .with_context(|| format!("creating {}", objects.display()))?;

    let n = container.resource_count as usize;
    let mut manifest = String::new();
    manifest.push_str("# First-game script container: what is measured\n\n");
    manifest.push_str(&format!("container: {}\n", container.path.display()));
    manifest.push_str(&format!("version: {}\n", scripts.version));
    manifest.push_str(&format!("objects: {n}\n"));
    manifest.push_str(&format!(
        "payloads start at: {} (offsets table size {} + the version u16)\n",
        scripts.payload_start,
        scripts.payload_start - 2
    ));

    let mut counts = std::collections::BTreeMap::new();
    let mut named = 0usize;
    for index in 0..n {
        let payload = container.payload(index);
        let count = if payload.len() >= 4 {
            u32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]])
        } else {
            0
        };
        *counts.entry(count).or_insert(0usize) += 1;
        if !scripts.names[index].is_empty() {
            named += 1;
        }

        let name = safe_name(&scripts.names[index], index);
        let file = objects.join(format!("{index:05}_{name}.txt"));
        let mut body = format!(
            "object {index}\npayload bytes: {}\nleading u32 (count): {count}\nname: {}\n",
            payload.len(),
            if scripts.names[index].is_empty() { "(none in payload)" } else { &scripts.names[index] }
        );
        if payload.len() >= 4 {
            body.push_str(&format!(
                "bytes after the count: {} (the per-class record length is unmeasured)\n",
                payload.len() - 4
            ));
        }
        std::fs::write(&file, body)
            .with_context(|| format!("writing {}", file.display()))?;
    }

    manifest.push_str(&format!("objects named in their payload: {named} of {n}\n\n"));
    manifest.push_str("distribution of the leading u32 (count) across objects:\n");
    for (count, how_many) in &counts {
        manifest.push_str(&format!("  count {count:>6}: {how_many:>4} objects\n"));
    }
    manifest.push_str(
        "\nNOT DECODED: the fixed-size record between each payload's leading\n\
         count and its trailing name. Its length varies per object, as the\n\
         games-2/3 `type_aux` table does, and the per-class sizes are still\n\
         unmeasured. Use --original for the byte-exact payloads.\n",
    );

    std::fs::write(out_dir.join("manifest.txt"), manifest)
        .with_context(|| format!("writing {}", out_dir.join("manifest.txt").display()))?;
    Ok(n)
}

/// A filename-safe object name, falling back to the index.
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
    if cleaned.is_empty() {
        format!("object{index}")
    } else {
        cleaned
    }
}
