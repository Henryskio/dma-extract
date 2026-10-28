//! Dump the odd picture payloads with their metadata for analysis.
//!
//! `oddsplit <container>` prints, for every resource whose decoded length
//! matches no known layout, the container metadata (flags/attr/kind/dims)
//! and the first bytes of the payload, plus structural probes: whether the
//! length fits a (count, N-byte) record stride, and what the running count
//! sums to.

use mmf_extract::container::Container;
use mmf_extract::lz77;
use mmf_extract::picture::PALETTE_BYTES;

fn stride8(width: u16) -> usize {
    ((width as usize) + 3) / 4 * 4
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect::<Vec<_>>().join("")
}

/// Sum of the count byte for a (count, payload) record of `rec` bytes.
fn count_sum(src: &[u8], rec: usize) -> Option<u64> {
    if rec == 0 || src.len() % rec != 0 {
        return None;
    }
    let mut total = 0u64;
    let mut i = 0;
    while i < src.len() {
        total += src[i] as u64;
        i += rec;
    }
    Some(total)
}

/// Same, but reading the count as a big-endian u16.
fn count_sum16(src: &[u8], rec: usize) -> Option<u64> {
    if rec < 2 || src.len() % rec != 0 {
        return None;
    }
    let mut total = 0u64;
    let mut i = 0;
    while i + 1 < src.len() {
        total += u16::from_be_bytes([src[i], src[i + 1]]) as u64;
        i += rec;
    }
    Some(total)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: oddsplit <container> <count|all> [index]");
        return;
    }
    let limit: usize = if args[1] == "all" {
        usize::MAX
    } else {
        args[1].parse().unwrap_or(5)
    };
    let only: Option<usize> = args.get(2).and_then(|s| s.parse().ok());

    let c = match Container::open(&args[0]) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}: {e:#}", args[0]);
            return;
        }
    };
    let n = c.resource_count as usize;

    let mut shown = 0usize;
    for i in 0..n {
        let dim = c.dimension(i);
        let (w, h) = (dim.width, dim.height);
        let pixels = w as usize * h as usize;
        if pixels == 0 {
            continue;
        }
        if let Some(k) = only {
            if i != k {
                continue;
            }
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
            pixels,
            pixels * 4,
            PALETTE_BYTES + stride8(w) * h as usize,
            stride8(w) * h as usize,
        ];
        if targets.contains(&plain.len()) {
            continue;
        }
        shown += 1;
        if shown > limit {
            continue;
        }

        println!(
            "#{i}\t{}\t{w}x{h}\tflags={:02x}\tattr={:02x}\tkind={}\tstored={}\tplain={}",
            c.name(i),
            c.flags_of(i),
            c.attr_of(i),
            c.kind_of(i),
            c.payload_len(i),
            plain.len()
        );
        let head = &plain[..plain.len().min(64)];
        println!("    head: {}", hex(head));

        // Does the payload start with something that looks like a palette?
        if plain.len() >= 8 {
            let e0 = u32::from_be_bytes([plain[0], plain[1], plain[2], plain[3]]);
            println!(
                "    first4 BE={e0:08x} LE={:08x}",
                u32::from_le_bytes([plain[0], plain[1], plain[2], plain[3]])
            );
        }

        // Record-stride probes.
        for rec in [2usize, 3, 4, 5, 6, 8] {
            if plain.len() % rec == 0 {
                let s = count_sum(&plain, rec);
                let s16 = count_sum16(&plain, rec);
                let fits = |v: u64| {
                    [
                        pixels as u64,
                        (pixels * 3) as u64,
                        (pixels * 4) as u64,
                        (stride8(w) * h as usize) as u64,
                    ]
                    .contains(&v)
                };
                let mark = |v: Option<u64>| match v {
                    Some(x) if fits(x) => format!("{x}*"),
                    Some(x) => format!("{x}"),
                    None => "-".into(),
                };
                println!(
                    "    rec={rec}: n={} sum8={} sum16be={}",
                    plain.len() / rec,
                    mark(s),
                    mark(s16)
                );
            }
        }

        // Palette-first hypothesis: is there a 1024-byte prefix that looks
        // like a palette, and what is left over?
        if plain.len() > PALETTE_BYTES {
            let rest = plain.len() - PALETTE_BYTES;
            let pal_entry0 = &plain[0..4];
            println!(
                "    minus1024={rest} div4={} div3={} div2={} pal0={}",
                rest % 4 == 0,
                rest % 3 == 0,
                rest % 2 == 0,
                hex(pal_entry0)
            );
            if rest % 4 == 0 {
                let s = count_sum(&plain[PALETTE_BYTES..], 4);
                if let Some(x) = s {
                    println!("    after-pal rec4 sum8={x} (pixels={pixels})");
                }
            }
        }

        // Distinct byte values in the first 256 bytes: a smooth ramp of many
        // distinct values points at a LUT; few values points at RLE colours.
        let mut seen = [false; 256];
        for &b in head {
            seen[b as usize] = true;
        }
        println!(
            "    distinct bytes in head={} ",
            seen.iter().filter(|&&x| x).count()
        );
    }
    eprintln!("odd resources examined: {shown}");
}
