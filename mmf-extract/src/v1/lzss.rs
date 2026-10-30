//! The first game's LZSS decompressor.
//!
//! Transliterated from `0x42e0d4` in `DMA1.exe`. This is **not** the routine
//! the second and third games use (that one is [`crate::lz77`], a byte
//! oriented LZ77 with a match table at `0x4c562d` in `DMA4.exe`); the first
//! game predates it and the two are not interchangeable.
//!
//! The version-1 container's metadata region declares, per resource, the size
//! its payload expands to (see [`crate::v1::metadata`]). A declared size of
//! zero means the payload is stored as it appears and this routine is never
//! called. Anything else means the payload is this LZSS stream.
//!
//! ## The stream
//!
//! ```text
//! u8 control    one bit per token, least significant bit first; 8 tokens per
//!               control byte. bit 1 = a literal, bit 0 = a back-reference.
//! literal:      the next input byte, verbatim.
//! match:        the next two input bytes, big-endian:
//!                  distance = word & 0x3FF       (10 bits, window indices)
//!                  length   = (byte0 >> 2) + 3    (3..=66)
//! ```
//!
//! Both forms append every byte they produce to the output *and* to
//! `window[write_cursor & 0x3FF]`; the cursor then advances one and wraps.
//! A match copies `length` bytes from `window[(distance + i) & 0x3FF]`, so
//! the same window position can be read repeatedly and overlapping runs are
//! legal.
//!
//! ## The window
//!
//! 1024 bytes. The original zeroes the first 958 of them at entry
//! (`0x42e0f7`, counting to `0x3be`) and starts the write cursor at 958
//! (`0x42e104`), i.e. it zeroes exactly the slots the cursor has not reached
//! yet. Zeroing the whole buffer is equivalent and is what this port does,
//! because the game's window lives at the fixed address `0x517540` and would
//! otherwise carry stale bytes between calls.
//!
//! The decoder stops as soon as it has produced the declared number of bytes;
//! the remaining input, if any, is padding.

use anyhow::{bail, Result};

/// Window size of the version-1 LZSS, and the initial write cursor.
///
/// `0x42e104` loads `eax` with `0x3be` (958) and every later update masks it
/// with `0x3ff`.
const V1_WINDOW: usize = 1024;

/// Where the write cursor starts. `0x42e104`: `mov eax,0x3be`.
const V1_WINDOW_START: usize = 958;

/// Decompress a version-1 LZSS stream to exactly `uncompressed` bytes.
///
/// `uncompressed` is the container's declared size for the resource; zero
/// means "stored as-is" and the caller must not call this.
///
/// Returns an error if the input runs out before that many bytes could be
/// produced, which means the payload does not match its declaration.
pub fn decompress(src: &[u8], uncompressed: usize) -> Result<Vec<u8>> {
    let mut window = [0u8; V1_WINDOW];
    let mut wp = V1_WINDOW_START;
    let mut out: Vec<u8> = Vec::with_capacity(uncompressed);
    let mut pos = 0usize;

    // The control byte and how many of its bits are still unconsumed.
    let mut control: u8 = 0;
    let mut bits_left: u8 = 0;

    while out.len() < uncompressed {
        // Refill the control byte every 8 tokens.
        if bits_left == 0 {
            let Some(&c) = src.get(pos) else {
                bail!("v1 lzss: control byte exhausted at input offset {pos}");
            };
            pos += 1;
            control = c;
            bits_left = 8;
        }
        let is_literal = control & 1 == 1;
        control >>= 1;
        bits_left -= 1;

        if is_literal {
            // `0x42e130`: one input byte, written to the output and the window.
            let Some(&b) = src.get(pos) else {
                bail!("v1 lzss: literal byte exhausted at input offset {pos}");
            };
            pos += 1;
            out.push(b);
            if out.len() >= uncompressed {
                break;
            }
            window[wp & 0x3FF] = b;
            wp = wp.wrapping_add(1) & 0x3FF;
        } else {
            // `0x42e158`: two input bytes, big-endian, split distance/length.
            let (Some(&b0), Some(&b1)) = (src.get(pos), src.get(pos + 1)) else {
                bail!("v1 lzss: match pair exhausted at input offset {pos}");
            };
            pos += 2;
            let word = ((b0 as usize) << 8) | b1 as usize;
            let distance = word & 0x3FF;
            let length = (b0 as usize >> 2) + 3;
            for i in 0..length {
                let b = window[(distance + i) & 0x3FF];
                out.push(b);
                if out.len() >= uncompressed {
                    break;
                }
                window[wp & 0x3FF] = b;
                wp = wp.wrapping_add(1) & 0x3FF;
            }
        }
    }

    Ok(out)
}
