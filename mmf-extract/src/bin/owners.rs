//! Look for the palette owner of the picture resources that carry no palette.
//!
//! `owners <container> <class>...` prints, for every resource whose decoded
//! length matches a bare indexed image, the metadata that could identify the
//! resource holding its palette.

use mmf_extract::container::Container;
use mmf_extract::lz77;
use mmf_extract::picture::PALETTE_BYTES;

fn stride8(width: u16) -> usize {
    ((width as usize) + 3) / 4 * 4
}

fn class_of(c: &Container, i: usize) -> &'static str {
    let dim = c.dimension(i);
    let pixels = dim.width as usize * dim.height as usize;
    let p = c.payload(i);
    let data = if c.sizes[i] == 0 {
        p.to_vec()
    } else {
        match lz77::decompress(p) {
            Ok(d) => d,
            Err(_) => return "decompress",
        }
    };
    if data.len() == pixels * 4 {
        "c32"
    } else if data.len() == PALETTE_BYTES + stride8(dim.width) * dim.height as usize {
        "c8"
    } else if data.len() == stride8(dim.width) * dim.height as usize && pixels > 0 {
        "nopalette"
    } else {
        "odd"
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: owners <container> <wanted-class>...");
        return;
    }
    let c = match Container::open(&args[0]) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}: {e:#}", args[0]);
            return;
        }
    };
    let wanted: Vec<String> = args[1..].to_vec();

    let n = c.resource_count as usize;
    // Nearest preceding resource that carries its own palette.
    let mut last_pal: Vec<Option<usize>> = vec![None; n];
    let mut cur: Option<usize> = None;
    for i in 0..n {
        if class_of(&c, i) == "c8" {
            cur = Some(i);
        }
        last_pal[i] = cur;
    }

    println!("#idx\tname\tw\th\tkind\tflags\tu32\tattr\tclass\tprev_pal\tk-2\tcls(k-2)\tu32-idx\tcls(u32)");
    for i in 0..n {
        let cls = class_of(&c, i);
        if !wanted.iter().any(|w| w == cls) {
            continue;
        }
        let dim = c.dimension(i);
        let k = c.kind_of(i) as isize;
        let j = k - 2;
        let cls_j = if j >= 0 && (j as usize) < n {
            class_of(&c, j as usize)
        } else {
            "-"
        };
        let u = c.extra_u32[i] as usize;
        let cls_u = if u < n { class_of(&c, u) } else { "-" };
        println!(
            "{i}\t{}\t{}\t{}\t{}\t0x{:02x}\t{}\t{}\t{}\t{}\t{j}\t{cls_j}\t{u}\t{cls_u}",
            c.name(i),
            dim.width,
            dim.height,
            c.kind_of(i),
            c.flags_of(i),
            c.extra_u32[i],
            c.attr_of(i),
            cls,
            last_pal[i]
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".into())
        );
    }
}
