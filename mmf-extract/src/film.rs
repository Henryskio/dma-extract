//! Films (MMFW `"Films"`) resources: an inner `.flm` chunk stream of
//! palette-indexed frames, timing chunks and audio blocks.
//!
//! A Films container stores the film-specific metadata in place of the
//! picture metadata table.  The resource payload itself is the complete
//! `.flm` chunk stream and is **not** LZ77-compressed at the container
//! level; the container's `sizes` table does not describe it.  (In
//! `DMA.MMF` it reads 26,870,306 even though the payload is 6,856,628
//! bytes.)  `--original` therefore writes the payload verbatim.
//!
//! The inner stream starts with a `0xF789` chunk tag.  The chunk types
//! understood here are:
//!
//! | type | meaning |
//! |---|---|
//! | `0x02` | audio block: 12-byte header + PCM + padding |
//! | `0x03` | marker (4 zero bytes in this game) |
//! | `0x06` | frame timestamp, one big-endian `u32` |
//! | `0x07` | end of stream |
//! | `0x0A` | full 8bpp frame, LZ77-compressed |
//! | `0x0B` | delta frame: 20 rects + LZ77 + cursor-relative RLE |
//!
//! See `/workspace/docs/FILMS.md` for the byte-level notes.

use crate::container::{Container, ContainerKind, read_u16, read_u32};
use crate::lz77;
use crate::picture;
use anyhow::{Context, Result, bail};
use std::collections::{BTreeMap, HashSet};
use std::fmt::Write as _;
use std::path::Path;

/// Tag at the start of every inner chunk.
pub const CHUNK_TAG: u16 = 0xF789;

pub const TYPE_AUDIO: u16 = 0x02;
pub const TYPE_MARKER: u16 = 0x03;
pub const TYPE_TIMESTAMP: u16 = 0x06;
pub const TYPE_END: u16 = 0x07;

/// Key frame chunk. The first game writes type `0x0A`, the sequel `0x00`.
pub const TYPE_FULL_FRAME: u16 = 0x0A;
pub const TYPE_FULL_FRAME_V2: u16 = 0x00;
/// Delta chunk. The first game writes type `0x0B`, the sequel `0x01`.
pub const TYPE_DELTA: u16 = 0x0B;
pub const TYPE_DELTA_V2: u16 = 0x01;

/// Whether this container's films use the sequel's chunk numbering.
fn is_film_v2(container: &Container) -> bool {
    container.version == 2
}

/// Chunk number for the key frame of this container.
fn full_frame_kind(container: &Container) -> u16 {
    if is_film_v2(container) {
        TYPE_FULL_FRAME_V2
    } else {
        TYPE_FULL_FRAME
    }
}

/// Chunk number for a delta frame of this container.
fn delta_kind(container: &Container) -> u16 {
    if is_film_v2(container) {
        TYPE_DELTA_V2
    } else {
        TYPE_DELTA
    }
}

/// Longest per-film record seen in a Films container.
///
/// The first game's record is 26 bytes; the sequel's is 22.  The parser
/// solves for the actual length from the file layout, so this is only a
/// buffer bound.
pub const MAX_RECORD_LEN: usize = 26;

/// One chunk of the inner `.flm` stream, with offsets relative to the
/// start of that stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chunk {
    pub offset: usize,
    pub kind: u16,
    pub len: usize,
}

/// One audio block of the inner stream.  The PCM bytes are not decoded;
/// `--original` preserves them exactly.
#[derive(Debug, Clone)]
pub struct AudioChunk {
    pub chunk_offset: usize,
    pub chunk_len: usize,
    /// Value of the block's first `u32` (the PCM byte count).
    pub data_len: u32,
    /// Value of the block's third `u32`; `0xFFFFFFFF` marks a sync block.
    pub index: u32,
    pub data_offset: usize,
    pub sync: bool,
}

