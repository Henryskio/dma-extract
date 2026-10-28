//! Test whether the second section is a flat colour stream consumed one
//! record per run.
//!
//! `oddtot <container> <index>` locates the u16 table, then compares the total
//! number of run-length bytes in section 1 with the total number of 4-byte
//! records in section 2, and checks whether walking rows and runs while
//! consuming one section-2 record per run lands exactly on the end.

use mmf_extract::container::Container;
use mmf_extract::lz77;
use mmf_extract::picture::PALETTE_BYTES;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: oddtot <container> <index>");
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
    let data = &body[start + ts..];

    let slice = |i: usize| -> &[u8] { &body[start + entries[i]..start + entries[i + 1]] };

    let total_runs: usize = (0..h).map(|i| slice(i).len()).sum();
    let sec2_bytes: usize = (h..n - 1).map(|i| slice(i).len()).sum();
    println!(
        "#{idx} {} {w}x{h} kind={} entries={n} data={} bytes",
        c.name(idx),
        c.kind_of(idx),
        data.len()
    );
    println!("section 1: {total_runs} run-length bytes over {h} rows");
    println!("section 2: {sec2_bytes} bytes = {} 4-byte records", sec2_bytes / 4);
    println!("pixels: {}", w * h);
    println!(
        "match? runs==records: {}   pixels==records*4: {}",
        total_runs == sec2_bytes / 4,
        w * h == sec2_bytes
    );

    // Per-row comparison: run count vs record count in the paired slice.
    let mut same = 0usize;
    for i in 0..h {
        let runs = slice(i).len();
        let recs = slice(h + i).len() / 4;
        if runs == recs {
            same += 1;
        }
    }
    println!("rows where nruns == ncols/4: {same}/{h}");

    // Cumulative: does the running record count track the running run count?
    let mut run_cursor = 0usize;
    let mut rec_cursor = 0usize;
    let mut drift = Vec::new();
    for i in 0..h {
        run_cursor += slice(i).len();
        rec_cursor += slice(h + i).len() / 4;
        drift.push((i, run_cursor, rec_cursor));
    }
    println!("cumulative (row, runs, records) first 12: {:?}", &drift[..12.min(drift.len())]);
    println!("cumulative last 4: {:?}", &drift[drift.len() - 4..]);

    // Is section 2 a flat (count, R, G, B) pixel RLE whose counts sum to the
    // pixel total?  Also try (R, G, B, count).
    for (name, cnt_at) in [("first byte", 0usize), ("last byte", 3)] {
        let mut sum = 0u64;
        let mut recs = 0usize;
        let mut off = start + ts + entries[h] - ts;
        let end = start + entries[n - 1];
        while off + 4 <= end {
            sum += body[off + cnt_at] as u64;
            off += 4;
            recs += 1;
        }
        println!(
            "section 2 as flat ({name} = count): {recs} records, count sum={sum}, pixels={}",
            w * h
        );
    }

    // Same, but read as (count, index) pairs over the whole section.
    {
        let mut sum = 0u64;
        let mut i = start + ts;
        let end = start + entries[n - 1];
        let mut recs = 0usize;
        while i + 1 < end {
            sum += body[i] as u64;
            i += 2;
            recs += 1;
        }
        println!(
            "section 2 as flat (count,index) pairs: {recs} records, count sum={sum}, pixels={}",
            w * h
        );
    }
}
