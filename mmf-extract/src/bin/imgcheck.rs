//! Corpus-level colour sanity check.
//!
//! `imgcheck <dir>` walks every `.png` under `dir`, decodes it, and reports:
//!   * how many images and pixels were read,
//!   * the fraction of opaque pixels whose red / green / blue channel is 0,
//!   * the fraction of opaque pixels that are pure white / pure black,
//!   * the ten most common opaque colours across the whole corpus.
//!
//! This is the programmatic guard against the old channel-order bug, which
//! forced blue to 0 on every opaque pixel and tinted the corpus yellow.

use png::Decoder;
use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, out);
        } else if p.extension().and_then(|e| e.to_str()) == Some("png") {
            out.push(p);
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: imgcheck <dir>");
        return;
    }
    let mut files = Vec::new();
    walk(Path::new(&args[0]), &mut files);
    files.sort();
    eprintln!("found {} png files", files.len());

    let mut images = 0usize;
    let mut pixels = 0u64;
    let mut opaque = 0u64;
    let mut r0 = 0u64;
    let mut g0 = 0u64;
    let mut b0 = 0u64;
    let mut white = 0u64;
    let mut black = 0u64;
    let mut hist: HashMap<[u8; 3], u64> = HashMap::new();

    for path in &files {
        let Ok(file) = File::open(path) else { continue };
        let mut reader = match Decoder::new(std::io::BufReader::new(file)).read_info() {
            Ok(r) => r,
            Err(_) => continue,
        };
        let (w, h) = {
            let i = reader.info();
            (i.width, i.height)
        };
        let mut buf = vec![0u8; (w as usize * h as usize * 4) + 4096];
        let Ok(info) = reader.next_frame(&mut buf) else {
            continue;
        };
        images += 1;
        let data = &buf[..info.buffer_size()];
        for px in data.chunks_exact(4) {
            pixels += 1;
            if px[3] < 128 {
                continue;
            }
            opaque += 1;
            if px[0] == 0 {
                r0 += 1;
            }
            if px[1] == 0 {
                g0 += 1;
            }
            if px[2] == 0 {
                b0 += 1;
            }
            if px[0] == 255 && px[1] == 255 && px[2] == 255 {
                white += 1;
            }
            if px[0] == 0 && px[1] == 0 && px[2] == 0 {
                black += 1;
            }
            *hist.entry([px[0], px[1], px[2]]).or_default() += 1;
        }
    }

    let pct = |n: u64| {
        if opaque == 0 {
            0.0
        } else {
            n as f64 * 100.0 / opaque as f64
        }
    };
    println!("images={images} pixels={pixels} opaque(alpha>=128)={opaque}");
    println!(
        "opaque channel-is-zero: R={:.2}% G={:.2}% B={:.2}%",
        pct(r0),
        pct(g0),
        pct(b0)
    );
    println!("pure white={:.2}% pure black={:.2}%", pct(white), pct(black));
    let mut top: Vec<(&[u8; 3], &u64)> = hist.iter().collect();
    top.sort_by(|a, b| b.1.cmp(a.1));
    print!("top opaque colours:");
    for (c, n) in top.iter().take(10) {
        print!(" {}x{}x{}:{}", c[0], c[1], c[2], n);
    }
    println!();
}