/// One decoded frame in timeline order.
#[derive(Debug, Clone)]
pub struct Frame {
    /// Timestamp of this frame.  The unit is a game tick; the shipped film
    /// uses 83-tick spacing.
    pub start_tick: u32,
    /// Time until the next frame (the last frame reuses the previous gap).
    pub duration_tick: u32,
    /// True when the timestamp had no following delta chunk, so this frame
    /// is the previous frame repeated.
    pub duplicate: bool,
    /// Raw 8bpp palette indices, `stride * height` bytes.
    pub indices: Vec<u8>,
    pub timestamp_chunk: Option<usize>,
    pub delta_chunk: Option<usize>,
    pub full_chunk: Option<usize>,
    /// Rectangles from the delta header, stored as `[left, top, right,
    /// bottom]`.  They are advisory: the RLE stream is authoritative.
    pub delta_rects: Vec<[u16; 4]>,
}

/// A fully decoded film.
#[derive(Debug, Clone)]
pub struct Film {
    pub index: usize,
    pub name: String,
    pub width: u16,
    pub height: u16,
    pub stride: usize,
    /// `[top, left, bottom, right]` from the per-film record.
    pub bounds: [u16; 4],
    pub record: Vec<u8>,
    pub palette: Vec<u8>,
    pub palette_count: u16,
    pub frames: Vec<Frame>,
    pub chunks: Vec<Chunk>,
    pub chunk_kinds: BTreeMap<u16, usize>,
    pub audio: Vec<AudioChunk>,
    pub unique_frames: usize,
}

impl Film {
    /// Total timeline length, in ticks.
    pub fn duration_ticks(&self) -> u64 {
        self.frames.iter().map(|f| f.duration_tick as u64).sum()
    }
}

/// Extract every film in a container.
///
/// With `original`, write each resource's inner `.flm` payload verbatim.
/// Otherwise decode the frames and write per-film directories containing
/// `frame_NNN.png` files plus a `manifest.json`.
///
/// Returns the number of films handled (not the number of frames).
pub fn extract_container(container: &Container, out_dir: &Path, original: bool) -> Result<usize> {
    if container.kind != ContainerKind::Films {
        bail!(
            "{}: expected a Films container, found {:?}",
            container.path.display(),
            container.kind
        );
    }
    std::fs::create_dir_all(out_dir).with_context(|| format!("creating {}", out_dir.display()))?;

    let mut written = 0usize;

    if original {
        // The payload is already the game's own `.flm` file.  Do not feed it
        // to the LZ77 decoder: the container's sizes table is not a
        // decompressed-size table for Films.
        for index in 0..container.resource_count as usize {
            let safe = crate::cli::sanitize(container.name(index), index);
            let file = out_dir.join(original_filename(index, &safe));
            std::fs::write(&file, container.payload(index))
                .with_context(|| format!("writing {}", file.display()))?;
            written += 1;
        }
        return Ok(written);
    }

    for index in 0..container.resource_count as usize {
        let film = decode_film(container, index)?;
        let safe = crate::cli::sanitize(&film.name, index);
        let dir = out_dir.join(format!("{index:05}_{}", strip_flm(&safe)));
        std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
        write_film_modern(container, &film, &dir)?;
        written += 1;
    }

    Ok(written)
}

