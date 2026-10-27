//! Colour correctness tests.
//!
//! The picture payload stores each pixel (and each palette entry) as
//! `[transparency, red, green, blue]`, with `alpha = 255 - transparency`.
//! An earlier revision read the bytes as `[blue, alpha, red, green]`, which
//! forced blue to zero on every opaque pixel and tinted the whole corpus
//! yellow/olive. These tests pin the correct order down with named sprites
//! whose colours are not in dispute, and check the two other payload layouts
//! (paletted 8bpp and bare 8bpp coverage masks).
//!
//! The final test is a corpus-wide scan of the extracted PNGs (it skips when
//! the corpus is not present).

use mmf_extract::container::Container;
use mmf_extract::picture::{self, PixelFormat};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Where the game's assets live.
fn raw_assets_dir() -> Option<PathBuf> {
    // The asset folder is normally mounted at /RawAssets; point DMA_ASSETS at
    // another copy if needed.
    let extra = std::env::var("DMA_ASSETS").unwrap_or_default();
    for candidate in ["/RawAssets", extra.as_str()] {
        let p = PathBuf::from(candidate);
        if p.is_dir() {
            return Some(p);
        }
    }
    None
}

fn open(assets: &Path, name: &str) -> Container {
    Container::open(assets.join(name)).expect("open container")
}

/// Most common opaque RGB triple, with its count.
fn top_opaque(rgba: &[u8]) -> ([u8; 3], usize) {
    let mut hist: HashMap<[u8; 3], usize> = HashMap::new();
    for px in rgba.chunks_exact(4) {
        if px[3] == 255 {
            *hist.entry([px[0], px[1], px[2]]).or_default() += 1;
        }
    }
    hist.into_iter()
        .max_by_key(|&(_, n)| n)
        .expect("at least one opaque pixel")
}

fn count_opaque(rgba: &[u8]) -> usize {
    rgba.chunks_exact(4).filter(|px| px[3] == 255).count()
}

fn count_matching(rgba: &[u8], f: impl Fn([u8; 4]) -> bool) -> usize {
    rgba.chunks_exact(4)
        .map(|px| [px[0], px[1], px[2], px[3]])
        .filter(|&px| f(px))
        .count()
}

/// `ITXTmustard24` is a mustard-coloured sprite. Its dominant opaque pixel
/// must decode to a bright yellow, not the sky blue the old channel order
/// produced.
#[test]
fn mustard_sprite_decodes_to_yellow() {
    let Some(assets) = raw_assets_dir() else {
        eprintln!("skipping: game assets not present");
        return;
    };
    let app = open(&assets, "app.MMP");
    let d = picture::decode(&app, 128).expect("decode mustard");
    assert_eq!(d.format, PixelFormat::TranspRgb32);
    assert!(count_opaque(&d.rgba) > 10_000);

    let (top, n) = top_opaque(&d.rgba);
    assert!(
        top[0] >= 240 && top[1] >= 210 && top[2] <= 130,
        "ITXTmustard24's dominant opaque colour is {top:?} ({n} px), expected a bright yellow \
         with high R and G and low B"
    );

    // The blue channel must be real: the old bug wrote B = 0 everywhere.
    let blueish = count_matching(&d.rgba, |p| p[3] == 255 && p[2] > 0);
    assert!(
        blueish > count_opaque(&d.rgba) / 2,
        "only {blueish} opaque pixels have a non-zero blue channel"
    );
}

/// `ITXTcarrot24` contains green foliage; green pixels prove the green byte
/// is not being reinterpreted as blue.
#[test]
fn carrot_sprite_has_green_foliage() {
    let Some(assets) = raw_assets_dir() else {
        eprintln!("skipping: game assets not present");
        return;
    };
    let app = open(&assets, "app.MMP");
    let d = picture::decode(&app, 124).expect("decode carrot");
    assert_eq!(d.format, PixelFormat::TranspRgb32);

    let green = count_matching(&d.rgba, |p| {
        p[3] == 255 && p[1] > 60 && p[1] > p[0].saturating_add(40) && p[1] > p[2].saturating_add(40)
    });
    assert!(
        green >= 100,
        "ITXTcarrot24 has only {green} green foliage pixels; the colour channels look wrong"
    );
}

/// `ISbigBALc` is a bunch of coloured balloons: it must contain both warm
/// colours and a genuinely blue/cyan one.
#[test]
fn balloon_sprite_has_warm_and_blue_colours() {
    let Some(assets) = raw_assets_dir() else {
        eprintln!("skipping: game assets not present");
        return;
    };
    let stmp1 = open(&assets, "stmp1.MMP");
    let d = picture::decode(&stmp1, 0).expect("decode balloons");
    assert_eq!(d.format, PixelFormat::TranspRgb32);

    let warm = count_matching(&d.rgba, |p| p[3] == 255 && p[0] > 200 && p[2] < 160);
    let blue = count_matching(&d.rgba, |p| p[3] == 255 && p[2] > 200 && p[0] < 100);

    assert!(warm > 1000, "only {warm} warm balloon pixels");
    assert!(
        blue > 100,
        "only {blue} blue/cyan balloon pixels; under the old order blue was always 0"
    );
}

