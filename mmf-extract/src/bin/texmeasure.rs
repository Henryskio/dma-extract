//! Report what the texture decoder does with every shipped payload.
//!
//! Scratch bin. `src/dma3d/texfile.rs` claims a measured split: the smallest
//! palette size K for which every byte past 4*K is a valid index, then an index
//! area that lands on a power of two in all 705 files. This runs that reading
//! over the shipped container and prints what actually happens, including any
//! payload that does not fit.

use mmf_extract::container::Container;
use mmf_extract::dma3d::texfile::{self, Axis};

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: texmeasure <DMA3D.MMX>");
    let raw = std::fs::read(&path).expect("read container");
    let c = Container::parse(&path, raw).expect("open Textures");
    let n = c.resource_count as usize;
    println!("N = {n}");

    let mut ok = 0usize;
    let mut bad = Vec::new();
    let mut dims = std::collections::BTreeMap::new();
    let mut axes: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    let mut pal = std::collections::BTreeMap::new();
    let mut areas = std::collections::BTreeMap::new();
    for i in 0..n {
        let payload = c.payload(i);
        match texfile::parse(payload) {
            Ok(t) => {
                ok += 1;
                *dims.entry((t.width, t.height)).or_insert(0usize) += 1;
                *axes
                    .entry(match t.axis {
                        Axis::Square => "square",
                        Axis::Wide => "wide",
                        Axis::Tall => "tall",
                    })
                    .or_insert(0usize) += 1;
                *pal.entry(t.palette.len()).or_insert(0usize) += 1;
                let mut a = t.indices.len();
                let mut twos = 0;
                while a % 2 == 0 && a > 0 {
                    a /= 2;
                    twos += 1;
                }
                *areas.entry(twos).or_insert(0usize) += 1;
            }
            Err(e) => {
                if bad.len() < 12 {
                    bad.push(format!(
                        "{:>3} {:>28} len={:>6} {}",
                        i,
                        c.name(i),
                        payload.len(),
                        e
                    ));
                } else if bad.len() == 12 {
                    bad.push("   ...".into());
                }
                bad.len().min(13);
            }
        }
    }
    println!("parse ok: {ok} of {n}");
    println!("\naxis distribution: {:?}", axes);
    let mut dv: Vec<((usize, usize), usize)> = dims.into_iter().collect();
    dv.sort_by_key(|(_, x)| std::cmp::Reverse(*x));
    println!(
        "top dimensions: {:?}",
        dv.into_iter().take(14).collect::<Vec<_>>()
    );
    let mut pv: Vec<(usize, usize)> = pal.into_iter().collect();
    pv.sort_by_key(|(_, x)| std::cmp::Reverse(*x));
    println!(
        "top palette sizes: {:?}",
        pv.into_iter().take(10).collect::<Vec<_>>()
    );
    let mut av: Vec<(usize, usize)> = areas.into_iter().collect();
    av.sort();
    println!("area as a power of two (2^m): {:?}", av);
    if !bad.is_empty() {
        println!("\nfailures:");
        for b in &bad {
            println!("   {b}");
        }
    }

    // A payload's index area must be a power of two, which is the claim the
    // module makes; check it directly rather than through the exponent map.
    let mut not_pow2 = 0usize;
    for i in 0..n {
        let Ok(t) = texfile::parse(c.payload(i)) else {
            continue;
        };
        let mut a = t.indices.len();
        if a == 0 || (a & (a - 1)) != 0 {
            not_pow2 += 1;
            if not_pow2 < 6 {
                println!("   {} area {} is not a power of two", c.name(i), a);
            }
        }
    }
    println!("\ndecoded textures whose area is not a power of two: {not_pow2}");

    // And how many distinct palette entries are actually *used*?
    let mut waste = 0usize;
    let mut total_entries = 0usize;
    for i in 0..n {
        let Ok(t) = texfile::parse(c.payload(i)) else {
            continue;
        };
        let used: std::collections::BTreeSet<u8> = t.indices.iter().copied().collect();
        total_entries += t.palette.len();
        if used.len() < t.palette.len() {
            waste += 1;
        }
    }
    println!("textures whose palette is longer than the set of indices used: {waste} of {ok}");
    println!(
        "mean palette entries: {:.1}",
        total_entries as f64 / ok as f64
    );
}