/// Decode one film resource into its frame timeline and audio map.
pub fn decode_film(container: &Container, index: usize) -> Result<Film> {
    if container.kind != ContainerKind::Films {
        bail!(
            "{}: expected a Films container, found {:?}",
            container.path.display(),
            container.kind
        );
    }
    if index >= container.resource_count as usize {
        bail!(
            "{}: film index {index} out of range (resource count {})",
            container.path.display(),
            container.resource_count
        );
    }

    let name = container.name(index).to_string();
    let meta = parse_film_metadata(container, index)?;
    let data = container.payload(index);

    let chunks = parse_chunks(data)
        .with_context(|| format!("parsing film {index} ('{name}') chunk stream"))?;
    let mut chunk_kinds = BTreeMap::new();
    for chunk in &chunks {
        *chunk_kinds.entry(chunk.kind).or_default() += 1;
    }

    let full_chunks: Vec<&Chunk> = chunks
        .iter()
        .filter(|c| c.kind == full_frame_kind(container))
        .collect();
    if full_chunks.len() != 1 {
        bail!(
            "film {index} ('{name}'): expected exactly one full-frame chunk (type {:#06x}), found {}",
            full_frame_kind(container),
            full_chunks.len()
        );
    }
    let mut buffer = decode_full_frame(data, full_chunks[0], is_film_v2(container))
        .with_context(|| format!("film {index} ('{name}'): decoding full frame"))?;

    let frame_size = meta
        .stride
        .checked_mul(meta.height as usize)
        .ok_or_else(|| anyhow::anyhow!("film {index}: frame size overflow"))?;
    if buffer.len() != frame_size {
        bail!(
            "film {index} ('{name}'): full frame is {} bytes, expected {}x{} (stride {}) = {}",
            buffer.len(),
            meta.width,
            meta.height,
            meta.stride,
            frame_size
        );
    }

    // Build the timeline before decoding so malformed chunk order is
    // reported clearly.
    struct Event<'a> {
        tick: u32,
        timestamp: &'a Chunk,
        delta: Option<&'a Chunk>,
    }
    let mut events: Vec<Event> = Vec::new();
    for chunk in &chunks {
        match chunk.kind {
            TYPE_TIMESTAMP => {
                let payload = chunk_payload(data, chunk)?;
                if payload.len() < 4 {
                    bail!(
                        "film {index} ('{name}'): timestamp chunk at {:#x} is {} bytes",
                        chunk.offset,
                        payload.len()
                    );
                }
                let tick = read_u32(payload, 0, true);
                events.push(Event {
                    tick,
                    timestamp: chunk,
                    delta: None,
                });
            }
            TYPE_DELTA | TYPE_DELTA_V2 => {
                let event = events.last_mut().ok_or_else(|| {
                    anyhow::anyhow!(
                        "film {index} ('{name}'): delta chunk at {:#x} has no preceding timestamp",
                        chunk.offset
                    )
                })?;
                if event.delta.is_some() {
                    bail!(
                        "film {index} ('{name}'): more than one delta chunk follows timestamp at {:#x}",
                        event.timestamp.offset
                    );
                }
                event.delta = Some(chunk);
            }
            _ => {}
        }
    }
    for pair in events.windows(2) {
        if pair[1].tick < pair[0].tick {
            bail!(
                "film {index} ('{name}'): timestamps go backwards ({} then {})",
                pair[0].tick,
                pair[1].tick
            );
        }
    }

    let mut frames = Vec::with_capacity(events.len() + 1);

    // Frame zero is the full frame at time zero; the first timestamp is the
    // moment the next frame becomes visible.
    let frame0_duration = events.first().map(|e| e.tick).unwrap_or(0);
    frames.push(Frame {
        start_tick: 0,
        duration_tick: frame0_duration,
        duplicate: false,
        indices: buffer.clone(),
        timestamp_chunk: None,
        delta_chunk: None,
        full_chunk: Some(full_chunks[0].offset),
        delta_rects: Vec::new(),
    });

    for (i, event) in events.iter().enumerate() {
        let duration_tick = if i + 1 < events.len() {
            events[i + 1].tick - event.tick
        } else if i > 0 {
            event.tick - events[i - 1].tick
        } else {
            frame0_duration
        };

        let mut delta_rects = Vec::new();
        let mut wrote = true;
        if let Some(delta) = event.delta {
            match decode_delta(data, delta, &mut buffer, is_film_v2(container)) {
                Ok(rects) => delta_rects = rects,
                Err(e) => {
                    // The sequel's delta grammar is not fully recovered: a few
                    // chunks drive the cursor negative. Keep the previous
                    // frame instead of losing the whole film.
                    if !is_film_v2(container) {
                        return Err(e).with_context(|| {
                            format!(
                                "film {index} ('{name}'): decoding delta chunk at {:#x}",
                                delta.offset
                            )
                        });
                    }
                    eprintln!(
                        "warning: {}: film {index} ('{name}'): delta chunk at {:#x} not decoded ({e}); reusing the previous frame",
                        container.path.display(),
                        delta.offset
                    );
                    wrote = false;
                }
            }
        }

        frames.push(Frame {
            start_tick: event.tick,
            duration_tick,
            duplicate: event.delta.is_none() || !wrote,
            indices: buffer.clone(),
            timestamp_chunk: Some(event.timestamp.offset),
            delta_chunk: event.delta.map(|c| c.offset),
            full_chunk: None,
            delta_rects,
        });
    }

    let mut seen: HashSet<&[u8]> = HashSet::new();
    let mut unique_frames = 0usize;
    for frame in &frames {
        if seen.insert(frame.indices.as_slice()) {
            unique_frames += 1;
        }
    }

    let mut audio = Vec::new();
    for chunk in &chunks {
        if chunk.kind != TYPE_AUDIO {
            continue;
        }
        let payload = chunk_payload(data, chunk)?;
        if payload.len() < 12 {
            bail!(
                "film {index} ('{name}'): audio chunk at {:#x} is {} bytes",
                chunk.offset,
                payload.len()
            );
        }
        let data_len = read_u32(payload, 0, true);
        let block_index = read_u32(payload, 8, true);
        if 12usize.saturating_add(data_len as usize) > payload.len() {
            bail!(
                "film {index} ('{name}'): audio chunk at {:#x} declares {} data bytes but holds {}",
                chunk.offset,
                data_len,
                payload.len()
            );
        }
        audio.push(AudioChunk {
            chunk_offset: chunk.offset,
            chunk_len: chunk.len,
            data_len,
            index: block_index,
            data_offset: chunk.offset + 8 + 12,
            sync: block_index == u32::MAX,
        });
    }

    Ok(Film {
        index,
        name,
        width: meta.width,
        height: meta.height,
        stride: meta.stride,
        bounds: meta.bounds,
        record: meta.record,
        palette: meta.palette,
        palette_count: meta.palette_count,
        frames,
        chunks,
        chunk_kinds,
        audio,
        unique_frames,
    })
}

