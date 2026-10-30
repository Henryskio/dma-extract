//! The first game's per-resource metadata region.
//!
//! Version 1 of the container keeps the later versions' shape up to the end
//! of the 32-byte names table: an offsets table, two bytes of padding, then
//! the names. Its metadata region is its own layout, smaller in places than
//! the versions 2 and 3 one, and is measured against every shipped first-game
//! container and against the loader in `DMA1.exe` (`0x4183xx`, the
//! `MMPictureResourceFile` reader).
//!
//! All three first-game kinds begin with a `4*N` u32 array. In sound
//! containers the values decode as Mac timestamps (seconds since 1904-01-01):
//! `0xaedb282b` is 1997-04-15, which matches the game's build date. In
//! picture containers the same array holds the per-resource handle word the
//! loader uses to look a picture up. What follows differs by kind:
//!
//! ```text
//! Pictures   4*N  u32 handle array
//!             2   marker, always 0x0007
//!             N   flags byte array
//!             N   kind byte array
//!            4*N  {u16 height, u16 width} dimension table
//!            4*N  u32 uncompressed-size table
//!             N   byte array whose accessor is unknown
//!             2   u16 palette count
//!      count*1024  palette bank
//! Sounds     4*N  u32 Mac timestamps
//!             N   format byte array
//!             2   padding
//! Blobs      4*N  u32 handle array
//!             2   padding
//! Scripts    (no version-1 metadata region; see `docs/GAME1.md` §7)
//! ```
//!
//! Picture metadata is therefore `15*N + 4` bytes plus the bank, and it must
//! end exactly at `offsets[0]`. The parser rejects a container where it does
//! not rather than guessing.
//!
//! ## The uncompressed-size table
//!
//! The `4*N` u32 array after the dimension table is **not** a size of the
//! stored payload and is not all zeros, which is how it was first read. It is
//! the size each payload expands to, and it is the discriminator that says
//! which storage form a payload is in:
//!
//! | container | `size == 0` | `size == len` | `size > len` |
//! |---|---|---|---|
//! | `HD.PIC` | 1682 | — | — |
//! | `Topolino.CPK` | 2 | — | — |
//! | `mpApp.PIC` | 274 | — | 110 |
//! | `pack.PIC` | 7775 | 125 | 2993 |
//!
//! * `size == 0` means the payload is stored as-is. This is 9,733 of the
//!   12,961 first-game pictures.
//! * `size > len` means the payload is an LZSS stream ([`super::lzss`])
//!   expanding to `size` bytes. 3,103 pictures (`pack.PIC` 2,993, `mpApp.PIC`
//!   110).
//! * `size == len` **also** means a stream. 125 `pack.PIC` resources declare a
//!   size equal to their own payload length, so the column reads like "stored"
//!   but the bytes differ; 107 of them decoded anyway, and treating the column
//!   as "not compressed" is what left the other 18 failing after the rest were
//!   fixed.
//!
//! Together that is 3,228 streams, 12,961 pictures, and **zero** undecoded
//! resources. The table in `docs/GAME1.md` §4.2 records the same counts as
//! they stood *before* the decompressor existed, which is the measurement that
//! identified the compressed set in the first place.

use crate::picture::PALETTE_BYTES;
use crate::{container::ContainerKind, container::Dimension};
use anyhow::{bail, Result};

/// Everything a first-game metadata region contributes.
#[derive(Debug, Default)]
pub struct V1Metadata {
    /// Sounds only: the per-resource format byte. Empty for the other kinds,
    /// because the caller gives them no `attr`.
    pub attr: Vec<u8>,
    /// The `flags` byte of each resource, container order.
    pub flags: Vec<u8>,
    /// The `kind` byte of each resource, container order.
    pub kinds: Vec<u8>,
    /// `{height, width}` pairs, container order.
    pub dimensions: Vec<Dimension>,
    /// Each resource's declared **uncompressed** size; `0` means stored
    /// as-is. See the module comment.
    pub uncompressed_sizes: Vec<u32>,
    /// The palette bank at the end of the region: `count` 1024-byte palettes.
    /// Empty for sounds, blobs and scripts.
    pub palettes: Vec<Vec<u8>>,
}

