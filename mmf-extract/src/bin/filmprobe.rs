//! Probe a Films container: resource table plus the inner `.flm` chunk
//! stream.
//!
//! Usage: `filmprobe <container.mmf>`.

use anyhow::{Result, bail};
use mmf_extract::container::{Container, ContainerKind};
use mmf_extract::lz77;

const TAG: u16 = 0xF789;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: filmprobe <container.mmf>");
        return Ok(());
    }
    for path in &args {
        let container = Container::open(path)?;
        println!(
            "{}: kind={:?} big_endian={} version={} N={} names_end={} payload_start={} meta_len={}",
            container.path.display(),
            container.kind,
            container.big_endian,
            container.version,
            container.resource_count,
            container.names_end,
            container.offsets.first().copied().unwrap_or(0),
            container.offsets.first().copied().unwrap_or(0) as usize - container.names_end,
        );

        // The generic metadata interpretation is only valid for pictures.
        // Show it anyway so the film-specific layout can be contrasted.
        for i in 0..container.resource_count as usize {
            let stored = container.payload_len(i);
            let plain = container.plain_len(i);
            let d = container.dimension(i);
            let head = &container.payload(i)[..container.payload_len(i).min(32)];
            println!(
                "  [{i:03}] {:32} stored={stored:9} sizes={:10} plain={plain:9} dim={}x{} flags={:02x} attr={:02x} kind={}",
                container.name(i),
                container.sizes[i],
                d.width,
                d.height,
                container.flags_of(i),
                container.attr_of(i),
                container.kind_of(i),
            );
            println!("        head={}  ascii={}", hex(head), ascii(head));

            if container.kind == ContainerKind::Films {
                probe_film(i, container.payload(i))?;
            }
        }
    }
    Ok(())
}

fn probe_film(index: usize, data: &[u8]) -> Result<()> {
    println!("  --- film {index}: {} bytes ---", data.len());
    let mut off = 0usize;
    let mut counts = std::collections::BTreeMap::<u16, usize>::new();
    let mut chunk_no = 0usize;
    let mut seen_type11_printed = false;
    while off + 8 <= data.len() {
        let tag = u16::from_be_bytes([data[off], data[off + 1]]);
        let typ = u16::from_be_bytes([data[off + 2], data[off + 3]]);
        let len = u32::from_be_bytes([data[off + 4], data[off + 5], data[off + 6], data[off + 7]])
            as usize;
        if tag != TAG {
            bail!("film {index}: bad chunk tag {tag:#06x} at {off:#x}");
        }
        if off + 8 + len > data.len() {
            bail!("film {index}: chunk type {typ} at {off:#x} length {len} runs past end");
        }
        let p = &data[off + 8..off + 8 + len];
        *counts.entry(typ).or_default() += 1;
        let first_type11 = typ == 11 && !seen_type11_printed;
        if typ == 11 && !seen_type11_printed {
            seen_type11_printed = true;
        }
        if chunk_no < 6 || typ == 3 || typ == 7 || typ == 10 || first_type11 {
            let u32s: Vec<String> = p
                .chunks_exact(4)
                .take(8)
                .map(|c| format!("{}", u32::from_be_bytes([c[0], c[1], c[2], c[3]])))
                .collect();
            println!(
                "    chunk {chunk_no:04} @{off:08x} type={typ:#06x} len={len:7} u32=[{}]",
                u32s.join(",")
            );
            if typ == 2 && len >= 12 {
                let data_len = be32(p, 0);
                let frame = be32(p, 8);
                println!(
                    "        audio/frame block: data_len={data_len} index={frame} (len-12={})",
                    len - 12
                );
            }
            if typ == 11 && len >= 8 {
                let a = be32(p, 0);
                let b = be32(p, 4);
                let body = &p[8..];
                let dec = lz77::decompress(body);
                match &dec {
                    Ok(d) => println!(
                        "        video delta: a={a} b={b} body={} lz77_ok=true out={} match_b={}",
                        body.len(),
                        d.len(),
                        d.len() == b as usize
                    ),
                    Err(e) => println!(
                        "        video delta: a={a} b={b} body={} lz77_ok=false err={e}",
                        body.len()
                    ),
                }
            }
        }
        chunk_no += 1;
        off += 8 + len;
    }
    println!(
        "    chunks={chunk_no} types={:?} end={off:#x} file_len={:#x}",
        counts,
        data.len()
    );
    Ok(())
}

fn be32(d: &[u8], off: usize) -> u32 {
    u32::from_be_bytes([d[off], d[off + 1], d[off + 2], d[off + 3]])
}

fn hex(d: &[u8]) -> String {
    d.iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn ascii(d: &[u8]) -> String {
    d.iter()
        .map(|&b| {
            if (32..127).contains(&b) {
                b as char
            } else {
                '.'
            }
        })
        .collect()
}