struct FilmMeta {
    record: Vec<u8>,
    bounds: [u16; 4],
    width: u16,
    height: u16,
    stride: usize,
    palette: Vec<u8>,
    palette_count: u16,
}

/// Parse the Films metadata region for one resource.
///
/// After the common names table comes the generic `4*N` u32 array, then the
/// film block: `u16` marker (7 in the first game, 5 in the sequel), `N`
/// per-film records, `u16` palette count and that many 1024-byte palettes.
/// The payload starts immediately after the last palette; that equality is
/// what pins the record length down, so the length is solved for rather than
/// assumed.
fn parse_film_metadata(container: &Container, index: usize) -> Result<FilmMeta> {
    let n = container.resource_count as usize;
    if index >= n {
        bail!("film index {index} out of range");
    }
    let data = &container.data;
    let block = container
        .names_end
        .checked_add(4 * n)
        .ok_or_else(|| anyhow::anyhow!("film metadata offset overflow"))?;
    let marker = read_u16_checked(data, block).with_context(|| {
        format!(
            "{}: film metadata marker at {:#x}",
            container.path.display(),
            block
        )
    })?;
    if marker != 7 && marker != 5 {
        bail!(
            "{}: film metadata marker at {:#x} is {marker}, expected 7 or 5",
            container.path.display(),
            block
        );
    }

    // Solve: record_len, palette_count such that
    //   block + 2 + n*record_len + 2 + 1024*palette_count == payload_start
    let payload_start = container.offsets[0] as usize;
    let mut found: Option<(usize, usize)> = None; // (record_len, palette_count)
    for record_len in (16..=MAX_RECORD_LEN).step_by(2) {
        let count_at = block
            .checked_add(2)
            .and_then(|v| v.checked_add(record_len * n))
            .ok_or_else(|| anyhow::anyhow!("film record table overflow"))?;
        let palette_count = match read_u16_checked(data, count_at) {
            Ok(v) => v as usize,
            Err(_) => continue,
        };
        if palette_count == 0 || palette_count > 8 {
            continue;
        }
        let palettes_end = match count_at
            .checked_add(2)
            .and_then(|v| v.checked_add(1024 * palette_count))
        {
            Some(v) => v,
            None => continue,
        };
        if palettes_end == payload_start {
            found = Some((record_len, palette_count));
            break;
        }
    }
    let (record_len, palette_count) = found.ok_or_else(|| {
        anyhow::anyhow!(
            "{}: no film record length/palette count ends exactly at the payload start {:#x}",
            container.path.display(),
            payload_start
        )
    })?;

    let records_start = block + 2;
    let palette_count_at = records_start + record_len * n;
    let palettes_start = palette_count_at + 2;
    if palettes_start + 1024 * palette_count > data.len() {
        bail!(
            "{}: film metadata ends at {:#x}, past the {:#x}-byte file",
            container.path.display(),
            palettes_start + 1024 * palette_count,
            data.len()
        );
    }

    let record_start = records_start + record_len * index;
    let record = data
        .get(record_start..record_start + record_len)
        .ok_or_else(|| anyhow::anyhow!("film record {index} is outside the file"))?
        .to_vec();

    // Bytes +2..+10 are four big-endian u16 values in the order
    // {top, left, bottom, right}.  The shipped film is (0, 0, 410, 546).
    let bounds = [
        u16::from_be_bytes([record[2], record[3]]),
        u16::from_be_bytes([record[4], record[5]]),
        u16::from_be_bytes([record[6], record[7]]),
        u16::from_be_bytes([record[8], record[9]]),
    ];
    let (top, left, bottom, right) = (bounds[0], bounds[1], bounds[2], bounds[3]);
    if right <= left || bottom <= top {
        bail!("film {index}: bad bounds top={top} left={left} bottom={bottom} right={right}");
    }
    let width = right - left;
    let height = bottom - top;
    // The first game pads 8bpp rows to a multiple of four bytes; the sequel
    // stores rows unpadded (its key frame is exactly width * height bytes).
    let stride = if container.version == 2 {
        width as usize
    } else {
        picture::stride8(width)
    };

    let palette = data[palettes_start..palettes_start + 1024].to_vec();

    Ok(FilmMeta {
        record,
        bounds,
        width,
        height,
        stride,
        palette,
        palette_count: palette_count as u16,
    })
}

