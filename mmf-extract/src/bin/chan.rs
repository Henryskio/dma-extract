//! Diagnose the channel order of 32bpp pixels and 8bpp palettes.
//!
//! `chan <container> <index>` decompresses one resource and prints:
//!   * for 32bpp: the most common 4-byte words and, per byte position, the
//!     fraction of values that are 0 / 255 / other;
//!   * for 8bpp: the first 16 palette entries and, per palette byte position,
//!     the same fractions;
//!   * for bare 8bpp: the index histogram.
//!
//! The alpha byte should look like an alpha channel (mostly 0 or 255, never
//! in between for hard sprites); the colour bytes should be spread out.

use mmf_extract::container::Container;
use mmf_extract::lz77;
use mmf_extract::picture::PALETTE_BYTES;
use std::collections::HashMap;

fn stride8(width: u16) -> usize {
    ((width as usize) + 3) / 4 * 4
}

fn byte_stats(data: &[u8], stride: usize) -> String {
    let n = data.len() / stride;
    if n == 0 {
        return "empty".into();
    }
    let mut out = String::new();
    for pos in 0..stride {
        let mut zero = 0usize;
        let mut full = 0usize;
        let mut other = 0usize;
        for i in 0..n {
            let b = data[i * stride + pos];
            if b == 0 {
                zero += 1;
            } else if b == 255 {
                full += 1;
            } else {
                other += 1;
            }
        }
        out.push_str(&format!(
            "  byte{pos}: 0={:5.1}% 255={:5.1}% other={:5.1}%\n",
            zero as f64 * 100.0 / n as f64,
            full as f64 * 100.0 / n as f64,
            other as f64 * 100.0 / n as f64,
        ));
    }
    out
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: chan <container> <index> [<index>...]");
        return;
    }
    let c = match Container::open(&args[0]) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}: {e:#}", args[0]);
            return;
        }
    };
    for a in &args[1..] {
        let i: usize = a.parse().unwrap();
        let dim = c.dimension(i);
        let (w, h) = (dim.width, dim.height);
        let pixels = w as usize * h as usize;
        let data = if c.sizes[i] == 0 {
            c.payload(i).to_vec()
        } else {
            match lz77::decompress(c.payload(i)) {
                Ok(d) => d,
                Err(e) => {
                    println!("#{i}: decompress error {e:#}");
                    continue;
                }
            }
        };
        println!(
            "=== #{i} '{}' {w}x{h} len={} kind={} flags=0x{:02x} attr=0x{:02x}",
            c.name(i),
            data.len(),
            c.kind_of(i),
            c.flags_of(i),
            c.attr_of(i)
        );

        if data.len() == pixels * 4 {
            // 32bpp
            let mut hist: HashMap<[u8; 4], usize> = HashMap::new();
            for px in data.chunks_exact(4) {
                *hist.entry([px[0], px[1], px[2], px[3]]).or_default() += 1;
            }
            let mut v: Vec<(&[u8; 4], &usize)> = hist.iter().collect();
            v.sort_by(|a, b| b.1.cmp(a.1));
            println!("32bpp, distinct words={}", hist.len());
            print!("top words:");
            for (w4, n) in v.iter().take(8) {
                print!(" {:02x}{:02x}{:02x}{:02x}:{}", w4[0], w4[1], w4[2], w4[3], n);
            }
            println!();
            print!("{}", byte_stats(&data, 4));
        } else if data.len() == PALETTE_BYTES + stride8(w) * h as usize {
            let pal = &data[..PALETTE_BYTES];
            println!("8bpp + palette, first 16 entries:");
            for e in 0..16 {
                let p = &pal[e * 4..e * 4 + 4];
                print!("  [{e:3}] {:02x} {:02x} {:02x} {:02x}", p[0], p[1], p[2], p[3]);
                println!();
            }
            println!("palette byte stats:");
            print!("{}", byte_stats(pal, 4));
            let idx = &data[PALETTE_BYTES..];
            let mut ih = [0usize; 256];
            for &b in idx {
                ih[b as usize] += 1;
            }
            let mut top: Vec<(usize, usize)> = ih.iter().copied().enumerate().collect();
            top.sort_by(|a, b| b.1.cmp(&a.1));
            print!("index top:");
            for (k, n) in top.iter().take(10) {
                print!(" {k}:{n}");
            }
            println!();
            println!("top index palette entries (t,byte1,byte2,byte3):");
            for (k, n) in top.iter().take(10) {
                let e = &pal[k * 4..k * 4 + 4];
                println!(
                    "  idx {k:3} n={n:5}  {:02x} {:02x} {:02x} {:02x}",
                    e[0], e[1], e[2], e[3]
                );
            }
        } else if data.len() == stride8(w) * h as usize {
            println!("bare 8bpp (no palette), index top:");
            let mut ih = [0usize; 256];
            for &b in &data {
                ih[b as usize] += 1;
            }
            let mut top: Vec<(usize, usize)> = ih.iter().copied().enumerate().collect();
            top.sort_by(|a, b| b.1.cmp(&a.1));
            for (k, n) in top.iter().take(10) {
                print!(" {k}:{n}");
            }
            println!();
        } else {
            println!("odd length");
        }
    }
}
