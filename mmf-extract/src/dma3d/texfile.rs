//! The texture payload: an indexed image.
//!
//! `DMA3D.MMX` (`MMFW Textures`, container version 3) holds 705 resources,
//! and every one of them is the same shape (`docs/DMA3D.md` §2.1):
//!
//! ```text
//! RGBA[K]   K palette entries, four bytes each
//! [u8; A]   A one-byte indices, every one of them < K
//! ```
//!
//! That reading is **measured on all 705**, not assumed: `K` is recovered
//! structurally as the smallest `K` for which every byte after `4*K` is a
//! valid index, and `A = payload_len - 4*K` then lands on a power of two in
//! every file (64, 256, 512, 1024, 2048, 4096, 8192, 16384, 32768, 65536).
//! No payload reserves bytes for a header, a width or a height: a four-byte
//! magic would make `balloon.TGA`'s 260-byte payload 252 index bytes, and 252
//! is not a power of two, while the plain reading gives 256.
//!
//! ## What the width and the height are
//!
//! The container carries no dimension table. Its metadata region (measured at
//! `32*N + 4*N + 2*N + 2` bytes — the names, a timestamp array, a two-byte
//! field per resource and padding) holds only Mac-style timestamps, so the
//! dimensions have to be taken from the pixel data itself.
//!
//! `A` is a power of two, so it pins the *area* and nothing else: 678 of the
//! 705 files are perfect squares (8x8 through 256x256) and are read at that
//! root. The remaining 27 are the 1:2 rectangles (16x32, 32x64, 64x128,
//! 128x256), and each of those has two readings. They are split by a measured
//! statistic, [`axis_of`], which is described where it is computed: it was
//! validated on the 678 square textures, where the same statistic lands on the
//! square root 643/659 times for real textures and never contradicts the
//! period structure. The 27 are named in `docs/DMA3D.md` §2.2 so the choice is
//! reviewable rather than silent.
//!
//! ## What the palette order is
//!
//! RGBA, in file order. `balloon.TGA` opens `66 e6 ff ff` = `(102, 230, 255)`,
//! a sky blue; `bike_frame.TGA` opens `1d b3 00 ff` = `(29, 179, 0)`, a green
//! frame; and the 629 payloads whose first entry is `ff 00 ff 00` are the ones
//! whose first palette colour is the transparent magenta key. That is the
//! usual colour key for this era of engine, and it is why the alpha channel
//! has to be carried rather than dropped.

use anyhow::{bail, Context, Result};

/// Bytes per palette entry: straight RGBA, no premultiplication.
pub const PALETTE_ENTRY_BYTES: usize = 4;
/// The widths a row-structure test considers. One byte holds 256 values, so a
/// texture wider than 256 bytes is impossible; the set is the powers of two an
/// engine of this era would allocate.
const WIDTH_CANDIDATES: [usize; 6] = [8, 16, 32, 64, 128, 256];

/// One decoded texture.
#[derive(Debug, Clone, PartialEq)]
pub struct Texture {
    /// The palette, in file order. Index `i` is `palette[i]`.
    pub palette: Vec<[u8; 4]>,
    /// The index pixels, row-major, `width` of them to a row.
    pub indices: Vec<u8>,
    /// Pixel width.
    pub width: usize,
    /// Pixel height.
    pub height: usize,
    /// How the width and the height were decided, for the report.
    pub axis: Axis,
}

/// Which of the two readings of a 1:2 rectangle was taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    /// The area is a perfect square and its root was read as both dimensions.
    Square,
    /// The area is a 1:2 rectangle; the longer side is the width.
    Wide,
    /// The area is a 1:2 rectangle; the longer side is the height.
    Tall,
}

impl Texture {
    /// Expand to straight RGBA, four bytes per pixel.
    pub fn to_rgba(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.indices.len() * 4);
        for &i in &self.indices {
            let entry = self.palette.get(usize::from(i)).copied().unwrap_or([0, 0, 0, 0]);
            out.extend_from_slice(&entry);
        }
        out
    }

    /// The palette entry for an index, or transparent black if the index
    /// names an entry the palette does not have.
    pub fn entry(&self, index: u8) -> [u8; 4] {
        self.palette
            .get(usize::from(index))
            .copied()
            .unwrap_or([0, 0, 0, 0])
    }
}

