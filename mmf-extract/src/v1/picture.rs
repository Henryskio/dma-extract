//! The first game's picture payload path.
//!
//! The shared [`crate::picture`] decoders take a payload that is already in
//! its final layout. A first-game picture resource is not always: when the
//! container's metadata declares a non-zero uncompressed size for it, the
//! stored payload is an LZSS stream ([`super::lzss`]) and must be expanded
//! before anything else can happen.
//!
//! So the first game's path is three steps where the shared one is two:
//!
//! 1. read the stored payload from the container;
//! 2. if the container declares a non-zero uncompressed size, expand it;
//! 3. hand the result to the shared [`crate::picture::decode`].
//!
//! The expanded stream is an ordinary `crate::picture` payload: the same
//! `2*(2h+1)` count word, the same big-endian offset table, the same
//! alternating transparent/opaque run bytes and the same colour records. That
//! is measured, not assumed: `pack.PIC` index 9062 (`mickey_j.PICT`, 222x202,
//! 10280 stored bytes, 23300 declared) expands to 23300 bytes whose table's
//! last entry is exactly 23300, whose 222 run slices each sum to the width
//! 202, and whose 222 colour slices each hold exactly one record per
//! foreground pixel.
//!
//! `crate::picture::decode` needs the palette bank, because a first-game
//! run-length resource with one-byte colour records indexes into it rather
//! than carrying a palette inline; see [`super::palette_of`].

use crate::picture::{self, Decoded};
use crate::{container::Container, v1::lzss};
use anyhow::{Context, Result};

/// The bytes a shared [`crate::picture`] decoder should look at for resource
/// `i`.
///
/// This is the stored payload, unless the container declares a non-zero
/// uncompressed size for it, in which case it is the LZSS expansion.
pub fn payload(container: &Container, i: usize) -> Result<Vec<u8>> {
    let raw = container.payload(i);
    let uncompressed = super::uncompressed_len(container, i);
    if uncompressed == 0 {
        return Ok(raw.to_vec());
    }
    lzss::decompress(raw, uncompressed).with_context(|| {
        format!(
            "expanding resource {i} ('{}') from {} to {uncompressed} bytes",
            container.name(i),
            raw.len()
        )
    })
}

/// Decode a first-game picture resource to straight RGBA.
pub fn decode(container: &Container, i: usize) -> Result<Decoded> {
    let data = payload(container, i)?;
    let palette = super::palette_of(container, i);
    picture::decode_bytes(container, i, &data, palette)
}

/// Which layout resource `i` is stored in, after any LZSS expansion.
pub fn pixel_format(container: &Container, i: usize) -> Option<picture::PixelFormat> {
    let data = payload(container, i).ok()?;
    let palette = super::palette_of(container, i);
    picture::pixel_format(container, i, &data, palette)
}
