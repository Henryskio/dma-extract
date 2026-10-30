//! The first game (1997), container version 1.
//!
//! Everything specific to the first game lives here so that the parsers and
//! decoders the second and third games share stay unaware of it: those live
//! in `crate::container`, `crate::picture`, `crate::lz77`, `crate::sound`,
//! `crate::film` and `crate::script`, and none of them mentions a version-1
//! variant. An edit inside this module cannot change how a version-2 or
//! version-3 container is read, which is the point of the split.
//!
//! What the first game changes is measured and documented in
//! `docs/GAME1.md`; the short version:
//!
//! * the resource count is the u16 at `0x1E` and the offsets table starts at
//!   `0x20`, four bytes earlier than the later versions' `0x24`;
//! * it *has* a per-resource metadata region, but its own layout
//!   ([`metadata`]);
//! * its palettes sit in a **bank** at the end of that region rather than
//!   being prefixed to each payload ([`palette_of`]);
//! * some payloads are LZSS-compressed and their compressed size is declared
//!   in that region ([`lzss`], [`uncompressed_len`]).
//!
//! ## Public API
//!
//! * [`metadata::parse`] turns the version-1 metadata region into the arrays
//!   the shared [`crate::Container`] needs.
//! * [`uncompressed_len`] and [`container::payload`] together give the bytes a
//!   picture decoder should read: the stored payload when the declared size is
//!   zero, the LZSS expansion otherwise.
//! * [`palette_of`] supplies the palette bank to [`crate::picture::decode`].
//! * [`picture::decode`] is the whole first-game picture path, which begins
//!   with the decompression step the shared decoders do not do.

pub mod lzss;
pub mod metadata;
pub mod picture;

pub use metadata::V1Metadata;

use crate::container::Container;
use crate::picture::PALETTE_BYTES;

/// The container's declared **uncompressed** size for resource `i`.
///
/// Zero means the payload is stored exactly as it appears in the container.
/// Any other value is the number of bytes [`lzss::decompress`] expands the
/// payload to, and is how a first-game container says "this payload is
/// compressed". See [`metadata`] for the table that separates them.
pub fn uncompressed_len(container: &Container, i: usize) -> usize {
    container.uncompressed_sizes.get(i).copied().unwrap_or(0) as usize
}

/// The palette resource `i` should index into, or `None` when it carries its
/// own.
///
/// First-game picture containers keep their palettes at the *end* of the
/// metadata region, immediately before the first payload, rather than
/// prefixing them to each payload the way versions 2 and 3 do. Each is 1024
/// bytes (256 entries of transparency, red, green, blue) and a run-length
/// sub-format resource whose colour records are single palette indices
/// indexes into one of them.
///
/// Entry 0 is the one returned. `mpApp.PIC` ships two palettes and nothing in
/// the container says which entry a picture indexes into -- there is no
/// per-resource selector field -- so that choice rests on a vision check
/// rather than a byte: rendering `dmalog074`/`dmalog079` and
/// `nameLess_230`/`nameLess_231` through both entries and asking
/// `qwen/qwen3.8-27b` which is the intended rendering returns "FIRST"
/// (entry 0) for both pictures tested. It affects colour only, and only for
/// the 35 `mpApp.PIC` resources that index the low-numbered slots.
pub fn palette_of(container: &Container, _i: usize) -> Option<&[u8]> {
    container.palettes.first().map(|p| p.as_slice())
}

/// The palette bank of a first-game picture container.
pub fn palette_bank(container: &Container) -> &[Vec<u8>] {
    &container.palettes
}

/// How many palettes a first-game picture container's bank holds.
pub fn palette_count(container: &Container) -> usize {
    container.palettes.len()
}

/// How many bytes one palette occupies.
pub const fn palette_bytes() -> usize {
    PALETTE_BYTES
}
