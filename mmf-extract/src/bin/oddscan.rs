//! Non-circular scan for the u16 table in an odd picture payload.
//!
//! `oddscan <container>` looks for every big-endian u16 run whose first value
//! equals twice its own length (`entries[0] == 2*n`) without assuming any
//! relationship to the resource's dimensions, then reports what `n` is in
//! terms of width and height, and tests the resulting row sums against both
//! dimensions. It also names the rows whose byte sum matches neither.

use mmf_extract::container::Container;
use mmf_extract::lz77;
use mmf_extract::picture::PALETTE_BYTES;

fn stride8(width: u16) -> usize {
    ((width as usize) + 3) / 4 * 4
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: oddscan <container>");
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

    // For each resource, collect every candidate table.
    let mut shapes: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    let mut row_sum_w = 0usize;
    let mut row_sum_h = 0usize;
    let mut rows_total = 0usize;
    let mut neither: Vec<String> = Vec::new();
    let mut no_table = 0usize;
    let mut resources = 0usize;

    for i in 0..n {
        let dim = c.dimension(i);
        let (w, h) = (dim.width as usize, dim.height as usize);
        if w == 0 || h == 0 {
            continue;
        }
        let plain = if c.sizes[i] == 0 {
            c.payload(i).to_vec()
        } else {
            match lz77::decompress(c.payload(i)) {
                Ok(d) => d,
                Err(_) => continue,
            }
        };
        let targets = [
            w * h,
            w * h * 4,
            PALETTE_BYTES + stride8(dim.width) * h,
            stride8(dim.width) * h,
        ];
        if targets.contains(&plain.len()) {
            continue;
        }
        resources += 1;

        let base = if c.kind_of(i) == 1 { PALETTE_BYTES } else { 0 };
        let body = &plain[base..];

        // Every start where entries[0] == 2*n and the run is non-decreasing.
        let mut found: Vec<(usize, usize, usize)> = Vec::new(); // (start, n, last)
        for sb in (0..body.len().saturating_sub(8)).step_by(2) {
            if sb + 8 > body.len() {
                break;
            }
            let first = u16::from_be_bytes([body[sb], body[sb + 1]]) as usize;
            if first < 8 || first % 2 != 0 || sb + first > body.len() {
                continue;
            }
            let cnt = first / 2;
            let mut prev = 0usize;
            let mut ok = true;
            for k in 0..cnt {
                let v = u16::from_be_bytes([body[sb + 2 * k], body[sb + 2 * k + 1]]) as usize;
                if v < prev {
                    ok = false;
                    break;
                }
                prev = v;
            }
            if ok && prev <= body.len() - sb {
                found.push((sb, cnt, prev));
            }
        }

        if found.is_empty() {
            no_table += 1;
            continue;
        }

        // Classify each candidate by how n relates to w and h.
        for (sb, cnt, last) in &found {
            let label = if *cnt == 2 * h + 1 {
                "2h+1"
            } else if *cnt == 2 * h {
                "2h"
            } else if *cnt == h + 1 {
                "h+1"
            } else if *cnt == h {
                "h"
            } else if *cnt == 2 * w + 1 {
                "2w+1"
            } else if *cnt == 2 * w {
                "2w"
            } else if *cnt == w {
                "w"
            } else {
                "other"
            };
            *shapes.entry(label.to_string()).or_insert(0) += 1;
            if *cnt == 2 * h + 1 {
                // Row sums against width AND height.
                let ts = cnt * 2;
                let entries: Vec<usize> = (0..*cnt)
                    .map(|k| {
                        let p = sb + 2 * k;
                        u16::from_be_bytes([body[p], body[p + 1]]) as usize
                    })
                    .collect();
                let rows = h.min(cnt - 1);
                for r in 0..rows {
                    let a = entries[r];
                    let b = entries[r + 1];
                    if b > *last || a > *last {
                        continue;
                    }
                    let slice = &body[sb + a..sb + b];
                    let s: u64 = slice.iter().map(|&x| x as u64).sum();
                    rows_total += 1;
                    if s == w as u64 {
                        row_sum_w += 1;
                    } else if s == h as u64 {
                        row_sum_h += 1;
                    } else if neither.len() < 12 {
                        neither.push(format!(
                            "#{i} {} {w}x{h} row {r}: sum={s} bytes={:02x?}",
                            c.name(i),
                            slice
                        ));
                    }
                }
            }
        }
    }

    println!("odd resources: {resources}, no candidate table: {no_table}");
    println!("table shapes by n vs dimensions: {shapes:?}");
    println!(
        "rows summing to width: {row_sum_w}/{rows_total}; to height: {row_sum_h}/{rows_total}"
    );
    println!("rows summing to neither (first 12):");
    for s in &neither {
        println!("  {s}");
    }
}
