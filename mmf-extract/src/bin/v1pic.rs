//! Report how each picture in a version-1 container is stored.
//!
//! `v1pic <container> [filter]` prints one line per resource: its index,
//! name, the container's height/width, the `kind` and `flags` metadata
//! bytes, the payload length, and which decoder path consumes it. This is the
//! tool that drove the version-1 picture reverse engineering, so the numbers
//! behind the claims in `docs/GAME1.md` are reproducible.
//!
//! The payload lengths are checked against every candidate layout
//! (`stride(w)*h` with and without a 1024-byte palette, `4*w*h`,
//! `4*stride(w)*h`) and against the two-section run-length sub-format, so a
//! resource is only reported as decoded when a decoder actually accepts it.

use mmf_extract::container::Container;
use mmf_extract::picture::{pixel_format, PALETTE_BYTES};
use mmf_extract::v1;

/// The layout detector, with the first game's palette bank supplied.
///
/// A run-length payload with one-byte colour records indexes into the bank
/// rather than carrying a palette of its own, so the detector cannot decide
/// the record width without it.
fn picture_pixel_format(
    c: &Container,
    i: usize,
    data: &[u8],
) -> Option<mmf_extract::picture::PixelFormat> {
    pixel_format(c, i, data, v1::palette_of(c, i))
}

fn stride_of(width: u16) -> usize {
    (width as usize + 3) / 4 * 4
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: v1pic <container> [name-filter]");
        return;
    }
    let c = match Container::open(&args[0]) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}: {e:#}", args[0]);
            return;
        }
    };
    if c.version != 1 {
        eprintln!("{}: version {}, not 1", args[0], c.version);
        return;
    }
    let filter = args.get(1).cloned().unwrap_or_default();

    let mut decoded = 0usize;
    let mut failed = 0usize;
    let mut by_layout: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for i in 0..c.resource_count as usize {
        let name = c.name(i);
        if !filter.is_empty() && !name.to_lowercase().contains(&filter.to_lowercase()) {
            continue;
        }
        let raw = c.payload(i);
        // The first game's own payload path: it expands the LZSS stream the
        // container declares, and copies through what is already stored plain.
        // Both counts are reported, because the gap between them is what says
        // a payload was compressed.
        let data = match mmf_extract::v1::picture::payload(&c, i) {
            Ok(d) => d,
            Err(e) => {
                println!("{i}\t{name}\texpand failed: {e:#}");
                failed += 1;
                continue;
            }
        };
        let stored = if mmf_extract::v1::uncompressed_len(&c, i) == 0 {
            "plain"
        } else {
            "lzss"
        };
        let dim = c.dimension(i);
        let (w, h) = (dim.width as usize, dim.height as usize);
        let st = stride_of(dim.width);
        let sizes: &[(&str, usize)] = &[
            ("pal+idx", PALETTE_BYTES + st * h),
            ("idx", st * h),
            ("pal+idx_nopad", PALETTE_BYTES + w * h),
            ("idx_nopad", w * h),
            ("pal+argb32", PALETTE_BYTES + 4 * st * h),
            ("argb32", 4 * w * h),
            ("argb32_pad", 4 * st * h),
            ("argb16", 2 * st * h),
        ];
        let layout = sizes
            .iter()
            .find(|(_, n)| *n == data.len())
            .map(|(l, _)| l.to_string())
            .unwrap_or_else(|| "-".to_string());
        let fmt = picture_pixel_format(&c, i, &data);
        let tag = match fmt {
            Some(f) => {
                decoded += 1;
                format!("{f:?}")
            }
            None => {
                failed += 1;
                "FAILED".to_string()
            }
        };
        *by_layout.entry(tag.clone()).or_insert(0) += 1;
        println!(
            "{i}\t{name}\t{h}x{w}\tkind=0x{:02x}\tflags=0x{:02x}\tstored={stored}\tstored_len={}\tlen={}\t{layout}\t{tag}",
            c.kind_of(i),
            c.flags_of(i),
            raw.len(),
            data.len()
        );
    }
    eprintln!("decoded {decoded}, failed {failed}; paths: {by_layout:?}");
}