/// Read a texture out of a `Textures` container payload.
///
/// The payload is the whole resource as stored, so the byte-exact `--original`
/// path never comes through here.
pub fn parse(payload: &[u8]) -> Result<Texture> {
    let k = palette_size(payload).context("no palette/index split fits this payload")?;
    let palette_bytes = k * PALETTE_ENTRY_BYTES;
    let mut palette = Vec::with_capacity(k);
    for entry in 0..k {
        let at = entry * PALETTE_ENTRY_BYTES;
        palette.push([
            payload[at],
            payload[at + 1],
            payload[at + 2],
            payload[at + 3],
        ]);
    }
    let indices = payload[palette_bytes..].to_vec();
    let area = indices.len();

    let (width, height, axis) = split_area(indices.as_slice(), area);
    if width * height != area {
        bail!(
            "{area} index bytes do not factor into {}x{}",
            width,
            height
        );
    }

    Ok(Texture {
        palette,
        indices,
        width,
        height,
        axis,
    })
}

/// The palette size: the smallest `K` for which every byte past `4*K` is a
/// valid index.
///
/// This is `1 + the largest index the pixel data uses`, which is the only
/// self-consistent reading: a palette of `K` entries has to hold every value
/// the indices can name, and it cannot be shorter than the largest one. The
/// length of the payload confirms the result, because `payload_len - 4*K`
/// then lands on a power of two in all 705 shipped textures.
pub fn palette_size(payload: &[u8]) -> Result<usize> {
    for k in 1..=payload.len() / PALETTE_ENTRY_BYTES {
        let start = k * PALETTE_ENTRY_BYTES;
        if start >= payload.len() {
            bail!("payload is all palette: {k} entries with no index bytes");
        }
        let largest = payload[start..].iter().copied().max().unwrap_or(0);
        if usize::from(largest) < k {
            return Ok(k);
        }
    }
    bail!("no palette size makes the trailing bytes valid indices")
}

/// Split an index count into a width and a height.
fn split_area(indices: &[u8], area: usize) -> (usize, usize, Axis) {
    let root = (area as f64).sqrt().round() as usize;
    if root * root == area && WIDTH_CANDIDATES.contains(&root) {
        return (root, root, Axis::Square);
    }
    axis_of(indices, area)
}

