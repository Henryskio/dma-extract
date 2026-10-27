//! Print a coarse character map of an indexed picture's index values.
//!
//! `grid <container> <index>` prints one character per pixel: `.` for index
//! 0, `:` for 1-63, `+` for 64-191 and `#` for 192-255. This makes it easy to
//! see whether an 8bpp payload is a smooth coverage mask or flat colour
//! regions without rendering it.

use mmf_extract::container::Container;
use mmf_extract::lz77;
use mmf_extract::picture::PALETTE_BYTES;

fn stride8(width: u16) -> usize {
    ((width as usize) + 3) / 4 * 4
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: grid <container> <index>");
        return;
    }
    let c = match Container::open(&args[0]) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}: {e:#}", args[0]);
            return;
        }
    };
    let i: usize = args[1].parse().expect("index");
    let dim = c.dimension(i);
    let data = if c.sizes[i] == 0 {
        c.payload(i).to_vec()
    } else {
        match lz77::decompress(c.payload(i)) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("decompress: {e:#}");
                return;
            }
        }
    };
    let has_pal = data.len() == PALETTE_BYTES + stride8(dim.width) * dim.height as usize;
    let idx = if has_pal {
        &data[PALETTE_BYTES..]
    } else {
        &data[..]
    };
    let stride = stride8(dim.width);
    println!(
        "#{i} '{}' {}x{} palette={}",
        c.name(i),
        dim.width,
        dim.height,
        has_pal
    );
    for y in 0..dim.height as usize {
        let mut line = String::with_capacity(dim.width as usize);
        for x in 0..dim.width as usize {
            let v = idx.get(y * stride + x).copied().unwrap_or(0);
            line.push(match v {
                0 => '.',
                1..=63 => ':',
                64..=191 => '+',
                _ => '#',
            });
        }
        println!("{line}");
    }
}
