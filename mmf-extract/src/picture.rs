//! MMFW picture resources: decode to PNG and dump the raw payload.
//!
//! A picture resource decompresses to one of two layouts.
//!
//! **32 bits per pixel.** The payload is a flat array of pixels, four bytes
//! each, in the order the game stored them: **transparency, red, green,
//! blue**. The first byte is an inverted alpha (0 = fully opaque, 255 =
//! fully transparent), and the colour bytes are straight, non-premultiplied
//! values. `ITXTmustard24` decodes to R=255, G=239, B=90 — mustard yellow —
//! and `ITXTcarrot24` to orange flesh with green leaves.
//!
//! **8 bits per pixel.** Two layouts share the same one-byte-per-pixel
//! payload size. If the payload starts with a 1024-byte palette (256 entries
//! of four bytes each, in the same transparency/red/green/blue order), the
//! bytes after it are palette indices. If there is no palette, the payload is
//! an **8-bit coverage mask**: each byte is an alpha value (0 = transparent,
//! 255 = fully opaque) for a shape the game tints at runtime. Rows are padded
//! to a multiple of four bytes in both cases.
//!
//! The container's picture dimension table gives each resource's width and
//! height, so the expected pixel count is known up front.

use anyhow::{bail, Context, Result};
use crate::container::{Container, ContainerKind};
use crate::lz77;
use std::path::Path;

/// Number of bytes in a 256-entry palette.
pub const PALETTE_BYTES: usize = 1024;

/// How a picture resource's pixels are stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    /// 32 bits per pixel: transparency (inverted alpha), red, green, blue.
    TranspRgb32,
    /// 8 bits per pixel, with a 1024-byte palette prefix.
    Paletted8,
    /// 8 bits per pixel without a palette: each byte is alpha coverage.
    AlphaMask8,
}

/// Convert one stored 32-bit pixel into straight RGBA.
///
/// Stored as transparency, red, green, blue; the transparency byte is the
/// inverse of alpha.
fn to_rgba(word: [u8; 4]) -> [u8; 4] {
    [word[1], word[2], word[3], 255 - word[0]]
}

/// Expand an 8bpp coverage mask to straight RGBA.
///
/// Each byte is alpha; the colour is plain white so the mask can be tinted
/// when composited. Index 0 is fully transparent.
fn mask_rgba(indices: &[u8], width: u16, height: u16) -> Vec<u8> {
    let stride = stride8(width);
    let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
    for y in 0..height as usize {
        let start = y * stride;
        for x in 0..width as usize {
            let a = indices.get(start + x).copied().unwrap_or(0);
            rgba.extend_from_slice(&[255, 255, 255, a]);
        }
    }
    rgba
}

/// Expand an 8bpp indexed image to straight RGBA.
///
/// `palette` is 1024 bytes (256 entries of transparency, red, green, blue),
/// `indices` the row-padded index rows. Rows shorter than the table says are
/// padded with transparent pixels rather than failing the resource.
pub(crate) fn indexed_rgba(palette: &[u8], indices: &[u8], width: u16, height: u16) -> Vec<u8> {
    let stride = stride8(width);
    let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
    for y in 0..height as usize {
        let start = y * stride;
        if start >= indices.len() {
            for _ in 0..width as usize {
                rgba.extend_from_slice(&[0, 0, 0, 0]);
            }
            continue;
        }
        let end = (start + width as usize).min(indices.len());
        let row = &indices[start..end];
        for &idx in row {
            let e = &palette[idx as usize * 4..idx as usize * 4 + 4];
            rgba.extend_from_slice(&to_rgba([e[0], e[1], e[2], e[3]]));
        }
        for _ in row.len()..width as usize {
            rgba.extend_from_slice(&[0, 0, 0, 0]);
        }
    }
    rgba
}

/// Row stride of an 8bpp image: each row is padded to a multiple of four.
pub(crate) fn stride8(width: u16) -> usize {
    ((width as usize) + 3) / 4 * 4
}

