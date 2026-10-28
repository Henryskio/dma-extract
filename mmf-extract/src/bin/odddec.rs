//! Decode the odd picture sub-format and print its per-row run structure.
//!
//! Layout, measured on `HD.MMP`:
//!
//! ```text
//! [kind == 1] 1024 bytes   palette (skipped by the loader when kind is 1)
//! u16[N]                    offset table, big-endian, N = 2*height + 1
//!   entries[0]              byte size of this table
//! bytes                      run lengths and colours, addressed by the table
//! ```
//!
//! A row's run lengths are bytes that sum to the image width. Two candidate
//! layouts for the table are tried: halves (`entries[0..=h]` then
//! `entries[h..=2h]`) and interleaved (`entries[2i]`, `entries[2i+1]`,
//! `entries[2i+2]`).

use mmf_extract::container::Container;
use mmf_extract::lz77;
use mmf_extract::picture::PALETTE_BYTES;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: odddec <container> <index>");
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

    let mut found: Option<(usize, usize)> = None;
    for start in 0..body.len().saturating_sub(8) {
        let first = u16::from_be_bytes([body[start], body[start + 1]]) as usize;
        if first < 8 || first % 2 != 0 || start + first > body.len() {
            continue;
        }
        let n = first / 2;
        if n != 2 * h + 1 {
            continue;
        }
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
        println!("#{idx} {} {w}x{h}: no 2h+1 table found", c.name(idx));
        return;
    };
    let ts = n * 2;
    let entries: Vec<usize> = (0..n)
        .map(|k| {
            let p = start + 2 * k;
            u16::from_be_bytes([body[p], body[p + 1]]) as usize
        })
        .collect();
    let data = &body[start + ts..];
    println!(
        "#{idx} {} {w}x{h} kind={} flags={:02x} table@{start} entries={n} ts={ts} data={}",
        c.name(idx),
        c.kind_of(idx),
        c.flags_of(idx),
        data.len()
    );

    // Layout A: halves.
    let mut ok_a = 0usize;
    let mut bad_a: Vec<String> = Vec::new();
    for i in 0..h {
        let (la, lb) = (entries[i], entries[i + 1]);
        let (ca, cb) = (entries[h + i], entries[h + i + 1]);
        let lens = &data[la - ts..lb - ts];
        let cols = &data[ca - ts..cb - ts];
        let sum: u64 = lens.iter().map(|&x| x as u64).sum();
        if sum == w as u64 && lens.len() == cols.len() {
            ok_a += 1;
        } else if bad_a.len() < 8 {
            bad_a.push(format!(
                "row {i}: lens={:02x?} sum={sum} cols={:02x?}",
                lens, cols
            ));
        }
    }
    println!("layout A (halves): {ok_a}/{h} ok");
    for s in &bad_a {
        println!("   {s}");
    }

    // Layout B: interleaved per row.
    let mut ok_b = 0usize;
    let mut bad_b: Vec<String> = Vec::new();
    for i in 0..h {
        let la = entries[2 * i];
        let lb = entries[2 * i + 1];
        let cb = entries[2 * i + 2];
        let lens = &data[la - ts..lb - ts];
        let cols = &data[lb - ts..cb - ts];
        let sum: u64 = lens.iter().map(|&x| x as u64).sum();
        if sum == w as u64 && lens.len() == cols.len() {
            ok_b += 1;
        } else if bad_b.len() < 8 {
            bad_b.push(format!(
                "row {i}: lens={:02x?} sum={sum} cols={:02x?}",
                lens, cols
            ));
        }
    }
    println!("layout B (interleaved): {ok_b}/{h} ok");
    for s in &bad_b {
        println!("   {s}");
    }

    // Layout C: halves, but both halves hold run lengths (no colours).
    let mut ok_c = 0usize;
    for i in 0..h {
        let a: u64 = (entries[i]..entries[i + 1])
            .map(|k| body[start + k] as u64)
            .sum();
        let b: u64 = (entries[h + i]..entries[h + i + 1])
            .map(|k| body[start + k] as u64)
            .sum();
        if a == w as u64 && b == w as u64 {
            ok_c += 1;
        }
    }
    println!("layout C (both halves are run lengths): {ok_c}/{h} ok");
}
