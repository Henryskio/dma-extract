//! Scan a picture container for 8bpp images whose palettes contain vivid
//! colours, and report the colours actually used by the image.
//!
//! `palscan <container>` prints, for each 8bpp+palette resource, the resource
//! index, name, the most-used opaque palette index and its entry, plus the
//! most saturated entry in the palette. Values are printed as the stored
//! bytes `[t, b1, b2, b3]` so either channel order can be read off.

use mmf_extract::container::Container;
use mmf_extract::lz77;
use mmf_extract::picture::PALETTE_BYTES;

fn stride8(width: u16) -> usize {
    ((width as usize) + 3) / 4 * 4
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: palscan <container>");
        return;
    }
    let c = match Container::open(&args[0]) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}: {e:#}", args[0]);
            return;
        }
    };
    let n = c.resource_count as usize;
    for i in 0..n {
        let dim = c.dimension(i);
        if dim.width == 0 || dim.height == 0 {
            continue;
        }
        let p = c.payload(i);
        let data = if c.sizes[i] == 0 {
            p.to_vec()
        } else {
            match lz77::decompress(p) {
                Ok(d) => d,
                Err(_) => continue,
            }
        };
        let need = PALETTE_BYTES + stride8(dim.width) * dim.height as usize;
        if data.len() != need {
            continue;
        }
        let pal = &data[..PALETTE_BYTES];
        let idx = &data[PALETTE_BYTES..];

        // How often each palette index is used.
        let mut hist = [0usize; 256];
        for &b in idx {
            hist[b as usize] += 1;
        }
        let mut order: Vec<(usize, usize)> = hist.iter().copied().enumerate().collect();
        order.sort_by(|a, b| b.1.cmp(&a.1));
        // Most-used non-zero index (index 0 is the transparent background).
        let (top_i, top_n) = order
            .iter()
            .copied()
            .find(|&(k, _)| k != 0)
            .unwrap_or((0, 0));

        // Most saturated palette entry.
        let mut best = (0usize, 0u8);
        for e in 0..256 {
            let q = &pal[e * 4 + 1..e * 4 + 4];
            let max = *q.iter().max().unwrap();
            let min = *q.iter().min().unwrap();
            let sat = max - min;
            if sat > best.1 {
                best = (e, sat);
            }
        }

        let te = &pal[top_i * 4..top_i * 4 + 4];
        let se = &pal[best.0 * 4..best.0 * 4 + 4];
        println!(
            "{i}\t{}\t{}x{}\ttopidx={top_i}({top_n})\t[{:02x} {:02x} {:02x} {:02x}]\tsat={}({}) [{:02x} {:02x} {:02x} {:02x}]",
            c.name(i),
            dim.width,
            dim.height,
            te[0], te[1], te[2], te[3],
            best.1,
            best.0,
            se[0], se[1], se[2], se[3]
        );
    }
}