/// Write a decoded picture to a PNG file.
pub fn write_png(path: &Path, width: u16, height: u16, rgba: &[u8]) -> Result<()> {
    let expected = width as usize * height as usize * 4;
    if rgba.len() < expected {
        bail!(
            "{}: picture has {} bytes but {}x{} needs {}",
            path.display(),
            rgba.len(),
            width,
            height,
            expected
        );
    }

    let file = std::fs::File::create(path)
        .with_context(|| format!("creating {}", path.display()))?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), width as u32, height as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header()?;
    writer.write_image_data(&rgba[..expected])?;
    Ok(())
}

/// The decoded pixels of one picture resource.
pub struct Decoded {
    pub width: u16,
    pub height: u16,
    pub format: PixelFormat,
    /// Straight RGBA, four bytes per pixel.
    pub rgba: Vec<u8>,
}

/// Decode one picture resource into RGBA pixels.
///
/// The bit depth is taken from the container's flags byte (bit 0 set means
/// 8 bits per pixel), and cross-checked against the payload length.
pub fn decode(container: &Container, index: usize) -> Result<Decoded> {
    let raw = container.payload(index);
    // A zero in the container's decompressed-size table means the payload was
    // stored uncompressed, so it must not go through the LZ77 decoder.
    let data = if container.sizes[index] == 0 {
        raw.to_vec()
    } else {
        lz77::decompress(raw).with_context(|| {
            format!(
                "decompressing resource {index} ('{}')",
                container.name(index)
            )
        })?
    };

    let dim = container.dimension(index);
    let (width, height) = (dim.width, dim.height);
    let pixels = width as usize * height as usize;

    // Decide the storage layout from the payload length, which is
    // unambiguous; the flags byte is only a fallback.
    let as32 = pixels * 4;
    let as8 = PALETTE_BYTES + stride8(width) * height as usize;
    let bare8 = stride8(width) * height as usize;

    let format = if data.len() == as32 {
        PixelFormat::TranspRgb32
    } else if data.len() == as8 {
        PixelFormat::Paletted8
    } else if pixels > 0 && data.len() == bare8 {
        // No palette: the bytes are an alpha coverage mask.
        PixelFormat::AlphaMask8
    } else {
        bail!(
            "resource {index} ('{}'): {} bytes match neither the 32bpp ({as32}) nor the 8bpp ({as8}, bare {bare8}) layout of a {width}x{height} picture",
            container.name(index),
            data.len()
        )
    };

    let rgba = match format {
        PixelFormat::TranspRgb32 => {
            if data.len() < as32 {
                bail!(
                    "resource {index} ('{}'): {} bytes for a {width}x{height} 32bpp picture, need {as32}",
                    container.name(index),
                    data.len()
                );
            }
            let mut rgba = Vec::with_capacity(as32);
            for chunk in data[..as32].chunks_exact(4) {
                rgba.extend_from_slice(&to_rgba([chunk[0], chunk[1], chunk[2], chunk[3]]));
            }
            rgba
        }
        PixelFormat::Paletted8 => indexed_rgba(&data[..PALETTE_BYTES], &data[PALETTE_BYTES..], width, height),
        PixelFormat::AlphaMask8 => mask_rgba(&data, width, height),
    };

    Ok(Decoded {
        width,
        height,
        format,
        rgba,
    })
}

/// Extract every picture from a container.
///
/// `out_dir` receives one PNG per resource. `raw_dir` receives the payload
/// exactly as stored in the container, untouched.
pub fn extract_container(
    container_path: &Path,
    out_dir: &Path,
    raw_dir: &Path,
) -> Result<ExtractReport> {
    let container = Container::open(container_path)?;
    std::fs::create_dir_all(out_dir)?;
    std::fs::create_dir_all(raw_dir)?;

    let stem = container_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("resource");

    let mut report = ExtractReport::default();
    report.container = container_path.display().to_string();

    for index in 0..container.resource_count as usize {
        let name = container.name(index);
        let safe = sanitize(name, index);

        // The untouched payload, byte for byte as it appears in the container.
        let raw_path = raw_dir.join(format!("{stem}_{index:05}_{safe}.bin"));
        std::fs::write(&raw_path, container.payload(index))
            .with_context(|| format!("writing {}", raw_path.display()))?;
        report.raw_written += 1;

        let decoded = match decode(&container, index) {
            Ok(d) => d,
            Err(e) => {
                report.failed.push((index, name.to_string(), e.to_string()));
                continue;
            }
        };

        if decoded.width == 0 || decoded.height == 0 {
            report.skipped.push((index, name.to_string(), "zero dimension".into()));
            continue;
        }

        let png_path = out_dir.join(format!("{stem}_{index:05}_{safe}.png"));
        match write_png(&png_path, decoded.width, decoded.height, &decoded.rgba) {
            Ok(()) => {
                report.png_written += 1;
                report.resources.push(ResourceEntry {
                    index,
                    name: name.to_string(),
                    width: decoded.width,
                    height: decoded.height,
                    bytes: decoded.rgba.len(),
                    format: decoded.format,
                    png: png_path.display().to_string(),
                    raw: raw_path.display().to_string(),
                });
            }
            Err(e) => {
                report.failed.push((index, name.to_string(), e.to_string()));
            }
        }
    }

    Ok(report)
}

