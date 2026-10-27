//! Cross-tabulate payload class against the three per-resource byte columns.
//!
//! `cols <container>...` prints a histogram of
//! `class x byteA(4N) x byteB(5N) x byteC(14N+2)` plus the file's palette-bank
//! count, so the runtime flags/kind columns can be identified by their
//! invariants (flags&1 == 1 exactly for 8bpp; kind == 1 exactly for
//! resources carrying their own palette).

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
        eprintln!("usage: cols <container>...");
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
        // Palette bank: a u16 count right after the 15*N+4 metadata region.
        let bank_at = c.names_end + 15 * n + 2;
        let bank = if bank_at + 2 <= c.data.len() {
            u16::from_le_bytes([c.data[bank_at], c.data[bank_at + 1]])
        } else {
            0
        };
        let bank_bytes = bank as usize * PALETTE_BYTES;
        println!(
            "# {} N={n} names_end={} bank_count={} bank_bytes={} file_len={} slack={}",
            c.path.file_name().unwrap().to_string_lossy(),
            c.names_end,
            bank,
            bank_bytes,
            c.data.len(),
            c.data.len() as isize - (bank_at as isize + 2 + bank_bytes as isize)
        );

        let mut hist: std::collections::BTreeMap<(String, u8, u8, u8), usize> =
            std::collections::BTreeMap::new();
        for i in 0..n {
            let cls = class_of(&c, i);
            *hist
                .entry((
                    cls.to_string(),
                    c.attr_of(i),
                    c.flags_of(i),
                    c.kind_of(i),
                ))
                .or_default() += 1;
        }
        for ((cls, a, b, cc), count) in &hist {
            println!("{cls}\tattr=0x{a:02x}\tflags=0x{b:02x}\tkind=0x{cc:02x}\t{count}");
        }
    }
}