/// Parse a first-game metadata region.
///
/// `base` is the offset just past the `4*N` u32 handle array, which the
/// caller has already consumed, and `payload_start` is `offsets[0]`. The
/// region must end exactly at `payload_start`.
pub fn parse(
    data: &[u8],
    kind: ContainerKind,
    n: usize,
    base: usize,
    payload_start: usize,
    big_endian: bool,
    path: &std::path::Path,
) -> Result<V1Metadata> {
    let read_u16 = |off: usize| -> u16 {
        if big_endian {
            u16::from_be_bytes([data[off], data[off + 1]])
        } else {
            u16::from_le_bytes([data[off], data[off + 1]])
        }
    };
    let read_u32 = |off: usize| -> u32 {
        if big_endian {
            u32::from_be_bytes([
                data[off],
                data[off + 1],
                data[off + 2],
                data[off + 3],
            ])
        } else {
            u32::from_le_bytes([
                data[off],
                data[off + 1],
                data[off + 2],
                data[off + 3],
            ])
        }
    };

    let need = base + 4 * n;
    if need > data.len() {
        bail!(
            "{}: version 1 metadata does not fit (need {}, have {})",
            path.display(),
            need,
            data.len()
        );
    }

    let mut meta = V1Metadata::default();
    let mut cursor = base + 4 * n;

    match kind {
        ContainerKind::Pictures => {
            // The 2-byte sub-version marker. Every shipped picture container
            // reads 7 here, and `0x4183`'s reader requires it.
            if cursor + 2 > data.len() || read_u16(cursor) != 7 {
                bail!("{}: version 1 picture metadata marker is not 7", path.display());
            }
            cursor += 2;

            meta.flags.reserve(n);
            for i in 0..n {
                meta.flags.push(data[cursor + i]);
            }
            cursor += n;

            meta.kinds.reserve(n);
            for i in 0..n {
                meta.kinds.push(data[cursor + i]);
            }
            cursor += n;

            meta.dimensions.reserve(n);
            for i in 0..n {
                // The pair is {height, width}: the SECOND u16 is the width,
                // exactly as in versions 2 and 3.
                let h = read_u16(cursor + 4 * i);
                let w = read_u16(cursor + 4 * i + 2);
                meta.dimensions.push(Dimension { width: w, height: h });
            }
            cursor += 4 * n;

            meta.uncompressed_sizes.reserve(n);
            for i in 0..n {
                meta.uncompressed_sizes.push(read_u32(cursor + 4 * i));
            }
            cursor += 4 * n;

            // One more N-byte array whose accessor is unknown; skipped.
            if cursor + n > data.len() {
                bail!(
                    "{}: version 1 unknown byte array does not fit (need {}, have {})",
                    path.display(),
                    cursor + n,
                    data.len()
                );
            }
            cursor += n;

            let palette_count = read_u16(cursor) as usize;
            cursor += 2;

            meta.palettes.reserve(palette_count);
            for k in 0..palette_count {
                let at = cursor + k * PALETTE_BYTES;
                let want = at + PALETTE_BYTES;
                if want > data.len() {
                    bail!(
                        "{}: palette bank overruns the file (need {}, have {})",
                        path.display(),
                        want,
                        data.len()
                    );
                }
                meta.palettes.push(data[at..want].to_vec());
            }
            cursor += palette_count * PALETTE_BYTES;
        }
        // Sounds: one `N` byte array (the format byte) and 2 bytes of padding.
        // The `4*N` array ahead of it holds the Mac timestamps, which the
        // caller reads as `extra_u32`.
        ContainerKind::Sounds => {
            if cursor + n + 2 > data.len() {
                bail!(
                    "{}: version 1 sound metadata does not fit (need {}, have {})",
                    path.display(),
                    cursor + n + 2,
                    data.len()
                );
            }
            meta.attr.reserve(n);
            for i in 0..n {
                meta.attr.push(data[cursor + i]);
            }
            cursor += n + 2;
        }
        // Blobs: nothing beyond the 4*N array plus 2 bytes.
        ContainerKind::Blobs => cursor += 2,
        // Scripts: no version-1 metadata region is decoded.
        _ => {}
    }

    if cursor != payload_start {
        bail!(
            "{}: version 1 metadata ends at {} but the first payload starts at {}",
            path.display(),
            cursor,
            payload_start
        );
    }

    Ok(meta)
}
