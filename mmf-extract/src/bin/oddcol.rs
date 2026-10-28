//! Score candidate rules for where an odd picture row's colours come from.
//!
//! `oddcol <container>` walks every resource whose decoded length matches no
//! known pixel layout, locates its u16 offset table, and scores a set of
//! candidate interpretations of the table's second half against the first
//! half's run-length rows. The rule that holds for every row of every
//! resource is the answer.

use mmf_extract::container::Container;
use mmf_extract::lz77;
use mmf_extract::picture::PALETTE_BYTES;

fn stride8(width: u16) -> usize {
    ((width as usize) + 3) / 4 * 4
}

/// One candidate interpretation, scored row by row.
struct RuleX {
    name: &'static str,
    ok: usize,
    bad: usize,
    first_bad: Option<String>,
}

/// The decoded payload and the table found inside it.
struct Found {
    plain: Vec<u8>,
    /// byte offset of the table inside `plain`
    start: usize,
    entries: Vec<usize>,
    data: Vec<u8>,
}

fn locate(plain: &[u8], h: usize) -> Option<Found> {
    for start in 0..plain.len().saturating_sub(8) {
        let first = u16::from_be_bytes([plain[start], plain[start + 1]]) as usize;
        if first < 8 || first % 2 != 0 || start + first > plain.len() {
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
            let v = u16::from_be_bytes([plain[p], plain[p + 1]]) as usize;
            if v < prev {
                ok = false;
                break;
            }
            prev = v;
        }
        if !ok || prev > plain.len() - start {
            continue;
        }
        let ts = n * 2;
        let entries: Vec<usize> = (0..n)
            .map(|k| {
                let p = start + 2 * k;
                u16::from_be_bytes([plain[p], plain[p + 1]]) as usize
            })
            .collect();
        let data = plain[start + ts..].to_vec();
        return Some(Found {
            plain: plain.to_vec(),
            start,
            entries,
            data,
        });
    }
    None
}

