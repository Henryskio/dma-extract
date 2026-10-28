//! Find how the u16 offset table of an odd picture payload is split.
//!
//! `oddsplit2 <container> <index>` prints every candidate split point of the
//! table into two halves and scores it by whether the two halves describe
//! matching per-row slice lengths, then dumps the best split's rows.

use mmf_extract::container::Container;
use mmf_extract::lz77;
use mmf_extract::picture::PALETTE_BYTES;

fn u16at(b: &[u8], byte_off: usize) -> usize {
    u16::from_be_bytes([b[byte_off], b[byte_off + 1]]) as usize
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: oddsplit2 <container> <index>");
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

    // Locate every u16 table whose first entry is twice its own entry count
    // and whose entries never decrease.
    let mut tables: Vec<(usize, usize)> = Vec::new();
    for sb in (0..body.len().saturating_sub(8)).step_by(2) {
        if sb + 8 > body.len() {
            break;
        }
        let n = u16at(body, sb);
        if n < 8 || n % 2 != 0 {
            continue;
        }
        let cnt = n / 2;
        if sb + n > body.len() {
            continue;
        }
        let mut ok = true;
        let mut prev = 0usize;
        for k in 0..cnt {
            let v = u16at(body, sb + 2 * k);
            if v < prev {
                ok = false;
                break;
            }
            prev = v;
        }
        if ok {
            tables.push((sb, cnt));
        }
    }
    println!(
        "#{idx} {} {w}x{h} kind={} candidate tables: {}",
        c.name(idx),
        c.kind_of(idx),
        tables.len()
    );

    for (sb, cnt) in tables {
        let ts = cnt * 2;
        let entries: Vec<usize> = (0..cnt).map(|k| u16at(body, sb + 2 * k)).collect();
        let last = entries[cnt - 1];
        if last > body.len() - sb {
            continue;
        }
        println!(
            "\ntable@byte {sb} entries={cnt} ts={ts} last={last} avail={}",
            body.len() - sb
        );
        let mut scored: Vec<(usize, usize)> = Vec::new();
        for s in 1..cnt - 1 {
            let m = s.min(cnt - 1 - s);
            let mut good = 0usize;
            for i in 0..m {
                if entries[i + 1] - entries[i] == entries[s + i + 1] - entries[s + i] {
                    good += 1;
                }
            }
            scored.push((good, s));
        }
        scored.sort_by(|a, b| b.0.cmp(&a.0));
        for (good, s) in scored.iter().take(3) {
            println!("  split {s}: {good}/{} rows match", (*s).min(cnt - 1 - *s));
        }
        let (good, s) = scored[0];
        if good == 0 {
            continue;
        }
        let rows = s.min(cnt - s);
        println!("  -- rows for split {s} --");
        for i in 0..rows.min(20) {
            let la = entries[i];
            let lb = entries[i + 1];
            let ca = entries[s + i];
            let cb = entries[s + i + 1];
            let lens: Vec<u8> = body[sb + la..sb + lb].to_vec();
            let cols: Vec<u8> = body[sb + ca..sb + cb].to_vec();
            let sum: u64 = lens.iter().map(|&x| x as u64).sum();
            println!(
                "   row {i:3}: lens({})={:02x?} sum={sum} cols({})={:02x?}",
                lens.len(),
                lens,
                cols.len(),
                cols
            );
        }
    }
}
