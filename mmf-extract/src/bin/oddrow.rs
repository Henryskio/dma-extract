//! Split an odd picture payload into its u16 offset table and row data.
//!
//! `oddrow <container> <index>` prints, per row, the bytes the offset table
//! assigns to it and their sum, so the per-row encoding can be read off
//! directly. The table is located by scanning for the monotonic u16 ramp
//! whose first entry equals its own byte size and whose last entry equals
//! the size of the region that follows it.

use mmf_extract::container::Container;
use mmf_extract::lz77;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: oddrow <container> <index>");
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

    // Locate the u16 table: the first run of >= 4 non-decreasing u16 values
    // whose first value is twice the number of entries and whose last value
    // is <= the payload length.
    let mut best: Option<(usize, usize, usize)> = None;
    for start in 0..plain.len().saturating_sub(8) {
        let i = start;
        let first = u16::from_be_bytes([plain[i], plain[i + 1]]) as usize;
        if first < 8 || first % 2 != 0 || start + first > plain.len() {
            continue;
        }
        let n = first / 2;
        let mut ok = true;
        let mut prev = 0usize;
        for k in 0..n {
            let p = start + 2 * k;
            if p + 1 >= plain.len() {
                ok = false;
                break;
            }
            let v = u16::from_be_bytes([plain[p], plain[p + 1]]) as usize;
            if v < prev {
                ok = false;
                break;
            }
            prev = v;
        }
        if !ok {
            continue;
        }
        let last = prev;
        if last <= plain.len() && last >= start + first {
            best = Some((start, n, last));
            break;
        }
    }

    let Some((start, n, last)) = best else {
        println!("no u16 table found");
        return;
    };
    println!(
        "#{idx} {} {}x{} flags={:02x} attr={:02x} kind={} plain={}",
        c.name(idx),
        dim.width,
        dim.height,
        c.flags_of(idx),
        c.attr_of(idx),
        c.kind_of(idx),
        plain.len()
    );
    println!(
        "table at payload[{start}] entries={n} bytesize={} last={last} region={}",
        n * 2,
        plain.len() - start
    );

    let data = &plain[start + n * 2..];
    println!("row data bytes={}", data.len());
    let w = dim.width as usize;
    let h = dim.height as usize;
    println!("width={w} height={h}");

    let mut entries = Vec::with_capacity(n);
    for k in 0..n {
        let p = start + 2 * k;
        entries.push(u16::from_be_bytes([plain[p], plain[p + 1]]) as usize);
    }
    // Print the first and last few entries to check alignment.
    println!("entries[0..6]={:?}", &entries[..n.min(6)]);
    println!("entries[{}..{}]:", n.saturating_sub(4), n);

    let mut sum_ok = 0usize;
    let mut sum_bad = 0usize;
    let mut bad = Vec::new();
    for k in 0..n - 1 {
        let (a, b) = (entries[k], entries[k + 1]);
        if b > data.len() + n * 2 || a > data.len() {
            continue;
        }
        let row = &data[a - n * 2..b - n * 2];
        let s: u64 = row.iter().map(|&x| x as u64).sum();
        if k < h {
            if s == w as u64 {
                sum_ok += 1;
            } else {
                sum_bad += 1;
                if bad.len() < 12 {
                    bad.push((k, s, row.to_vec()));
                }
            }
        }
    }
    println!("rows whose bytes sum to width: {sum_ok} / {}", sum_bad + sum_ok);
    for (k, s, row) in &bad {
        println!("  row {k}: sum={s} (want {w}) bytes={:02x?}", row);
    }
}
