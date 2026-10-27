//! Count how often flags bit 0 (8bpp selector) disagrees with the payload
//! length classification, and cross-check the kind byte.
//!
//! `invariants <container>...` prints, for each container, the number of
//! resources where `flags&1` predicts a different bit depth than the payload
//! length does, broken down by class and flags value.

use mmf_extract::container::Container;
use mmf_extract::lz77;
use mmf_extract::picture::PALETTE_BYTES;

fn stride8(width: u16) -> usize {
    ((width as usize) + 3) / 4 * 4
}

fn class_of(c: &Container, i: usize) -> &'static str {
    let dim = c.dimension(i);
    let pixels = dim.width as usize * dim.height as usize;
    let p = c.payload(i);
    let data = if c.sizes[i] == 0 {
        p.to_vec()
    } else {
        match lz77::decompress(p) {
            Ok(d) => d,
            Err(_) => return "decompress",
        }
    };
    if data.len() == pixels * 4 {
        "c32"
    } else if data.len() == PALETTE_BYTES + stride8(dim.width) * dim.height as usize {
        "c8"
    } else if data.len() == stride8(dim.width) * dim.height as usize && pixels > 0 {
        "nopalette"
    } else {
        "odd"
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: invariants <container>...");
        return;
    }
    for path in &args {
        let c = match Container::open(path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("{path}: {e:#}");
                continue;
            }
        };
        let n = c.resource_count as usize;
        let mut mismatch8 = 0usize; // flags says 32bpp but payload is 8bpp
        let mut mismatch32 = 0usize; // flags says 8bpp but payload is 32bpp
        let mut kind1_nopal = 0usize; // kind==1 but payload has no palette prefix
        let mut kind0_pal = 0usize; // kind==0 but payload has a palette prefix
        let mut kind_ge2 = 0usize;
        let mut kind_hist = std::collections::BTreeMap::new();
        for i in 0..n {
            let cls = class_of(&c, i);
            let flags = c.flags_of(i);
            let kind = c.kind_of(i);
            *kind_hist.entry(kind).or_insert(0usize) += 1;
            let bit0 = flags & 1 == 1;
            let payload8 = cls == "c8" || cls == "nopalette";
            let payload32 = cls == "c32" || cls == "raw32";
            if payload8 && !bit0 {
                mismatch8 += 1;
            }
            if payload32 && bit0 {
                mismatch32 += 1;
            }
            match kind {
                1 => {
                    if cls != "c8" {
                        kind1_nopal += 1;
                    }
                }
                0 => {
                    if cls == "c8" {
                        kind0_pal += 1;
                    }
                }
                _ => kind_ge2 += 1,
            }
        }
        println!(
            "{}: N={n} flags-bit0-vs-length mismatch (says32/actual8)={mismatch8} (says8/actual32)={mismatch32} | kind==1-but-no-pal-prefix={kind1_nopal} kind==0-but-has-pal-prefix={kind0_pal} kind>=2={kind_ge2}",
            c.path.file_name().unwrap().to_string_lossy()
        );
        let hist: Vec<String> = kind_hist
            .iter()
            .map(|(k, v)| format!("{k}:{v}"))
            .collect();
        println!("  kind histogram: {}", hist.join(" "));
    }
}