/// What happened while extracting one container.
#[derive(Debug, Default)]
pub struct ExtractReport {
    pub container: String,
    pub png_written: usize,
    pub raw_written: usize,
    pub skipped: Vec<(usize, String, String)>,
    pub failed: Vec<(usize, String, String)>,
    pub resources: Vec<ResourceEntry>,
}

/// One extracted picture.
#[derive(Debug, Clone)]
pub struct ResourceEntry {
    pub index: usize,
    pub name: String,
    pub width: u16,
    pub height: u16,
    pub bytes: usize,
    pub format: PixelFormat,
    pub png: String,
    pub raw: String,
}

/// Turn a resource name into something safe to put in a filename.
fn sanitize(name: &str, index: usize) -> String {
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
        format!("res{index:05}")
    } else {
        cleaned
    }
}

/// Convenience: fail if the container has no pictures at all.
pub fn ensure_pictures(container: &Container) -> Result<()> {
    if container.kind != ContainerKind::Pictures {
        bail!(
            "{}: expected a Pictures container, found {:?}",
            container.path.display(),
            container.kind
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The stored 32-bit word is transparency, red, green, blue; alpha is the
    /// inverse of the transparency byte. This is the regression test for the
    /// bug that forced blue to zero on every opaque pixel.
    #[test]
    fn pixel_order_is_transparency_rgb() {
        // Mustard: the dominant opaque word in ITXTmustard24.
        assert_eq!(to_rgba([0, 255, 239, 90]), [255, 239, 90, 255]);
        // The transparent background word.
        assert_eq!(to_rgba([255, 0, 0, 0]), [0, 0, 0, 0]);
        // A half-transparent pixel: alpha = 255 - 128.
        assert_eq!(to_rgba([128, 10, 20, 30]), [10, 20, 30, 127]);
        // Pure primary channels stay in place: red.
        assert_eq!(to_rgba([0, 255, 0, 0]), [255, 0, 0, 255]);
        // Pure blue must actually reach the blue channel.
        assert_eq!(to_rgba([0, 0, 0, 255]), [0, 0, 255, 255]);
    }

    /// A bare 8bpp payload is alpha coverage, rendered as white so it can be
    /// tinted. The byte becomes alpha unchanged.
    #[test]
    fn mask_is_white_with_byte_as_alpha() {
        let indices = [0u8, 1, 128, 255];
        let rgba = mask_rgba(&indices, 4, 1);
        assert_eq!(
            rgba,
            vec![
                255, 255, 255, 0, //
                255, 255, 255, 1, //
                255, 255, 255, 128, //
                255, 255, 255, 255,
            ]
        );
    }

    /// Palette entries use the same transparency/R/G/B order as pixels.
    /// Entry values from the fire stamp `IStriFIRa01`: entry 0 is the
    /// transparent background, entry 1 is orange.
    #[test]
    fn palette_entries_use_the_same_order_as_pixels() {
        let mut palette = vec![0u8; PALETTE_BYTES];
        palette[0..4].copy_from_slice(&[255, 0, 0, 0]); // transparent
        palette[4..8].copy_from_slice(&[0, 254, 151, 4]); // orange

        let rgba = indexed_rgba(&palette, &[0u8, 1, 1], 3, 1);
        assert_eq!(&rgba[0..4], &[0, 0, 0, 0]);
        assert_eq!(&rgba[4..8], &[254, 151, 4, 255]);
        assert_eq!(&rgba[8..12], &[254, 151, 4, 255]);
    }
}