fn parse_chunks(data: &[u8]) -> Result<Vec<Chunk>> {
    let mut chunks = Vec::new();
    let mut offset = 0usize;
    while offset + 8 <= data.len() {
        let tag = read_u16(data, offset, true);
        if tag != CHUNK_TAG {
            bail!("bad chunk tag {tag:#06x} at {offset:#x}, expected {CHUNK_TAG:#06x}");
        }
        let kind = read_u16(data, offset + 2, true);
        let len = read_u32(data, offset + 4, true) as usize;
        let end = offset
            .checked_add(8)
            .and_then(|v| v.checked_add(len))
            .ok_or_else(|| anyhow::anyhow!("chunk at {offset:#x} length overflow"))?;
        if end > data.len() {
            bail!("chunk type {kind:#06x} at {offset:#x} length {len} runs past the payload end");
        }
        chunks.push(Chunk { offset, kind, len });
        offset = end;
    }
    if offset != data.len() {
        bail!(
            "{} trailing bytes at {offset:#x} after the last chunk",
            data.len() - offset
        );
    }
    Ok(chunks)
}

fn chunk_payload<'a>(data: &'a [u8], chunk: &Chunk) -> Result<&'a [u8]> {
    let start = chunk
        .offset
        .checked_add(8)
        .ok_or_else(|| anyhow::anyhow!("chunk offset overflow"))?;
    let end = start
        .checked_add(chunk.len)
        .ok_or_else(|| anyhow::anyhow!("chunk length overflow"))?;
    data.get(start..end)
        .ok_or_else(|| anyhow::anyhow!("chunk at {:#x} is outside the payload", chunk.offset))
}

fn decode_full_frame(data: &[u8], chunk: &Chunk, v2: bool) -> Result<Vec<u8>> {
    let payload = chunk_payload(data, chunk)?;
    if v2 {
        // The sequel stores the key frame as raw palette indices with no
        // header and no LZ77: exactly width * height bytes.
        if payload.len() != 800 * 600 {
            bail!(
                "full-frame chunk at {:#x} is {} bytes, expected {} for an 800x600 frame",
                chunk.offset,
                payload.len(),
                800 * 600
            );
        }
        return Ok(payload.to_vec());
    }
    if payload.len() < 8 {
        bail!("full-frame chunk at {:#x} is too short", chunk.offset);
    }
    let lz77_len = read_u32(payload, 0, true) as usize;
    let decoded_len = read_u32(payload, 4, true) as usize;
    if lz77_len > payload.len() - 8 {
        bail!(
            "full-frame chunk at {:#x} declares {lz77_len} compressed bytes but only {} follow the header",
            chunk.offset,
            payload.len() - 8
        );
    }
    let decoded = lz77::decompress(&payload[8..8 + lz77_len]).with_context(|| {
        format!(
            "full-frame chunk at {:#x}: LZ77 stream ({} bytes)",
            chunk.offset, lz77_len
        )
    })?;
    if decoded.len() != decoded_len {
        bail!(
            "full-frame chunk at {:#x}: decoded {} bytes, header says {}",
            chunk.offset,
            decoded.len(),
            decoded_len
        );
    }
    Ok(decoded)
}