/// Slice `i` of the table, as a byte vector.
fn slice_of(f: &Found, i: usize) -> Vec<u8> {
    let a = f.entries[i];
    let b = f.entries[i + 1];
    f.plain[f.start + a..f.start + b].to_vec()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: oddcol <container>");
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

    let mut rules: Vec<RuleX> = vec![
        RuleX { name: "A: cols=slice[h+i], one byte per run", ok: 0, bad: 0, first_bad: None },
        RuleX { name: "B: cols=slice[h+i] as (cnt,idx) from 0, sum(cnt)=nruns", ok: 0, bad: 0, first_bad: None },
        RuleX { name: "C: cols=slice[h+i] as (cnt,idx) from 1, sum(cnt)=nruns", ok: 0, bad: 0, first_bad: None },
        RuleX { name: "D: cols=slice[h+i] as (idx,cnt) from 0, sum(cnt)=nruns", ok: 0, bad: 0, first_bad: None },
        RuleX { name: "E: cols=slice[h+i] as (idx,cnt) from 1, sum(cnt)=nruns", ok: 0, bad: 0, first_bad: None },
        RuleX { name: "F: cols=slice[h+i], sum(even bytes)=width", ok: 0, bad: 0, first_bad: None },
        RuleX { name: "G: cols=slice[h+i], sum(odd bytes)=width", ok: 0, bad: 0, first_bad: None },
        RuleX { name: "H: cols=slice[h+i], sum(all bytes)=width", ok: 0, bad: 0, first_bad: None },
        RuleX { name: "I: cols=slice[h+i], ncols = 2*nruns", ok: 0, bad: 0, first_bad: None },
        RuleX { name: "J: cols=slice[h+i], ncols = nruns+1", ok: 0, bad: 0, first_bad: None },
        RuleX { name: "K: cols=slice[h+i-1] (shift -1), one per run", ok: 0, bad: 0, first_bad: None },
        RuleX { name: "L: cols=slice[h+i+1] (shift +1), one per run", ok: 0, bad: 0, first_bad: None },
        RuleX { name: "M: cols=slice[h+i], ncols even, all (cnt,idx) pairs", ok: 0, bad: 0, first_bad: None },
    ];

    let mut resources = 0usize;
    let mut located = 0usize;
    let mut run_rows_ok = 0usize;
    let mut run_rows = 0usize;

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
        let Some(f) = locate(&plain, h) else {
            continue;
        };
        located += 1;

        for row in 0..h {
            let runs = slice_of(&f, row);
            let sum: u64 = runs.iter().map(|&x| x as u64).sum();
            run_rows += 1;
            if sum == w as u64 {
                run_rows_ok += 1;
            }

            let nruns = runs.len();
            let cols = slice_of(&f, h + row);
            let ncols = cols.len();
            let tag = format!("#{i} r{row} nruns={nruns} ncols={ncols}");

            // A
            mark(&mut rules[0], ncols == nruns, &tag);
            // B / C / D / E
            mark(&mut rules[1], pairs_sum(&cols, 0, true) == Some(nruns as u64), &tag);
            mark(&mut rules[2], pairs_sum(&cols, 1, true) == Some(nruns as u64), &tag);
            mark(&mut rules[3], pairs_sum(&cols, 0, false) == Some(nruns as u64), &tag);
            mark(&mut rules[4], pairs_sum(&cols, 1, false) == Some(nruns as u64), &tag);
            // F / G / H
            let even: u64 = cols.iter().step_by(2).map(|&x| x as u64).sum();
            let odd: u64 = cols.iter().skip(1).step_by(2).map(|&x| x as u64).sum();
            let all: u64 = cols.iter().map(|&x| x as u64).sum();
            mark(&mut rules[5], even == w as u64, &tag);
            mark(&mut rules[6], odd == w as u64, &tag);
            mark(&mut rules[7], all == w as u64, &tag);
            // I / J
            mark(&mut rules[8], ncols == 2 * nruns, &tag);
            mark(&mut rules[9], ncols == nruns + 1, &tag);
            // K / L
            let km = slice_of(&f, h + row).len();
            let _ = km;
            if row >= 1 {
                let ck = slice_of(&f, h + row - 1);
                mark(&mut rules[10], ck.len() == nruns, &tag);
            } else {
                rules[10].ok += 1;
            }
            if h + row + 2 <= f.entries.len() - 1 {
                let cl = slice_of(&f, h + row + 1);
                mark(&mut rules[11], cl.len() == nruns, &tag);
            } else {
                rules[11].ok += 1;
            }
            // M
            mark(&mut rules[12], ncols % 2 == 0, &tag);
        }
    }

    println!("odd resources: {resources}, table located: {located}");
    println!("rows whose run lengths sum to the width: {run_rows_ok} / {run_rows}");
    println!();
    let mut order: Vec<&RuleX> = rules.iter().collect();
    order.sort_by(|a, b| b.ok.cmp(&a.ok));
    for r in order {
        let total = r.ok + r.bad;
        let pct = if total == 0 { 0.0 } else { 100.0 * r.ok as f64 / total as f64 };
        println!("{:5.1}%  {:4}/{:4}  {}", pct, r.ok, total, r.name);
        if let Some(b) = &r.first_bad {
            println!("            first failure: {b}");
        }
    }
}

/// Sum of the count bytes when `s` is read as pairs; `first_is_count`
/// selects `(count, value)` vs `(value, count)`.
fn pairs_sum(s: &[u8], offset: usize, first_is_count: bool) -> Option<u64> {
    let mut i = offset;
    let mut total = 0u64;
    let mut pairs = 0usize;
    while i + 1 < s.len() {
        let (a, b) = (s[i], s[i + 1]);
        let cnt = if first_is_count { a } else { b };
        total += cnt as u64;
        i += 2;
        pairs += 1;
    }
    if pairs == 0 {
        None
    } else {
        Some(total)
    }
}

fn mark(r: &mut RuleX, ok: bool, tag: &str) {
    if ok {
        r.ok += 1;
    } else {
        r.bad += 1;
        if r.first_bad.is_none() {
            r.first_bad = Some(tag.to_string());
        }
    }
}
