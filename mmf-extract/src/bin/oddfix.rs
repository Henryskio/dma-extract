//! Test the foreground-run / colour-record correspondence.
//!
//! `oddfix <container> <index>` checks, for every row, whether the number of
//! 4-byte colour records in the second section's slice equals the sum of the
//! foreground run lengths in the first section's slice — under both
//! parities (runs starting foreground or background).

use mmf_extract::container::Container;
use mmf_extract::lz77;
use mmf_extract::picture::PALETTE_BYTES;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: oddfix <container> <index>");
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

    let mut found = None;
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
        if ok && prev <= body.len() - start {
            found = Some((start, n));
            break;
        }
    }
    let Some((start, n)) = found else {
        println!("no table");
        return;
    };
    let ts = n * 2;
    let entries: Vec<usize> = (0..n)
        .map(|k| {
            let p = start + 2 * k;
            u16::from_be_bytes([body[p], body[p + 1]]) as usize
        })
        .collect();
    let slice = |i: usize| -> Vec<u8> { body[start + entries[i]..start + entries[i + 1]].to_vec() };

    // Odd-indexed runs are foreground (runs start with background).
    let mut odd_fg = 0usize;
    let mut even_fg = 0usize;
    let mut bad_odd: Vec<String> = Vec::new();
    let mut bad_even: Vec<String> = Vec::new();
    let mut fg_pixels = 0usize;
    let mut total_records = 0usize;

    for i in 0..h {
        let runs = slice(i);
        let recs = slice(h + i);
        let nrec = if c.kind_of(idx) == 1 { recs.len() } else { recs.len() / 4 };
        let odd_sum: usize = runs.iter().step_by(2).map(|&x| x as usize).sum();
        let even_sum: usize = runs.iter().skip(1).step_by(2).map(|&x| x as usize).sum();
        fg_pixels += odd_sum;
        total_records += nrec;
        if odd_sum == nrec {
            odd_fg += 1;
        } else if bad_odd.len() < 6 {
            bad_odd.push(format!("row {i}: runs={runs:02x?} oddsum={odd_sum} nrec={nrec}"));
        }
        if even_sum == nrec {
            even_fg += 1;
        } else if bad_even.len() < 6 {
            bad_even.push(format!("row {i}: runs={runs:02x?} evensum={even_sum} nrec={nrec}"));
        }
    }

    println!(
        "#{idx} {} {w}x{h} kind={}",
        c.name(idx),
        c.kind_of(idx)
    );
    println!("rows where sum(odd runs) == ncols/4 : {odd_fg}/{h}");
    println!("rows where sum(even runs) == ncols/4: {even_fg}/{h}");
    println!("total foreground pixels (odd runs): {fg_pixels}");
    println!("total colour records: {total_records}");
    if !bad_odd.is_empty() {
        println!("first mismatches (odd):");
        for s in &bad_odd {
            println!("  {s}");
        }
    }
    if !bad_even.is_empty() {
        println!("first mismatches (even):");
        for s in &bad_even {
            println!("  {s}");
        }
    }

    // If the odd-run reading holds, dump the first rows fully decoded.
    if odd_fg * 2 > h {
        println!("first 6 rows decoded (run, colour):");
        for i in 0..6.min(h) {
            let runs = slice(i);
            let recs = slice(h + i);
            let mut out = String::new();
            let mut p = 0usize;
            for (j, &r) in runs.iter().enumerate() {
                if j % 2 == 1 {
                    // foreground: take `r` colour records
                    let take = r as usize;
                    let mut cols = Vec::new();
                    for _ in 0..take {
                        if p + 4 <= recs.len() {
                            cols.push(format!(
                                "{:02x}{:02x}{:02x}{:02x}",
                                recs[p], recs[p + 1], recs[p + 2], recs[p + 3]
                            ));
                            p += 4;
                        }
                    }
                    out.push_str(&format!(" fg({r})={}", cols.join(",")));
                } else {
                    out.push_str(&format!(" bg({r})"));
                }
            }
            println!("  row {i}:{out}");
        }
    }
}