/// Decode one delta chunk into `frame`, returning the header's rectangles.
fn decode_delta(
    data: &[u8],
    chunk: &Chunk,
    frame: &mut [u8],
    v2: bool,
) -> Result<Vec<[u16; 4]>> {
    let payload = chunk_payload(data, chunk)?;
    if payload.len() < 168 {
        bail!("delta chunk at {:#x} is too short", chunk.offset);
    }

    // Layout, first game: [u32 lz77_len][u32 decoded_len][168 bytes of
    // rects][LZ77 stream] whose output is the RLE data.
    //
    // Layout, sequel: [168 bytes of rects][RLE stream] with no LZ77 wrapper.
    let (rect_off, delta) = if v2 {
        (0usize, payload[168..].to_vec())
    } else {
        let lz77_len = read_u32(payload, 0, true) as usize;
        let decoded_len = read_u32(payload, 4, true) as usize;
        if lz77_len > payload.len() - 168 {
            bail!(
                "delta chunk at {:#x} declares {lz77_len} compressed bytes but only {} follow the rect table",
                chunk.offset,
                payload.len() - 168
            );
        }
        let delta = lz77::decompress(&payload[168..168 + lz77_len]).with_context(|| {
            format!(
                "delta chunk at {:#x}: LZ77 stream ({} bytes)",
                chunk.offset, lz77_len
            )
        })?;
        if delta.len() != decoded_len {
            bail!(
                "delta chunk at {:#x}: decoded {} bytes, header says {}",
                chunk.offset,
                delta.len(),
                decoded_len
            );
        }
        (8, delta)
    };

    // Each rect is four u16: top, left, bottom, right.  Store the more
    // conventional left/top/right/bottom order.
    let mut rects = Vec::with_capacity(20);
    for i in 0..20 {
        let p = rect_off + i * 8;
        let top = read_u16(payload, p, true);
        let left = read_u16(payload, p + 2, true);
        let bottom = read_u16(payload, p + 4, true);
        let right = read_u16(payload, p + 6, true);
        rects.push([left, top, right, bottom]);
    }

    apply_delta(frame, &delta).with_context(|| {
        format!(
            "delta chunk at {:#x}: cursor-relative RLE ({} bytes)",
            chunk.offset,
            delta.len()
        )
    })?;
    Ok(rects)
}

/// Apply the cursor-relative run-length delta to an 8bpp frame.
///
/// Each record is two big-endian `i16`s: a signed cursor displacement and a
/// signed length.  A zero length only moves the cursor, a positive length
/// copies literals and a negative length repeats a four-byte pattern.  The
/// pattern bytes are byte-reversed before repetition (the game's
/// `FUN_004c396b` swaps the 32-bit word).
fn apply_delta(frame: &mut [u8], delta: &[u8]) -> Result<()> {
    let mut pos = 0usize;
    let mut cursor: i64 = 0;

    while pos + 4 <= delta.len() {
        let displacement = i16::from_be_bytes([delta[pos], delta[pos + 1]]) as i64;
        let length = i16::from_be_bytes([delta[pos + 2], delta[pos + 3]]) as i64;
        pos += 4;
        cursor += displacement;

        if length == 0 {
            continue;
        }

        if length < 0 {
            let count = (-length) as usize;
            if pos + 4 > delta.len() {
                bail!("pattern record at {pos:#x} has no four-byte pattern");
            }
            let pattern = [delta[pos + 3], delta[pos + 2], delta[pos + 1], delta[pos]];
            pos += 4;
            let start = checked_span(cursor, count, frame.len())?;
            for (i, byte) in frame[start..start + count].iter_mut().enumerate() {
                *byte = pattern[i & 3];
            }
            cursor += count as i64;
        } else {
            let count = length as usize;
            if pos + count > delta.len() {
                bail!("literal record at {pos:#x} runs past the delta data");
            }
            let start = checked_span(cursor, count, frame.len())?;
            frame[start..start + count].copy_from_slice(&delta[pos..pos + count]);
            pos += count;
            cursor += count as i64;
        }
    }

    if pos != delta.len() {
        bail!("{} trailing delta bytes at {pos:#x}", delta.len() - pos);
    }
    Ok(())
}

