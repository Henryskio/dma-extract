//! Inspect individual resources of a container.
//!
//! `probe <container> <index>...` prints the metadata, the stored payload's
//! first bytes and, when it decompresses, the decompressed payload's first
//! bytes too.

use mmf_extract::container::Container;
use mmf_extract::lz77;

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect::<Vec<_>>().join(" ")
}

fn words(b: &[u8]) -> String {
    b.chunks_exact(2)
        .take(24)
        .map(|c| format!("{:04x}", u16::from_be_bytes([c[0], c[1]])))
        .collect::<Vec<_>>()
        .join(" ")
}

fn dword(b: &[u8]) -> String {
    b.chunks_exact(4)
        .take(16)
        .map(|c| format!("{:08x}", u32::from_be_bytes([c[0], c[1], c[2], c[3]])))
        .collect::<Vec<_>>()
        .join(" ")
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: probe <container> <index>...");
        return;
    }
    let c = match Container::open(&args[0]) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}: {e:#}", args[0]);
            return;
        }
    };

    for a in &args[1..] {
        let i: usize = match a.parse() {
            Ok(i) => i,
            Err(_) => continue,
        };
        if i >= c.resource_count as usize {
            println!("#{i}: out of range");
            continue;
        }
        let dim = c.dimension(i);
        let p = c.payload(i);
        println!(
            "#{i} '{}' w={} h={} kind={} flags=0x{:02x} stored={} size_tbl={} ",
            c.name(i),
            dim.width,
            dim.height,
            c.kind_of(i),
            c.flags_of(i),
            p.len(),
            c.sizes[i]
        );
        println!("  stored: {}", hex(&p[..p.len().min(48)]));
        println!("  stored u16be: {}", words(&p[..p.len().min(96)]));
        match lz77::decompress(p) {
            Ok(d) => {
                println!("  plain:  {} bytes", d.len());
                println!("  plain hex: {}", hex(&d[..d.len().min(64)]));
                println!("  plain u16be: {}", words(&d[..d.len().min(96)]));
                println!("  plain u32be: {}", dword(&d[..d.len().min(64)]));
            }
            Err(e) => println!("  decompress error: {e}"),
        }
        println!();
    }
}
