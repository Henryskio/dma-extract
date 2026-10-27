//! Try candidate second-stage decoders against the odd picture payloads.
//!
//! `oddparse <container>` prints, for every resource whose decoded length
//! matches neither a raw nor an indexed layout, the result of running a set
//! of candidate RLE decoders over the payload and whether the decoded length
//! equals one of the expected pixel-buffer sizes.

use mmf_extract::container::Container;
use mmf_extract::lz77;
use mmf_extract::picture::PALETTE_BYTES;

fn stride8(width: u16) -> usize {
    ((width as usize) + 3) / 4 * 4
}

/// Classic PackBits: `n` in 0..=127 copies `n+1` literal bytes, `n` in
/// 129..=255 repeats the following byte `257-n` times, 128 is a no-op.
fn packbits(src: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < src.len() {
        let n = src[i] as i8 as i32;
        i += 1;
        if n >= 0 {
            let take = (n + 1) as usize;
            if i + take > src.len() {
                return None;
            }
            out.extend_from_slice(&src[i..i + take]);
            i += take;
        } else if n == -128 {
            // no-op
        } else {
            if i >= src.len() {
                return None;
            }
            let b = src[i];
            i += 1;
            out.extend(std::iter::repeat_n(b, (1 - n) as usize));
        }
    }
    Some(out)
}

/// `(count, byte)` pairs; a count of zero is tried as both 1 and 256.
fn rle_count_byte(src: &[u8], zero: u32) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut i = 0;
    while i + 1 < src.len() {
        let n = if src[i] == 0 { zero } else { src[i] as u32 };
        out.extend(std::iter::repeat_n(src[i + 1], n as usize));
        i += 2;
    }
    Some(out)
}

/// `(count, b0, b1, b2)` records; a count of zero is tried as both 1 and 256.
fn rle_count_triple(src: &[u8], zero: u32) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut i = 0;
    while i + 3 < src.len() {
        let n = if src[i] == 0 { zero } else { src[i] as u32 };
        let r = [src[i + 1], src[i + 2], src[i + 3]];
        for _ in 0..n {
            out.extend_from_slice(&r);
        }
        i += 4;
    }
    Some(out)
}

/// `(count, b0, b1, b2, b3)` records.
fn rle_count_quad(src: &[u8], zero: u32) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut i = 0;
    while i + 4 < src.len() {
        let n = if src[i] == 0 { zero } else { src[i] as u32 };
        let r = [src[i + 1], src[i + 2], src[i + 3], src[i + 4]];
        for _ in 0..n {
            out.extend_from_slice(&r);
        }
        i += 5;
    }
    Some(out)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: oddparse <container>");
        return;
    }
    let c = match Container::open(&args[0]) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}: {e:#}", args[0]);
            return;
        }
    };
    let n = c.resource_count as usize;

    for i in 0..n {
        let dim = c.dimension(i);
        let (w, h) = (dim.width, dim.height);
        let pixels = w as usize * h as usize;
        if pixels == 0 {
            continue;
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
            continue; // not an odd resource
        }

        let mut hits: Vec<&str> = Vec::new();
        let mut marks: Vec<String> = Vec::new();
        let cands: [(&str, Option<Vec<u8>>); 6] = [
            ("packbits", packbits(&plain)),
            ("cnt1x0_1", rle_count_byte(&plain, 1)),
            ("cnt1x0_256", rle_count_byte(&plain, 256)),
            ("cnt3x0_1", rle_count_triple(&plain, 1)),
            ("cnt3x0_256", rle_count_triple(&plain, 256)),
            ("cnt4x0_1", rle_count_quad(&plain, 1)),
        ];
        for (name, out) in cands {
            let Some(out) = out else {
                marks.push(format!("{name}:err"));
                continue;
            };
            let len = out.len();
            let hit = targets.iter().position(|&t| t == len);
            if hit.is_some() {
                hits.push(name);
            }
            marks.push(format!("{name}:{len}{}", if hit.is_some() { "*" } else { "" }));
        }

        println!(
            "#{i}\t{}\t{w}x{h}\tplain={}\ttargets={}/{}/{}/{}\t{}\tHITS={}",
            c.name(i),
            plain.len(),
            targets[0],
            targets[1],
            targets[2],
            targets[3],
            marks.join(" "),
            if hits.is_empty() { "-".to_string() } else { hits.join(",") }
        );
    }
}
