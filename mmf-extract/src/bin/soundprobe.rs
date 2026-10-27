//! Inspect the sound resources of an MMFW "Sounds" container.
//!
//! `soundprobe <container>` prints, for every resource: index, name, stored
//! length, decoded length, encoding, format and the first samples as hex.
//!
//! `soundprobe <container> <index>...` restricts the output to those indices.

use mmf_extract::container::Container;
use mmf_extract::sound;

fn hex(b: &[u8]) -> String {
    b.iter()
        .map(|x| format!("{x:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: soundprobe <container> [index...]");
        return;
    }
    let c = match Container::open(&args[0]) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}: {e:#}", args[0]);
            return;
        }
    };

    let wanted: Vec<usize> = args[1..].iter().filter_map(|a| a.parse().ok()).collect();
    let all = wanted.is_empty();
    let n = c.resource_count as usize;

    println!(
        "container {} kind={:?} version={} big_endian={} resources={} names_end={}",
        c.path.display(),
        c.kind,
        c.version,
        c.big_endian,
        n,
        c.names_end
    );

    let mut total_stored = 0usize;
    let mut total_pcm = 0usize;
    let mut failures = 0usize;

    for i in 0..n {
        if !all && !wanted.contains(&i) {
            continue;
        }
        let stored = c.payload_len(i);
        total_stored += stored;
        let decoded = match sound::decode(&c, i) {
            Ok(d) => d,
            Err(e) => {
                failures += 1;
                println!(
                    "#{i:05} '{}' stored={stored} DECODE FAILED: {e:#}",
                    c.name(i)
                );
                continue;
            }
        };
        total_pcm += decoded.pcm.len();
        let head = &decoded.pcm[..decoded.pcm.len().min(32)];
        println!(
            "#{i:05} '{}' stored={stored} size_tbl={} pcm={} encoding={:?} rate={} channels={} bits={} attr=0x{:02x} flags=0x{:02x}",
            c.name(i),
            c.sizes[i],
            decoded.pcm.len(),
            decoded.encoding,
            decoded.format.sample_rate,
            decoded.format.channels,
            decoded.format.bits_per_sample,
            c.attr_of(i),
            c.flags_of(i),
        );
        println!("        hex: {}", hex(head));
    }

    println!("total stored={total_stored} total pcm={total_pcm} failures={failures}");
}
