//! Dump every slice an odd-picture offset table describes.
//!
//! `oddslices <container> <index>` prints, for each consecutive pair of
//! table entries, the byte slice and its sum, so the table's real structure
//! can be read off without assuming a layout.

use mmf_extract::container::Container;
use mmf_extract::lz77;
use mmf_extract::picture::PALETTE_BYTES;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: oddslices <container> <index>");
        return;
    }
    let idx: usize = args[1].parse().expect("index");
    let c = match Container::open(&args[0]) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}: {e:#}", args[0]);
            return;
        }
    };
    let dim = c.dimension(idx);
    let plain = if c.sizes[idx] == 0 {
        c.payload(idx).to_vec()
    } else {
        lz77::decompress(c.payload(idx)).expect("lz77")
    };
    let w = dim.width as usize;
    let h = dim.height as usize;
    let base = if c.kind_of(idx) == 1 { PALETTE_BYTES } else { 0 };
    let body = &plain[base..];

    // Find the table.
    let mut found: Option<(usize, usize)> = None;
    for start in 0..body.len().saturating_sub(8) {
        let first = u16::from_be_bytes([body[start], body[start + 1]]) as usize;
        if first < 8 || first % 2 != 0 || start + first > body.len() {
            continue;
        }
        let n = first / 2;
        let mut prev = 0usize;
        let mut ok = true;
        for k in 0..n {
            let p = start + 2 * k;
            let v = u16::from_be_bytes([body[p], body[p + 1]]) as usize;
            if v < prev {
                ok = false;
                break;
            }
            prev = v;
        }
        if ok && prev <= body.len() && prev >= start + first {
            found = Some((start, n));
            break;
        }
    }
    let Some((start, n)) = found else {
        println!("#{idx}: no table");
        return;
    };
    let ts = n * 2;
    let entries: Vec<usize> = (0..n)
        .map(|k| {
            let p = start + 2 * k;
            u16::from_be_bytes([body[p], body[p + 1]]) as usize
        })
        .collect();
    println!(
        "#{idx} {} {w}x{h} kind={} flags={:02x} attr={:02x} entries={n} ts={ts}",
        c.name(idx),
        c.kind_of(idx),
        c.flags_of(idx),
        c.attr_of(idx)
    );
    println!("width={w} height={h} 2h+1={}", 2 * h + 1);
    for k in 0..n - 1 {
        let (a, b) = (entries[k], entries[k + 1]);
        let slice = &body[start + a..start + b];
        let sum: u64 = slice.iter().map(|&x| x as u64).sum();
        println!(
            "  [{k:3}] len={:3} sum={sum:5} {:02x?}",
            slice.len(),
            slice
        );
    }
}
