//! Dump the raw skeleton of an MMFW container the shared parser rejects.
//!
//! Scratch bin: the third game's `DMA3D.MMM` (Models) and `DMA3D.MMX`
//! (Textures) are `MMFW` containers of container version 3, but the
//! per-kind metadata parser in `container.rs` refuses to open them, so there
//! is nothing to ask the library for. This bin reads the skeleton straight
//! from the bytes -- the parts every MMFW container shares, which
//! `container.rs`'s own header layout documents -- and prints the region
//! between the names table and the first payload, which is the part that is
//! **not** shared and therefore needs measuring per kind.
//!
//! The goal is geometry, not decoding: find `names_end`, find
//! `offsets[0]`, and see what lies between them.

use std::path::Path;

fn u16be(d: &[u8], o: usize) -> u16 {
    u16::from_be_bytes([d[o], d[o + 1]])
}
fn u32be(d: &[u8], o: usize) -> u32 {
    u32::from_be_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    for path in &args {
        let data = match std::fs::read(path) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("{path}: {e}");
                continue;
            }
        };
        if &data[0..5] != b"MMFW " {
            eprintln!("{path}: not an MMFW container");
            continue;
        }
        let tag_len = data[5] as usize;
        let tag = String::from_utf8_lossy(&data[6..6 + tag_len]).to_string();
        let big = &data[0x10..0x12] == b"MM";
        let version = u16be(&data, 0x12);
        // Version 1 puts N at 0x1E and the offsets table at 0x20; versions 2
        // and 3 put N at 0x22 and the table at 0x24.
        let (n, off_base) = if version == 1 {
            (u16be(&data, 0x1E) as usize, 0x20)
        } else {
            (u16be(&data, 0x22) as usize, 0x24)
        };
        let n = n as usize;
        println!("=== {path}");
        println!(
            "  tag={tag:?} endian={} version={version} N={n} file={} bytes",
            if big { "MM (big)" } else { "II (little)" },
            data.len()
        );

        let offsets: Vec<u32> = (0..=n)
            .map(|i| {
                let o = off_base + 4 * i;
                if big {
                    u32be(&data, o)
                } else {
                    u32::from_le_bytes([data[o], data[o + 1], data[o + 2], data[o + 3]])
                }
            })
            .collect();
        let first = offsets[0] as usize;
        let last = offsets[n] as usize;
        println!(
            "  offsets[0]={first} offsets[N]={last} file_len={} => {}",
            data.len(),
            if last == data.len() {
                "exact"
            } else {
                "MISMATCH"
            }
        );
        // A monotone table that ends at EOF is the skeleton's own proof.
        let monotone = offsets.windows(2).all(|w| w[0] <= w[1]);
        println!("  offsets monotone: {monotone}");

        // Names: 32-byte NUL-padded fields, after the table + 2 padding bytes.
        let names_end = off_base + 4 * (n + 1) + 2;
        let meta_start = names_end + 32 * n;
        let meta_len = first.saturating_sub(meta_start);
        println!(
            "  names {:#x}..{:#x}, metadata {:#x}..{:#x} = {} bytes for {n} resources",
            names_end,
            meta_start,
            meta_start,
            first,
            meta_len
        );
        for i in 0..n.min(6) {
            let f = &data[names_end + 32 * i..names_end + 32 * (i + 1)];
            let s = f.iter().take_while(|&&b| b != 0).collect::<Vec<_>>();
            println!(
                "    name[{i}] = {:?}",
                String::from_utf8_lossy(&s.iter().map(|b| **b).collect::<Vec<_>>())
            );
        }

        // Which known layouts fit the metadata region? Only a comment for the
        // reader; the sizes of the two measured kinds are 14*N+2 (sounds) and
        // 15*N+4 (pictures).
        println!("  meta_len/N = {:.3}", meta_len as f64 / n as f64);
        for (label, size) in [
            ("14*N+2 (sounds)", 14 * n + 2),
            ("15*N+4 (pictures)", 15 * n + 4),
            ("16*N", 16 * n),
            ("12*N", 12 * n),
            ("8*N", 8 * n),
            ("4*N", 4 * n),
            ("2*N", 2 * n),
        ] {
            if size == meta_len {
                println!("  ** metadata length matches {label}");
            }
        }

        // The metadata region's own bytes: a u32 array, a u16 array and a byte
        // array are the three shapes that appear in the known kinds, so show
        // the first 64 bytes as all three readings and let the reader compare.
        if meta_len >= 64 {
            let region = &data[meta_start..meta_start + 64.min(meta_len)];
            let hex = region
                .iter()
                .take(48)
                .map(|b| format!("{b:02x}"))
                .collect::<Vec<_>>()
                .join(" ");
            println!("  metadata[0..48]: {hex}");
            let as_u32 = region
                .chunks_exact(4)
                .take(8)
                .map(|c| if big { u32be(c, 0) } else { u32::from_le_bytes([c[0], c[1], c[2], c[3]]) })
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join(" ");
            println!("  metadata as u32[0..8]: {as_u32}");
            let as_u16 = region
                .chunks_exact(2)
                .take(10)
                .map(|c| if big { u16be(c, 0) } else { u16::from_le_bytes([c[0], c[1]]) })
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join(" ");
            println!("  metadata as u16[0..10]: {as_u16}");
        }

        // The first payload: enough to see whether resources carry their own
        // sub-header, and whether the region is ASCII.
        let payload = &data[first..(first + 96).min(data.len())];
        let hex = payload
            .iter()
            .take(48)
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join(" ");
        println!("  payload[0][0..48]: {hex}");
        let text: String = payload
            .iter()
            .take(96)
            .filter(|b| b.is_ascii_graphic() || **b == b' ')
            .map(|&b| b as char)
            .collect();
        if !text.is_empty() {
            println!("  payload[0] printable: {text:?}");
        }
        println!();
        let _ = Path::new(path);
    }
}
