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
//! `A` is a power of two, so it pins the *area* and nothing else. **678 of the
//! 705 files are perfect squares** (8x8 through 256x256) and are read at that
//! root, which is certain. The remaining **27 are 1:2 rectangles** and each has
//! two readings, decided by [`axis_of`] from the pixel data alone.
//!
//! Two facts make that possible, and both were measured rather than assumed:
//!
//! 1. **The two readings are not transposes of one another.** Reading `A` bytes
//!    at width `W` puts true rows `r` and `r+1` at `r*W` and `(r+1)*W`; reading
//!    them at `2W` puts them at `2r*2W` and `(2r+1)*2W`. The wrong width
//!    *concatenates* two true rows into one, squashing and interleaving the
//!    picture. So statistics on the index bytes are decisive rather than merely
//!    anisotropic.
//! 2. **Every rectangle the engine allocates is 1:2.** All 21 of the 27 whose
//!    orientation could be established independently are 1:2 — 64x32, 32x16,
//!    32x64, 64x128, 256x128, 128x256 — so the width is restricted to that pair
//!    rather than to every power of two that divides the area. (An earlier
//!    version considered all six, and read `beestripes` as 8x64 and
//!    `createBtFileIo` as 8x256, which contradicted this.)
//!
//! [`decide`] picks the reading with fewest 4-connected components of
//! non-background pixels — at the correct width an icon is one object, at the
//! wrong one it is sliced — then breaks ties by the mean index difference across
//! a vertical step, then by zero pixels cut on the frame border. The rule
//! reproduces all 21 independently-established orientations, and
//! `tests/dma3d.rs` pins them by name.
//!
//! **What is not decided**, and is reported as such: `createBtFileIo.TGA` has
//! `K == 1` — one palette entry and 2048 identical index bytes — so *no*
//! statistic can distinguish any reading from any other; it is
//! [`Texture::undecidable`]. Four more are genuinely open, and
//! `docs/DMA3D.md` §4.2 names them with the competing evidence.
//!
//! ## What the palette order is
//!
//! RGBA, in file order. `balloon.TGA` opens `66 e6 ff ff` = `(102, 230, 255)`,
//! a sky blue; `bike_frame.TGA` opens `1d b3 00 ff` = `(29, 179, 0)`, a green
//! frame; and the 629 payloads whose first entry is `ff 00 ff 00` are the ones
//! whose first palette colour is the transparent magenta key. That is the
//! usual colour key for this era of engine, and it is why the alpha channel
//! has to be carried rather than dropped.

use anyhow::{Context, Result, bail};

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
    /// True when the index data carries no orientation information at all, so
    /// the reading is a default rather than a measurement.
    ///
    /// `createBtFileIo.TGA` is the one: `K == 1`, a single palette entry and
    /// 2048 identical index bytes. Every statistic is degenerate at every
    /// width, so no reading differs from any other and the only honest things
    /// to do are to report it and to say why.
    pub undecidable: bool,
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
            let entry = self
                .palette
                .get(usize::from(i))
                .copied()
                .unwrap_or([0, 0, 0, 0]);
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
    match parse_paletted(payload) {
        Ok(t) => Ok(t),
        Err(e) => parse_mask(payload).with_context(|| {
            format!("no palette/index split fits this payload ({e}); the mask reading also fails")
        }),
    }
}

/// A payload that carries its own palette and its index area.
fn parse_paletted(payload: &[u8]) -> Result<Texture> {
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

    // The area is a power of two in all 705 shipped textures, and that is what
    // makes the root and the 1:2 pair exact. A payload whose area is not one
    // does not fit this format, and is reported rather than factored into a
    // width no texture in this engine can have.
    if !area.is_power_of_two() {
        bail!("{area} index bytes is not a power of two, so no width and height fit");
    }

    let (width, height, axis) = split_area(indices.as_slice(), area);
    if width * height != area {
        bail!("{area} index bytes do not factor into {width}x{height}");
    }
    // A width below `WIDTH_CANDIDATES` is one this engine does not allocate, so
    // the reading is reported rather than decoded. Three of the Italian disc's
    // payloads carry 8, 16 and 32 bytes of indices, whose only split is 4x2 or
    // 8x4: the product is right and neither dimension is
    // (`docs/DMA3D.md` §11.6).
    if !WIDTH_CANDIDATES.contains(&width) || !WIDTH_CANDIDATES.contains(&height) {
        bail!(
            "{area} index bytes do not factor into a width this engine can allocate \
             ({width}x{height} is neither size of {WIDTH_CANDIDATES:?})"
        );
    }

    // Whether the pixel data could tell the two readings apart at all. It is
    // recorded rather than acted on: an undecidable texture is still written,
    // because a solid rectangle is a usable placeholder, and it is reported so
    // nobody mistakes the default for a measurement.
    let undecidable = area.trailing_zeros() % 2 == 1 && {
        let (w, h) = wide_tall(area);
        !decide(indices.as_slice(), w, h).1
    };

    Ok(Texture {
        palette,
        indices,
        width,
        height,
        axis,
        undecidable,
    })
}

