//! Classify every picture resource of every container.
//!
//! `analyze <container>...` prints a per-container summary of how many
//! resources match each known layout (32bpp, 8bpp+palette, 8bpp without a
//! palette, stored uncompressed) plus a line for every resource that matches
//! none of them.
//!
//! `analyze --tsv <container>...` prints one tab-separated row per resource
//! instead, for cross-tabulation with awk/sort.

use mmf_extract::container::Container;
use mmf_extract::lz77;
use mmf_extract::picture::PALETTE_BYTES;

fn stride8(width: u16) -> usize {
    ((width as usize) + 3) / 4 * 4
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum Class {
    Raw32,
    Raw8,
    C32,
    C8,
    NoPalette,
    Odd,
    Decompress,
}

impl Class {
    fn name(self) -> &'static str {
        match self {
            Class::Raw32 => "raw32",
            Class::Raw8 => "raw8",
            Class::C32 => "c32",
            Class::C8 => "c8",
            Class::NoPalette => "nopalette",
            Class::Odd => "odd",
            Class::Decompress => "decompress",
        }
    }
}

fn classify(c: &Container, i: usize) -> (Class, usize) {
    let dim = c.dimension(i);
    let (w, h) = (dim.width, dim.height);
    let pixels = w as usize * h as usize;
    let as32 = pixels * 4;
    let as8 = PALETTE_BYTES + stride8(w) * h as usize;
    let bare8 = stride8(w) * h as usize;
    let p = c.payload(i);

    // A zero entry in the decompressed-size table means "stored as is".
    let (data, stored_raw) = if c.sizes[i] == 0 {
        (Some(p.to_vec()), true)
    } else {
        (lz77::decompress(p).ok(), false)
    };

    let Some(data) = data else {
        return (Class::Decompress, p.len());
    };
    let len = data.len();

    let class = if stored_raw {
        if len == as32 {
            Class::Raw32
        } else if len == as8 {
            Class::Raw8
        } else if len == bare8 && pixels > 0 {
            Class::NoPalette
        } else {
            Class::Odd
        }
    } else if len == as32 {
        Class::C32
    } else if len == as8 {
        Class::C8
    } else if len == bare8 && pixels > 0 {
        Class::NoPalette
    } else {
        Class::Odd
    };
    (class, len)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let tsv = args.first().map(|a| a == "--tsv").unwrap_or(false);
    let paths: Vec<String> = args
        .into_iter()
        .filter(|a| a != "--tsv")
        .collect();
    if paths.is_empty() {
        eprintln!("usage: analyze [--tsv] <container>...");
        return;
    }

    for path in &paths {
        let c = match Container::open(path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("{path}: {e:#}");
                continue;
            }
        };
        let stem = std::path::Path::new(path)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let n = c.resource_count as usize;
        let mut counts = [0usize; 7];
        let mut odd_lines: Vec<String> = Vec::new();

        for i in 0..n {
            let (class, len) = classify(&c, i);
            counts[class_index(class)] += 1;
            let dim = c.dimension(i);
            if tsv {
                println!(
                    "{stem}\t{i}\t{}\t{}\t{}\t{}\t0x{:02x}\t{}\t{}\t{}\t{}",
                    c.name(i),
                    dim.width,
                    dim.height,
                    c.kind_of(i),
                    c.flags_of(i),
                    c.sizes[i],
                    c.payload(i).len(),
                    len,
                    class.name()
                );
            } else if class == Class::Odd || class == Class::Decompress {
                let head: String = c.payload(i)
                    .iter()
                    .take(16)
                    .map(|b| format!("{b:02x}"))
                    .collect::<Vec<_>>()
                    .join(" ");
                odd_lines.push(format!(
                    "  #{i}\t{}\t{}\tlen={len} w={} h={} kind={} flags=0x{:02x} size_tbl={} stored={} head: {head}",
                    c.name(i),
                    class.name(),
                    dim.width,
                    dim.height,
                    c.kind_of(i),
                    c.flags_of(i),
                    c.sizes[i],
                    c.payload(i).len()
                ));
            }
        }

        if !tsv {
            println!(
                "{path}: N={n} raw32={} raw8={} c32={} c8={} nopalette={} odd={} decompress={}",
                counts[0],
                counts[1],
                counts[2],
                counts[3],
                counts[4],
                counts[5],
                counts[6]
            );
            for l in odd_lines {
                println!("{l}");
            }
        }
    }
}

fn class_index(c: Class) -> usize {
    match c {
        Class::Raw32 => 0,
        Class::Raw8 => 1,
        Class::C32 => 2,
        Class::C8 => 3,
        Class::NoPalette => 4,
        Class::Odd => 5,
        Class::Decompress => 6,
    }
}