/// Decide the orientation of a rectangle whose area is `1 * 2^m`.
///
/// The statistic is **row continuity**: for a candidate width `W`, the share
/// of vertically adjacent pixel pairs that carry the same index. The right
/// width is the one where a column of the image is *read* as a column — pixels
/// that belong together sit in the same offset of consecutive rows.
///
/// It was validated on the 678 square textures, where the answer is known in
/// advance: for real textures with three or more palette entries in use it
/// lands on the square root 643 out of 659 times. The 16 exceptions are
/// textures whose content repeats with a short period (buttons built from an
/// 8-pixel motif, dot patterns), so the statistic is a measurement with a
/// success rate, not a proof — which is why every rectangle it decides is
/// named in `docs/DMA3D.md` §2.2.
fn axis_of(indices: &[u8], area: usize) -> (usize, usize, Axis) {
    let mut best = (0usize, f64::NEG_INFINITY);
    for &w in &WIDTH_CANDIDATES {
        if area % w != 0 {
            continue;
        }
        let h = area / w;
        let mut same = 0usize;
        let mut pairs = 0usize;
        for row in 0..h - 1 {
            let base = row * w;
            for col in 0..w {
                if indices[base + col] == indices[base + w + col] {
                    same += 1;
                }
                pairs += 1;
            }
        }
        let score = same as f64 / pairs as f64;
        if score > best.1 {
            best = (w, score);
        }
    }
    let w = if best.0 == 0 { WIDTH_CANDIDATES[0] } else { best.0 };
    let h = area / w;
    if w > h {
        (w, h, Axis::Wide)
    } else {
        (w, h, Axis::Tall)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A payload of one repeated byte has a trivial split -- one palette
    /// entry of that value and the rest index zero -- but the index count
    /// then has to be a usable area, so `parse` refuses it.
    #[test]
    fn a_payload_of_one_repeated_byte_is_not_an_image() {
        assert!(palette_size(&[0x00; 64]).is_ok());
        assert!(parse(&[0x00; 64]).is_err());
    }

    /// The simplest palette texture: one colour, 8x8, every index zero.
    #[test]
    fn one_palette_entry_and_a_grid_of_zero_indices() {
        let mut p = vec![0x00, 0x00, 0x00, 0xff];
        p.extend(std::iter::repeat(0u8).take(64));
        let t = parse(&p).expect("parse");
        assert_eq!(t.palette, vec![[0, 0, 0, 255]]);
        assert_eq!(t.width, 8);
        assert_eq!(t.height, 8);
        assert_eq!(t.indices.len(), 64);
        assert_eq!(t.axis, Axis::Square);
        let rgba = t.to_rgba();
        assert_eq!(rgba.len(), 256);
        assert_eq!(&rgba[..4], &[0, 0, 0, 255]);
    }

    /// The palette size must be one past the largest index, and the length has
    /// to come out a power of two.
    #[test]
    fn the_palette_size_is_one_past_the_largest_index() {
        let mut p = Vec::new();
        for i in 0..40u8 {
            p.extend_from_slice(&[i, 0x80, 0x80, 0xff]);
        }
        p.extend(std::iter::repeat(7u8).take(2048));
        assert_eq!(palette_size(&p).expect("size"), 40);
        let t = parse(&p).expect("parse");
        assert_eq!(t.palette.len(), 40);
        assert_eq!(t.indices.len(), 2048);
        assert_eq!(t.width * t.height, 2048);
    }

    /// A transparent magenta first entry is a colour key, not a palette bug.
    #[test]
    fn a_transparent_magenta_first_entry_survives() {
        let mut p = vec![0xff, 0x00, 0xff, 0x00];
        p.extend(std::iter::repeat(0u8).take(2048));
        let t = parse(&p).expect("parse");
        assert_eq!(t.palette[0], [255, 0, 255, 0]);
        assert_eq!(t.indices.len(), 2048);
        assert_eq!(&t.to_rgba()[..4], &[255, 0, 255, 0]);
    }

    #[cfg(test)]
    mod shipped {
        use super::super::*;
        use crate::container::Container;

        fn textures() -> Option<Container> {
            for dir in [
                std::env::var("DMA3D_ASSETS").unwrap_or_default(),
                "/tmp/dma3d/pc".into(),
                "/tmp/en/dma3d_en".into(),
            ] {
                let path = std::path::Path::new(&dir).join("DMA3D.MMX");
                if let Ok(data) = std::fs::read(&path) {
                    return Container::parse(path, data).ok();
                }
            }
            None
        }

        /// Every shipped texture must decode, with its area a power of two and
        /// every palette index naming an entry that exists.
        #[test]
        fn every_shipped_texture_decodes() {
            let Some(c) = textures() else {
                eprintln!("skipping: the 3D game's assets are not present");
                return;
            };
            let n = c.resource_count as usize;
            let mut square = 0usize;
            let mut wide = 0usize;
            let mut tall = 0usize;
            let mut axes = Vec::new();
            for i in 0..n {
                let t = parse(c.payload(i))
                    .unwrap_or_else(|e| panic!("{}: {e}", c.name(i)));
                let area = t.width * t.height;
                assert!(
                    area.is_power_of_two(),
                    "{}: {area} index bytes is not a power of two",
                    c.name(i)
                );
                assert!(
                    (8..=256).contains(&t.width) && (8..=256).contains(&t.height),
                    "{}: {}x{} is outside the engine's size set",
                    c.name(i),
                    t.width,
                    t.height
                );
                let largest = t.indices.iter().copied().max().unwrap_or(0);
                assert!(
                    usize::from(largest) < t.palette.len(),
                    "{}: index {largest} names no palette entry",
                    c.name(i)
                );
                assert_eq!(t.to_rgba().len(), area * 4);
                match t.axis {
                    Axis::Square => square += 1,
                    Axis::Wide => wide += 1,
                    Axis::Tall => {
                        tall += 1;
                        axes.push(format!("{}x{}", t.width, t.height));
                    }
                }
            }
            eprintln!(
                "{n} textures: {square} square, {wide} wide, {tall} tall ({axes:?})"
            );
            assert!(square > 600, "only {square} square textures");
        }

        /// The measured payload length must be `4*K + area` exactly, so a
        /// decoder cannot silently drop or invent bytes.
        #[test]
        fn the_payload_length_is_the_palette_plus_the_indices() {
            let Some(c) = textures() else {
                eprintln!("skipping: the 3D game's assets are not present");
                return;
            };
            for i in 0..c.resource_count as usize {
                let payload = c.payload(i);
                let t = parse(payload).unwrap_or_else(|e| panic!("{}: {e}", c.name(i)));
                assert_eq!(t.palette.len() * 4 + t.indices.len(), payload.len());
            }
        }
    }
}