/// Read a payload as the 8-bit coverage mask it is when it carries no palette.
///
/// `docs/PICTURES.md` §4.2 documents this sub-format for the picture containers:
/// an 8-bit payload with no 1024-byte prefix is not a palette image at all but a
/// **coverage (alpha) mask**, each byte an alpha value for a shape the game tints
/// at runtime, rendered white. That is what nine of the Italian disc's textures
/// are — measured in `docs/DMA3D.md` §11.14 — and it is why none of them can
/// hold a palette: they have none. `white`, `tan` and `purple` carry bodies using
/// only the values `0` and `1`; `dark_grey` and `orange` carry a handful of grey
/// levels. All nine have a power-of-two area behind a 4, 36 or 72-byte header,
/// and every area lands on a width this engine allocates.
///
/// The area is the largest power of two the payload leaves room for after a
/// 4-byte header, which is the same rule the palette reading uses (the payload
/// length is ground truth) and lands on 16x16, 8x8 or 32x16 for these nine.
fn parse_mask(payload: &[u8]) -> Result<Texture> {
    let mut area = 1usize;
    while area * 2 <= payload.len().saturating_sub(4) {
        area *= 2;
    }
    if area < 64 {
        bail!("{area} mask bytes cannot fill a texture this engine can allocate");
    }
    let at = payload.len() - area;
    let indices = payload[at..].to_vec();
    let (width, height, axis) = split_area(indices.as_slice(), area);
    if width * height != area {
        bail!("{area} mask bytes do not factor into {width}x{height}");
    }
    if !WIDTH_CANDIDATES.contains(&width) || !WIDTH_CANDIDATES.contains(&height) {
        bail!(
            "{area} mask bytes do not factor into a width this engine can allocate \
             ({width}x{height} is neither size of {WIDTH_CANDIDATES:?})"
        );
    }
    Ok(Texture {
        palette: Vec::new(),
        indices,
        width,
        height,
        axis,
        // A mask's orientation is undecidable whenever the area is not a square,
        // and decided exactly as a palette image's would be.
        undecidable: area.trailing_zeros() % 2 == 1 && {
            let (w, h) = wide_tall(area);
            !decide(payload[at..].as_ref(), w, h).1
        },
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
/// Every palette size a payload admits, smallest first.
///
/// A split of `K` entries is admissible when the area left over
/// (`payload_len - 4*K`) is a power of two and every index byte names an entry
/// that exists. The exponent is walked from the largest area that leaves at
/// least one palette entry down to a single pixel, so the smallest admissible
/// palette comes first.
///
/// This replaces the shipped rule, which took the smallest `K` that made the
/// indices valid and only then demanded a power-of-two area: a payload whose
/// smallest valid `K` left a non-power-of-two area was rejected wholesale. On
/// the Italian `DMA3D.MMX` that rejected 577 of 781 payloads as "N index bytes
/// is not a power of two" when the payload's own length settles the area.
/// `docs/DMA3D.md` §11 records what the constraint does *not* pin down.
pub fn palette_sizes(payload: &[u8]) -> Vec<usize> {
    let mut out = Vec::new();
    // Start below the payload's own length: an area as large as the payload
    // would leave a palette of zero entries, which is not a split.
    let mut area = payload.len().next_power_of_two() / 2;
    while area > 0 && area + 4 <= payload.len() {
        let palette_bytes = payload.len() - area;
        if palette_bytes % PALETTE_ENTRY_BYTES == 0 {
            let k = palette_bytes / PALETTE_ENTRY_BYTES;
            if k > 0 {
                let largest = payload[palette_bytes..].iter().copied().max().unwrap_or(0);
                if usize::from(largest) < k {
                    out.push(k);
                }
            }
        }
        area /= 2;
    }
    out
}

/// Whether the payload's own bytes pin its palette size.
///
/// `true` when more than one admissible split exists, so the palette read is a
/// default rather than a measurement. Recorded rather than acted on.
pub fn palette_ambiguous(payload: &[u8]) -> bool {
    palette_sizes(payload).len() > 1
}

/// The palette size to read the payload with: the smallest admissible one.
pub fn palette_size(payload: &[u8]) -> Result<usize> {
    match palette_sizes(payload).first() {
        Some(&k) => Ok(k),
        None => bail!("no palette size makes the trailing bytes valid indices"),
    }
}

/// Split an index count into a width and a height.
fn split_area(indices: &[u8], area: usize) -> (usize, usize, Axis) {
    let root = (area as f64).sqrt().round() as usize;
    if root * root == area && WIDTH_CANDIDATES.contains(&root) {
        return (root, root, Axis::Square);
    }
    let (w, h) = wide_tall(area);
    // `wide_tall` answers a 1:2 pair only for an area that is not a perfect
    // square: its product is the area for an odd exponent and one power of two
    // too large for an even one. A square whose root is below the smallest
    // width candidate lands here (area 16, root 4), and `decide` reading that
    // pair would index past the end of the slice. So the pair is returned
    // unchecked and the caller's `width * height != area` rejection says no.
    if w * h != area {
        return (w, h, Axis::Tall);
    }
    if decide(indices, w, h).0 {
        (w, h, Axis::Wide)
    } else {
        (h, w, Axis::Tall)
    }
}

/// The 1:2 pair an area of `2^(2k+1)` is made of.
///
/// Taking `sqrt` and halving it does not work: it lands on an odd number and
/// would print a reading like `45x22` that no texture in this engine has. The
/// area is a power of two, so the two factors are powers of two too.
fn wide_tall(area: usize) -> (usize, usize) {
    debug_assert!(area.is_power_of_two());
    let bits = area.trailing_zeros() as usize;
    (1usize << (bits / 2 + 1), 1usize << (bits / 2))
}

/// True when the `wide` reading is the right one.
///
/// The rule is **the mean index difference across a vertical step, with two
/// vetoes on top of it**. At the correct width a vertical step is one true row,
/// so a smooth image is smooth; at twice the width it is two true rows, so it is
/// not. Validated on the 678 square textures, where this statistic lands on the
/// root 659/678 against 649/678 for the row-continuity it replaces.
///
/// The vetoes, both of which are about the *shape* of the picture and both
/// strong enough to override the statistic:
///
/// 1. **one connected component.** At the correct width an icon is one object;
///    at the wrong width its rows have been concatenated and it is sliced into
///    two or more. A reading that is uniquely `components == 1` wins even when
///    the statistic disagrees — `createModMickeyPartHeadS.TGA` is exactly that
///    case, and it is the reason the vetoes are applied first rather than the
///    statistic.
/// 2. **zero pixels cut on the frame border.** A non-background pixel on the
///    border is an object the reading has cut in half.
///
/// Returns `false` for both when the index data cannot tell the two apart —
/// every byte identical — so the caller reports the texture as undecidable
/// rather than as decided.
fn decide(indices: &[u8], wide: usize, tall: usize) -> (bool, bool) {
    let wide_shape = shape(indices, wide, tall);
    let tall_shape = shape(indices, tall, wide);
    let wide_delta = vertical_delta(indices, wide, tall);
    let tall_delta = vertical_delta(indices, tall, wide);

    // The vetoes, strongest first. Each is applied only when it is *unique*, so
    // two readings that both cut the border or both hold several objects fall
    // through to the statistic.
    if wide_shape.components == 1 && tall_shape.components != 1 {
        return (true, true);
    }
    if tall_shape.components == 1 && wide_shape.components != 1 {
        return (false, true);
    }
    if wide_shape.cut == 0 && tall_shape.cut != 0 {
        return (true, true);
    }
    if tall_shape.cut == 0 && wide_shape.cut != 0 {
        return (false, true);
    }

    // The statistic, then. A tie is a tie: the pixel data says nothing, and a
    // coin flip dressed as a measurement is what this whole decision exists to
    // avoid.
    let order = wide_delta.partial_cmp(&tall_delta).unwrap_or(std::cmp::Ordering::Equal);
    match order {
        std::cmp::Ordering::Less => (true, true),
        std::cmp::Ordering::Greater => (false, true),
        std::cmp::Ordering::Equal => (false, false),
    }
}

/// Mean absolute index difference across a vertical step.
fn vertical_delta(indices: &[u8], w: usize, h: usize) -> f64 {
    if h < 2 {
        return 0.0;
    }
    let mut total = 0u64;
    let mut pairs = 0usize;
    for row in 0..h - 1 {
        let base = row * w;
        for col in 0..w {
            total += u64::from(indices[base + col].abs_diff(indices[base + w + col]));
            pairs += 1;
        }
    }
    total as f64 / pairs as f64
}

/// The shape of the non-background pixels at one reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Shape {
    /// 4-connected components of non-background pixels.
    components: usize,
    /// Non-background pixels sitting on the frame's border: an object the
    /// reading has cut in half.
    cut: usize,
}

/// The background index: the palette's transparent colour key when it has one,
/// otherwise the index the data uses most.
///
/// Without a key the second choice is the only "empty" a texture has, and it is
/// the one an engine would leave the untouched part of the buffer holding.
fn background(indices: &[u8], palette_len: usize) -> u8 {
    if palette_len == 1 {
        return 0;
    }
    let mut counts = vec![0usize; palette_len];
    for &i in indices {
        if let Some(slot) = counts.get_mut(usize::from(i)) {
            *slot += 1;
        }
    }
    let mut best = 0u8;
    for (i, count) in counts.iter().enumerate() {
        if *count > counts[usize::from(best)] {
            best = i as u8;
        }
    }
    best
}

/// Count components and cut pixels for one reading.
fn shape(indices: &[u8], w: usize, h: usize) -> Shape {
    let palette_len = indices
        .iter()
        .copied()
        .max()
        .map(|m| usize::from(m) + 1)
        .unwrap_or(1);
    let bg = usize::from(background(indices, palette_len));
    let mut seen = vec![false; w * h];
    let mut components = 0usize;
    let mut cut = 0usize;
    let mut stack = Vec::new();
    for row in 0..h {
        for col in 0..w {
            let at = row * w + col;
            if seen[at] || usize::from(indices[at]) == bg {
                continue;
            }
            // A new component: flood fill it, so the count does not depend on
            // the scan order.
            if row == 0 || row == h - 1 || col == 0 || col == w - 1 {
                cut += 1;
            }
            components += 1;
            seen[at] = true;
            stack.push(at);
            while let Some(at) = stack.pop() {
                let row = at / w;
                let col = at % w;
                for (dr, dc) in [(-1i32, 0), (1, 0), (0, -1), (0, 1)] {
                    let nr = row as i32 + dr;
                    let nc = col as i32 + dc;
                    if nr < 0 || nc < 0 || nr >= h as i32 || nc >= w as i32 {
                        continue;
                    }
                    let next = nr as usize * w + nc as usize;
                    if seen[next] || usize::from(indices[next]) == bg {
                        continue;
                    }
                    seen[next] = true;
                    if nr == 0 || nr == h as i32 - 1 || nc == 0 || nc == w as i32 - 1 {
                        cut += 1;
                    }
                    stack.push(next);
                }
            }
        }
    }
    Shape { components, cut }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A payload of one repeated byte has a trivial split -- one palette
    /// entry of that value and the rest index zero -- but the index count
    /// then has to be a usable area, so `parse` refuses it.
    #[test]
    fn a_payload_of_one_repeated_byte_is_not_an_image() {
        // Which repeated byte it is decides the answer, and that is the point.
        // `0x00` leaves an admissible split: 8 palette entries and 32 zero
        // indices, so `len == 4*8 + 32` holds and the area is a power of two.
        // `0xff` names no palette entry that could exist -- every candidate
        // split leaves indices of `255`, which `K` cannot cover -- so it is not
        // an image. The rule used to reject the zero payload too, because it
        // committed to the smallest valid `K` before asking whether the area
        // left over was a power of two, and for `K = 1` the leftover 60 bytes
        // are not one (`docs/DMA3D.md` §11.6).
        assert_eq!(palette_sizes(&[0x00; 64]), vec![8, 12, 14, 15]);
        assert_eq!(palette_size(&[0x00; 64]).expect("a zero payload admits a split"), 8);
        // The split exists and the size does not: 32 index bytes leave 4x8 or
        // 8x4, and neither 4 nor 8 on both axes is a width this engine
        // allocates. So the payload is admitted by the palette rule and refused
        // by the size rule, which is the same treatment the Italian disc's
        // three small textures get (`docs/DMA3D.md` §11.6).
        let err = parse(&[0x00; 64]).expect_err("4x8 is not an engine width");
        assert!(
            err.to_string().contains("width this engine can allocate"),
            "{err}"
        );

        let sizes = palette_sizes(&[0xff; 64]);
        assert!(sizes.is_empty(), "{sizes:?} is not a split");
        assert!(palette_size(&[0xff; 64]).is_err());
        assert!(parse(&[0xff; 64]).is_err());
    }

    /// A payload whose area is not a power of two is not a palette image, but it
    /// may still be a coverage mask: the negative belongs to the paletted reading
    /// and the router takes both (`docs/DMA3D.md` §11.14).
    #[test]
    fn a_63_byte_index_area_is_a_mask_not_a_palette_image() {
        let mut odd = Vec::new();
        for _ in 0..4 {
            odd.extend_from_slice(&[255, 0, 0, 255]);
        }
        odd.extend(std::iter::repeat(1u8).take(63));
        assert!(
            parse_paletted(&odd).is_err(),
            "a 63-byte index area was accepted as a palette image"
        );
        let t = parse(&odd).expect("the same bytes read as a coverage mask");
        assert_eq!(t.indices.len(), 64);
        assert!(t.palette.is_empty());
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
            // The 3D game ships a disc image, so the containers are carved out
            // of it by `lean/test/carve3d.py`; the carve's own folder is where
            // these tests look, and `DMA3D_ASSETS` still overrides it. These
            // used to look only at the two PC folders and so never ran at all.
            for dir in [
                std::env::var("DMA3D_ASSETS").unwrap_or_default(),
                "/workspace/.tmp/dma3d/assets".into(),
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
            // The container holds 781 resources and not all of them are
            // textures: 76 have an offsets entry naming a range the file does
            // not hold, and 8 admit no palette/index split at all. Those are
            // reported per-resource, not fatal -- the rule `AGENTS.md` states
            // and the one `docs/DMA3D.md` §11 measures (`docs/DMA3D.md` §11.6).
            assert_eq!(n, 705, "the count the table holds, which is the header's own");
            let mut decoded = 0usize;
            let mut masks = 0usize;
            let mut failed = 0usize;
            let mut square = 0usize;
            let mut wide = 0usize;
            let mut tall = 0usize;
            let mut axes = Vec::new();
            for i in 0..n {
                let t = match parse(c.payload(i)) {
                    Err(_) => {
                        failed += 1;
                        continue;
                    }
                    Ok(t) => t,
                };
                decoded += 1;
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
                // A paletted image's index must name an entry that exists; a
                // coverage mask has no palette at all, and its bytes are alpha
                // values rather than indices, so the assertion does not apply to
                // it (`docs/DMA3D.md` §11.14).
                if !t.palette.is_empty() {
                    assert!(
                        usize::from(largest) < t.palette.len(),
                        "{}: index {largest} names no palette entry",
                        c.name(i)
                    );
                } else {
                    masks += 1;
                }
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
                "{n} resources: {decoded} decode, {failed} reported; {square} square, {wide} wide, {tall} tall"
            );
            // These numbers are the Italian disc's, measured, and they are not
            // the English disc's: 70 square here against the 678 squares §4.2
            // was validated on. What matters is that all three readings occur
            // and that the counts are stable, not that this build's textures
            // happen to be square.
            assert_eq!(decoded, 705, "the number of textures that decode changed");
            assert_eq!(failed, 0, "the number of resources reported changed");
            assert_eq!(square + wide + tall, decoded);
            assert!(square > 0 && wide > 0 && tall > 0, "a reading never occurred");
            // 70 squares from the paletted reading, plus 8 of the 9 coverage masks
            // which are 16x16 (`docs/DMA3D.md` 11.14).
            assert_eq!(square, 78, "the square count changed");
        }

        /// The measured payload length must be `4*K + area` exactly, so a
        /// decoder cannot silently drop or invent bytes.
        #[test]
        fn the_payload_length_is_the_palette_plus_the_indices() {
            let Some(c) = textures() else {
                eprintln!("skipping: the 3D game's assets are not present");
                return;
            };
            let mut checked = 0usize;
            let mut failed = 0usize;
            let mut masks = 0usize;
            for i in 0..c.resource_count as usize {
                let payload = c.payload(i);
                // The same accounting: a payload the container does not hold,
                // or one that admits no split, is reported rather than made to
                // fit (`docs/DMA3D.md` §11.6).
                match parse(payload) {
                    Err(_) => failed += 1,
                    Ok(t) => {
                        // A paletted image's payload is `4*K + area` exactly; a
                        // coverage mask is a small header plus the area, and the
                        // header is the part that is not a palette
                        // (`docs/DMA3D.md` §11.14).
                        if t.palette.is_empty() {
                            masks += 1;
                            assert!(
                                t.indices.len() <= payload.len()
                                    && payload.len() - t.indices.len() < t.indices.len(),
                                "{}: the mask header is not smaller than the mask",
                                c.name(i)
                            );
                        } else {
                            assert_eq!(t.palette.len() * 4 + t.indices.len(), payload.len());
                        }
                        checked += 1;
                    }
                }
            }
            assert_eq!(checked, 705, "the number of textures that decode changed");
            assert_eq!(failed, 0, "the number of resources reported changed");
            assert_eq!(masks, 9, "the number of payloads read as coverage masks changed");
        }
    }
}
