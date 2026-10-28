//! Full-payload dump for one odd picture resource.
//!
//! `oddfull <container> <index>` prints the whole decompressed payload
//! three ways: as u32 big-endian words (the game stores 4-byte groups),
//! as a run-length view, and as a byte histogram summary.

use mmf_extract::container::Container;
use mmf_extract::lz77;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: oddfull <container> <index>");
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

    // 1. u32 big-endian words, 16 to a line.
    println!("\n-- u32 BE words --");
    let words: Vec<u32> = plain
        .chunks_exact(4)
        .map(|w| u32::from_be_bytes([w[0], w[1], w[2], w[3]]))
        .collect();
    for (i, chunk) in words.chunks(8).enumerate() {
        let line: Vec<String> = chunk.iter().map(|w| format!("{w:08x}")).collect();
        println!("{:5}: {}", i * 8, line.join(" "));
    }
    if plain.len() % 4 != 0 {
        println!("tail: {:02x?}", &plain[plain.len() / 4 * 4..]);
    }

    // 2. Run-length over the u32 words.
    println!("\n-- runs over u32 words (value, count) --");
    let mut runs: Vec<(u32, usize)> = Vec::new();
    for &w in &words {
        if let Some(last) = runs.last_mut() {
            if last.0 == w {
                last.1 += 1;
                continue;
            }
        }
        runs.push((w, 1));
    }
    println!("runs={}", runs.len());
    for (i, chunk) in runs.chunks(8).enumerate() {
        let line: Vec<String> = chunk.iter().map(|(w, n)| format!("{w:08x}x{n}")).collect();
        println!("{:5}: {}", i * 8, line.join(" "));
    }

    // 3. Where the byte pattern changes: for each 4-byte group, classify
    //    bytes 1..4 as all-zero or not.
    println!("\n-- group shape --");
    let mut shape = String::new();
    for w in plain.chunks_exact(4) {
        shape.push(if w[1] == 0 && w[2] == 0 && w[3] == 0 {
            '0'
        } else {
            'C'
        });
    }
    for (i, chunk) in shape.as_bytes().chunks(64).enumerate() {
        println!("{:5}: {}", i * 64, String::from_utf8_lossy(chunk));
    }

    // 4. Sum of the first byte of each group.
    let sum: u64 = plain.chunks_exact(4).map(|w| w[0] as u64).sum();
    let px = dim.width as usize * dim.height as usize;
    println!(
        "\nsum(group[0])={sum}  pixels={px}  sum*4={}  pal+stride*h={}",
        sum * 4,
        1024 + ((dim.width as usize + 3) / 4 * 4) * dim.height as usize
    );
}