fn checked_span(cursor: i64, count: usize, frame_len: usize) -> Result<usize> {
    if cursor < 0 {
        bail!("write at negative frame offset {cursor}");
    }
    let start = cursor as usize;
    let end = start
        .checked_add(count)
        .ok_or_else(|| anyhow::anyhow!("frame write range overflow"))?;
    if end > frame_len {
        bail!("write {start}..{end} is past the {frame_len}-byte frame");
    }
    Ok(start)
}

fn write_film_modern(container: &Container, film: &Film, dir: &Path) -> Result<()> {
    for (i, frame) in film.frames.iter().enumerate() {
        let rgba = picture::indexed_rgba_stride(
            &film.palette,
            &frame.indices,
            film.width,
            film.height,
            film.stride,
        );
        let path = dir.join(format!("frame_{i:03}.png"));
        picture::write_png(&path, film.width, film.height, &rgba)
            .with_context(|| format!("writing {}", path.display()))?;
    }

    let path = dir.join("manifest.json");
    let manifest = manifest_json(container, film);
    std::fs::write(&path, manifest).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

fn manifest_json(container: &Container, film: &Film) -> String {
    let mut s = String::with_capacity(256 * 1024);
    s.push_str("{\n");
    s.push_str(&format!(
        "  \"container\": \"{}\",\n",
        json_escape(&container.path.display().to_string())
    ));
    s.push_str(&format!("  \"film_index\": {},\n", film.index));
    s.push_str(&format!("  \"name\": \"{}\",\n", json_escape(&film.name)));
    s.push_str(&format!("  \"width\": {},\n", film.width));
    s.push_str(&format!("  \"height\": {},\n", film.height));
    s.push_str(&format!("  \"stride\": {},\n", film.stride));
    s.push_str(&format!(
        "  \"frame_size_bytes\": {},\n",
        film.stride * film.height as usize
    ));
    s.push_str(
        "  \"pixel_format\": \"8bpp palette indices; RGBA = palette[transparency, red, green, blue], alpha = 255 - transparency\",\n",
    );
    s.push_str(&format!("  \"record_hex\": \"{}\",\n", hex(&film.record)));
    s.push_str(&format!(
        "  \"record_bounds\": [{}, {}, {}, {}],\n",
        film.bounds[0], film.bounds[1], film.bounds[2], film.bounds[3]
    ));
    s.push_str(
        "  \"record_note\": \"bytes +2..+10 are u16 BE {top, left, bottom, right}; the rest is preserved verbatim\",\n",
    );
    s.push_str(&format!("  \"palette_count\": {},\n", film.palette_count));
    s.push_str(&format!("  \"palette_hex\": \"{}\",\n", hex(&film.palette)));
    s.push_str(&format!("  \"frame_count\": {},\n", film.frames.len()));
    s.push_str(&format!(
        "  \"unique_frame_count\": {},\n",
        film.unique_frames
    ));
    s.push_str(
        "  \"timebase\": \"ticks treated as milliseconds (83-tick spacing = 12.05 fps)\",\n",
    );
    s.push_str(&format!("  \"duration_ms\": {},\n", film.duration_ticks()));
    s.push_str(&format!("  \"chunk_count\": {},\n", film.chunks.len()));
    let kinds: Vec<String> = film
        .chunk_kinds
        .iter()
        .map(|(kind, count)| format!("\"{kind}\": {count}"))
        .collect();
    s.push_str(&format!("  \"chunk_types\": {{{}}},\n", kinds.join(", ")));
    s.push_str(
        "  \"offset_note\": \"all chunk offsets are relative to the start of the inner .flm payload\",\n",
    );

    s.push_str("  \"frames\": [\n");
    for (i, frame) in film.frames.iter().enumerate() {
        s.push_str("    {\n");
        s.push_str(&format!("      \"index\": {i},\n"));
        s.push_str(&format!("      \"start_ms\": {},\n", frame.start_tick));
        s.push_str(&format!(
            "      \"duration_ms\": {},\n",
            frame.duration_tick
        ));
        s.push_str(&format!("      \"duplicate\": {},\n", frame.duplicate));
        s.push_str(&format!("      \"png\": \"frame_{i:03}.png\",\n"));
        let source = match (frame.full_chunk, frame.delta_chunk, frame.timestamp_chunk) {
            (Some(offset), _, _) => {
                format!("{{\"kind\": \"full\", \"chunk_offset\": {offset}}}")
            }
            (_, Some(delta), Some(timestamp)) => format!(
                "{{\"kind\": \"delta\", \"timestamp_chunk_offset\": {timestamp}, \"delta_chunk_offset\": {delta}}}"
            ),
            (_, None, Some(timestamp)) => {
                format!("{{\"kind\": \"duplicate\", \"timestamp_chunk_offset\": {timestamp}}}")
            }
            _ => "{\"kind\": \"unknown\"}".to_string(),
        };
        s.push_str(&format!("      \"source\": {source},\n"));
        let rects: Vec<String> = frame
            .delta_rects
            .iter()
            .map(|r| format!("[{},{},{},{}]", r[0], r[1], r[2], r[3]))
            .collect();
        s.push_str(&format!("      \"delta_rects\": [{}]\n", rects.join(", ")));
        if i + 1 == film.frames.len() {
            s.push_str("    }\n");
        } else {
            s.push_str("    },\n");
        }
    }
    s.push_str("  ],\n");

    let data_blocks = film.audio.iter().filter(|a| !a.sync).count();
    let sync_blocks = film.audio.iter().filter(|a| a.sync).count();
    s.push_str("  \"audio\": {\n");
    s.push_str(&format!("    \"chunk_count\": {},\n", film.audio.len()));
    s.push_str(&format!("    \"data_blocks\": {data_blocks},\n"));
    s.push_str(&format!("    \"sync_blocks\": {sync_blocks},\n"));
    s.push_str(
        "    \"note\": \"12-byte header [data_len, zero, block_index], then PCM bytes; sync blocks use block_index 4294967295. Audio is preserved verbatim by --original.\",\n",
    );
    s.push_str("    \"chunks\": [\n");
    for (i, audio) in film.audio.iter().enumerate() {
        s.push_str(&format!(
            "      {{\"chunk_offset\": {}, \"chunk_len\": {}, \"data_offset\": {}, \"data_len\": {}, \"index\": {}, \"sync\": {}}}{}\n",
            audio.chunk_offset,
            audio.chunk_len,
            audio.data_offset,
            audio.data_len,
            audio.index,
            audio.sync,
            if i + 1 == film.audio.len() { "" } else { "," }
        ));
    }
    s.push_str("    ]\n");
    s.push_str("  }\n");
    s.push_str("}\n");
    s
}

fn original_filename(index: usize, safe: &str) -> String {
    if safe.to_ascii_lowercase().ends_with(".flm") {
        format!("{index:05}_{safe}")
    } else {
        format!("{index:05}_{safe}.flm")
    }
}

fn strip_flm(safe: &str) -> String {
    if safe.to_ascii_lowercase().ends_with(".flm") {
        let stem = &safe[..safe.len() - 4];
        if !stem.is_empty() {
            return stem.to_string();
        }
    }
    safe.to_string()
}

fn read_u16_checked(data: &[u8], offset: usize) -> Result<u16> {
    if offset + 2 > data.len() {
        bail!("read past end of file at {offset:#x}");
    }
    Ok(read_u16(data, offset, true))
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(s, "{byte:02x}");
    }
    s
}

fn json_escape(text: &str) -> String {
    let mut s = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '"' => s.push_str("\\\""),
            '\\' => s.push_str("\\\\"),
            '\n' => s.push_str("\\n"),
            '\r' => s.push_str("\\r"),
            '\t' => s.push_str("\\t"),
            '\u{08}' => s.push_str("\\b"),
            '\u{0c}' => s.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                let _ = write!(s, "\\u{:04x}", c as u32);
            }
            c => s.push(c),
        }
    }
    s
}