/// `IStriFIRa01` is an 8bpp paletted fire stamp: its palette contains an
/// orange entry that must render as orange, not blue.
#[test]
fn fire_stamp_renders_orange() {
    let Some(assets) = raw_assets_dir() else {
        eprintln!("skipping: game assets not present");
        return;
    };
    let stmp1 = open(&assets, "stmp1.MMP");
    let d = picture::decode(&stmp1, 3319).expect("decode fire stamp");
    assert_eq!(d.format, PixelFormat::Paletted8);

    let orange = count_matching(&d.rgba, |p| {
        p[3] == 255 && p[0] > 200 && (80..=200).contains(&p[1]) && p[2] < 100
    });
    assert!(
        orange >= 100,
        "IStriFIRa01 has only {orange} orange pixels; its palette is being read in the wrong order"
    );
}

/// A bare 8bpp payload is a coverage mask: white RGB with the byte as alpha.
/// `ITeraserPHASE_A05_1` is a uniform half-strength eraser field, and
/// `MarkerStrokeBANANA04` is a banana-shaped coverage ramp.
#[test]
fn bare_8bpp_is_a_white_coverage_mask() {
    let Some(assets) = raw_assets_dir() else {
        eprintln!("skipping: game assets not present");
        return;
    };
    let hd = open(&assets, "HD.MMP");

    for index in [1316usize, 5789] {
        let d = picture::decode(&hd, index).expect("decode mask");
        assert_eq!(d.format, PixelFormat::AlphaMask8);

        let mut alphas: Vec<u8> = d
            .rgba
            .chunks_exact(4)
            .map(|px| {
                assert_eq!(
                    [px[0], px[1], px[2]],
                    [255, 255, 255],
                    "mask pixel is not white: {:?}",
                    &px[..4]
                );
                px[3]
            })
            .collect();

        alphas.sort_unstable();
        alphas.dedup();

        // The banana stamp has a full antialiased ramp (and both extremes);
        // the uniform eraser field is dominated by one value but still has
        // several coverage levels.
        if index == 1316 {
            assert!(
                alphas.len() > 20,
                "banana mask has only {} alpha levels",
                alphas.len()
            );
            assert!(alphas.contains(&0), "mask has no fully transparent pixels");
            assert!(alphas.contains(&255), "mask has no fully opaque pixels");
        } else {
            assert!(
                alphas.len() > 5,
                "eraser mask has only {} alpha levels",
                alphas.len()
            );
        }
    }
}

/// Corpus-wide scan of the extracted PNGs: the old channel-order bug made
/// blue exactly 0 on *every* opaque pixel. In correctly decoded art the
/// fraction of opaque pixels with a zero blue channel should be comparable to
/// the red and green fractions.
///
/// Scans the extracted corpus if it is present and skips otherwise.
#[test]
fn corpus_has_no_systematic_zero_blue_channel() {
    let dir = std::env::var("MMF_PNG_DIR")
        .unwrap_or_else(|_| "/workspace/extracted/Pictures".to_string());
    let dir = PathBuf::from(dir);
    if !dir.is_dir() {
        eprintln!("skipping: {} not present", dir.display());
        return;
    }

    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().and_then(|e| e.to_str()) == Some("png") {
                out.push(p);
            }
        }
    }

    let mut files = Vec::new();
    walk(&dir, &mut files);
    files.sort();
    assert!(files.len() > 1000, "only {} PNGs found", files.len());

    let mut images = 0u64;
    let mut opaque = 0u64;
    let (mut r0, mut g0, mut b0) = (0u64, 0u64, 0u64);

    for path in &files {
        let Ok(file) = std::fs::File::open(path) else { continue };
        let mut reader =
            match png::Decoder::new(std::io::BufReader::new(file)).read_info() {
                Ok(r) => r,
                Err(_) => continue,
            };
        let (w, h) = {
            let i = reader.info();
            (i.width, i.height)
        };
        let mut buf = vec![0u8; (w as usize * h as usize * 4) + 4096];
        let Ok(info) = reader.next_frame(&mut buf) else { continue };
        images += 1;
        for px in buf[..info.buffer_size()].chunks_exact(4) {
            if px[3] < 128 {
                continue;
            }
            opaque += 1;
            r0 += (px[0] == 0) as u64;
            g0 += (px[1] == 0) as u64;
            b0 += (px[2] == 0) as u64;
        }
    }

    assert!(images > 1000, "only {images} images decoded");
    assert!(opaque > 1_000_000, "only {opaque} opaque pixels");

    let pct = |n: u64| n as f64 * 100.0 / opaque as f64;
    let (rp, gp, bp) = (pct(r0), pct(g0), pct(b0));
    eprintln!(
        "images={images} opaque={opaque} zero-channel R={rp:.2}% G={gp:.2}% B={bp:.2}%"
    );

    assert!(
        bp < 15.0,
        "blue is zero on {bp:.2}% of opaque pixels — the channel-order bug is back"
    );
    assert!(
        (bp - rp).abs() < 8.0 && (bp - gp).abs() < 8.0,
        "zero-blue ({bp:.2}%) is far more common than zero-red ({rp:.2}%) or zero-green ({gp:.2}%)"
    );
}
