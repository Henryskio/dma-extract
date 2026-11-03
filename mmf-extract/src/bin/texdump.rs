//! Dump the texture container's payload structure using the verified parser.
//!
//! Scratch bin. `container.rs` opens `MMFW Textures` (version 3) correctly --
//! `tests/dma3d.rs` asserts a byte-exact payload for every resource -- so this
//! asks the library for the resources and prints what is inside them, which is
//! the part that is **not** decoded yet (`docs/DMA3D.md` §2).

use mmf_extract::container::Container;

fn u16le(d: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([d[o], d[o + 1]])
}
fn u16be(d: &[u8], o: usize) -> u16 {
    u16::from_be_bytes([d[o], d[o + 1]])
}
fn u32le(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}
fn u32be(d: &[u8], o: usize) -> u32 {
    u32::from_be_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}
fn f32le(d: &[u8], o: usize) -> f32 {
    f32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}

fn main() {
    let path = std::env::args().nth(1).expect("usage: texdump <DMA3D.MMX>");
    let data = std::fs::read(&path).expect("read container");
    let c = Container::parse(&path, data).expect("open Textures");
    let n = c.resource_count as usize;
    println!("N = {n}");

    // The sizes, and the first 32 bytes of a spread of resources.
    let mut sizes = Vec::new();
    for i in 0..n {
        sizes.push((c.name(i).to_string(), c.payload_len(i)));
    }
    let mut distinct: Vec<usize> = sizes.iter().map(|(_, l)| *l).collect();
    distinct.sort_unstable();
    distinct.dedup();
    println!(
        "size: min {} max {} distinct {}",
        distinct[0],
        distinct[distinct.len() - 1],
        distinct.len()
    );
    let hist = {
        let mut m = std::collections::BTreeMap::new();
        for (_, l) in &sizes {
            *m.entry(*l).or_insert(0usize) += 1;
        }
        let mut v: Vec<(usize, usize)> = m.into_iter().collect();
        v.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
        v
    };
    println!("most common sizes: {:?}", &hist[..hist.len().min(8)]);

    println!("\n--- 12 resources, first 48 bytes ---");
    for i in 0..n {
        if i >= 12 {
            break;
        }
        let p = c.payload(i);
        let head: Vec<String> = p.iter().take(48).map(|b| format!("{b:02x}")).collect();
        println!(
            "{:>3} {:>24} len={:>7} {}",
            i,
            c.name(i),
            p.len(),
            head.join(" ")
        );
    }

    // Count how many payloads start with a 256-entry RGBA palette, i.e. the
    // first u16 really is a width and the second a height whose product is
    // what the rest of the file can hold.
    println!("\n--- geometry test: [u16 w][u16 h][data] ---");
    let mut ok8 = 0usize;
    let mut ok4 = 0usize;
    let mut other = 0usize;
    let mut rows = Vec::new();
    for i in 0..n {
        let p = c.payload(i);
        if p.len() < 8 {
            other += 1;
            continue;
        }
        let w = u16le(p, 0);
        let h = u16le(p, 2);
        let d = p.len() - 4;
        if w > 0 && h > 0 {
            if d == (w as usize) * (h as usize) * 4 {
                ok4 += 1;
                if rows.len() < 6 && (w as usize) * (h as usize) * 4 + 4 > 600 {
                    rows.push(format!("{:>3} {} {}x{} RGBA", i, c.name(i), w, h));
                }
                continue;
            }
            if d == (w as usize) * (h as usize) {
                ok8 += 1;
                if rows.len() < 6 {
                    rows.push(format!("{:>3} {} {}x{} indexed", i, c.name(i), w, h));
                }
                continue;
            }
        }
        other += 1;
    }
    println!("[u16 w][u16 h] with w*h*4+4 == len: {ok4}");
    println!("[u16 w][u16 h] with w*h+4   == len: {ok8}");
    println!("neither: {other}");
    for r in rows {
        println!("   {r}");
    }

    // Also test the BE reading of the same two bytes, since the container's
    // own tables are big-endian.
    println!("\n--- geometry test with BE [u16 w][u16 h] ---");
    let mut ok8 = 0usize;
    let mut ok4 = 0usize;
    let mut other = 0usize;
    let mut rows = Vec::new();
    for i in 0..n {
        let p = c.payload(i);
        if p.len() < 8 {
            other += 1;
            continue;
        }
        let w = u16be(p, 0) as usize;
        let h = u16be(p, 2) as usize;
        let d = p.len() - 4;
        if w > 0 && h > 0 {
            if d == w * h * 4 {
                ok4 += 1;
                if rows.len() < 6 {
                    rows.push(format!("{:>3} {} {}x{} RGBA", i, c.name(i), w, h));
                }
                continue;
            }
            if d == w * h {
                ok8 += 1;
                if rows.len() < 6 {
                    rows.push(format!("{:>3} {} {}x{} indexed", i, c.name(i), w, h));
                }
                continue;
            }
        }
        other += 1;
    }
    println!("BE [u16 w][u16 h] with w*h*4+4 == len: {ok4}");
    println!("BE [u16 w][u16 h] with w*h+4   == len: {ok8}");
    println!("neither: {other}");
    for r in rows {
        println!("   {r}");
    }

    // What do the first 8 bytes look like repeated across files?
    println!("\n--- first u32 of every payload, as u32le and u32be ---");
    let mut cl = std::collections::BTreeMap::new();
    let mut cb = std::collections::BTreeMap::new();
    for i in 0..n {
        let p = c.payload(i);
        if p.len() < 4 {
            continue;
        }
        *cl.entry(u32le(p, 0)).or_insert(0usize) += 1;
        *cb.entry(u32be(p, 0)).or_insert(0usize) += 1;
    }
    let mut cv: Vec<(u32, usize)> = cl.into_iter().collect();
    cv.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
    println!("u32le head, top 8: {:?}", &cv[..cv.len().min(8)]);
    let mut cv: Vec<(u32, usize)> = cb.into_iter().collect();
    cv.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
    println!("u32be head, top 8: {:?}", &cv[..cv.len().min(8)]);

    // Distinct value profile of one resource.
    let p = c.payload(0);
    println!("\n--- resource 0 ({}), len {} ---", c.name(0), p.len());
    let hist = {
        let mut m = std::collections::BTreeMap::new();
        for &b in p {
            *m.entry(b).or_insert(0usize) += 1;
        }
        let mut v: Vec<(u8, usize)> = m.into_iter().collect();
        v.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
        v
    };
    println!("top byte values: {:?}", &hist[..hist.len().min(10)]);
    println!("distinct bytes: {}", hist.len());
    println!(
        "as u16le pairs: {:?}",
        p.chunks(2)
            .take(16)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect::<Vec<_>>()
    );
    println!(
        "as f32 (first 12): {:?}",
        (0..12).map(|k| f32le(p, k * 4)).collect::<Vec<_>>()
    );
}
