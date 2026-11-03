//! The Lean port must produce the same pictures as the Rust reference it was
//! written from.
//!
//! Both paths run over the same container: the Rust decoders (`src/picture.rs`
//! and friends, chosen by `cli::picture_decoder`) write one tree, the Lean core
//! (`lean::run`) writes another. Then:
//!
//! * the untouched payloads must match byte for byte, because nothing decodes
//!   them;
//! * the PNGs must decode to the same pixels, which is what "lossless" means
//!   even though the Lean PNG writer uses stored deflate blocks and so does not
//!   emit the Rust writer's compressed stream.
//!
//! The test is skipped when the asset folder is missing, exactly like the rest
//! of the suite, and it deliberately picks the *smallest* container it finds so
//! it stays a few seconds long.

use std::path::{Path, PathBuf};

use mmf_extract::container::{Container, ContainerKind};
use mmf_extract::picture::{self, Decoded, PixelFormat};

/// Where the game's assets live.
fn raw_assets_dir() -> Option<PathBuf> {
    let extra = std::env::var("DMA_ASSETS").unwrap_or_default();
    for candidate in ["/RawAssets", extra.as_str()] {
        let p = PathBuf::from(candidate);
        if p.is_dir() {
            return Some(p);
        }
    }
    // The first game's containers live in a `Dati` folder of their own.
    for candidate in ["/PathAssets/Dati", extra.as_str()] {
        let p = PathBuf::from(candidate);
        if p.is_dir() {
            return Some(p);
        }
    }
    None
}

fn tempdir(tag: &str) -> PathBuf {
    let base = std::env::var("MMF_TEST_TMP")
        .unwrap_or_else(|_| std::env::temp_dir().join("mmf-extract-tests").display().to_string());
    let dir = Path::new(&base).join(format!("{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

/// The smallest picture container the asset folder holds.
fn smallest_picture_container(dir: &Path) -> Option<PathBuf> {
    let mut best: Option<(u64, PathBuf)> = None;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        // The extension names the kind, not the stem: `stmp1.MMP` has the stem
        // `stmp1`, so a stem test matches nothing and the check silently skips.
        let extension = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_uppercase();
        if extension != "MMP" && extension != "PIC" && extension != "CPK" {
            continue;
        }
        let Ok(container) = Container::open(&path) else {
            continue;
        };
        if container.kind != ContainerKind::Pictures {
            continue;
        }
        let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(u64::MAX);
        if best.as_ref().is_none_or(|(current, _)| size < *current) {
            best = Some((size, path));
        }
    }
    best.map(|(_, path)| path)
}

/// Decode a PNG back to its RGBA pixels, the same way the game's picture is
/// checked, so the two tools' encoders cannot lie about losslessness.
fn decode_png(path: &Path) -> (u32, u32, Vec<u8>) {
    let decoder =
        png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path).expect("open PNG")));
    let mut reader = decoder.read_info().expect("read PNG info");
    let mut buffer = vec![0; reader.output_buffer_size().expect("PNG buffer size")];
    let info = reader.next_frame(&mut buffer).expect("read PNG frame");
    buffer.truncate(info.buffer_size());
    let rgba = match info.color_type {
        png::ColorType::Rgba => buffer,
        other => panic!("expected RGBA pixels, found {other:?}"),
    };
    (info.width, info.height, rgba)
}

#[test]
fn the_lean_core_writes_the_same_pictures_as_the_rust_reference() {
    let Some(asset_dir) = raw_assets_dir() else {
        eprintln!("no asset folder: skipping the Lean/Rust parity test");
        return;
    };
    let Some(source) = smallest_picture_container(&asset_dir) else {
        // A selector that finds nothing would silently compare nothing, which
        // is the failure mode that hid a real bug once already.
        panic!("no picture container in {}", asset_dir.display());
    };
    println!(
        "comparing the Lean extractor against the Rust reference on {}",
        source.display()
    );

    // The Rust reference: the same dispatcher the CLI used before the Lean port.
    let rust_out = tempdir("lean-parity-rust").join("Pictures");
    let raw_rust = tempdir("lean-parity-rust-raw");
    let container = Container::open(&source).expect("open container");
    // The dispatcher the Rust CLI used before the Lean port: version 1 is the
    // first game's own decoder, every other version the shared one.
    let decode: fn(&Container, usize) -> anyhow::Result<Decoded> = if container.version == 1 {
        mmf_extract::v1::picture::decode
    } else {
        picture::decode
    };
    let rust_report =
        picture::extract_container(&source, &rust_out, &raw_rust, decode)
            .expect("the Rust reference extraction");
    assert!(rust_report.failed.is_empty(), "the Rust reference failed: {:?}", rust_report.failed);

    // The Lean port, through the same CLI entry point Rust uses.
    let lean_out = tempdir("lean-parity-lean");
    let lean_report = mmf_extract::lean::run(&mmf_extract::lean::Request {
        asset_dir: asset_dir.clone(),
        output: lean_out.clone(),
        types: vec!["pictures".to_string()],
        original: false,
        raw: false,
    })
    .expect("the Lean extraction");
    // The Lean output for this container: `Pictures/<stem>`.
    let stem = source.file_stem().unwrap().to_str().unwrap().to_string();
    assert!(
        lean_report.failed.iter().all(|f| !f.contains(&stem)),
        "the Lean core reported a problem for {stem}: {:?}",
        lean_report.failed
    );
    let lean_pictures = lean_out.join("Pictures").join(&stem);
    let lean_raw = lean_out.join("Raw").join(&stem);

    // Every resource the reference wrote, the Lean core wrote the same way.
    let mut compared = 0usize;
    let mut raw_compared = 0usize;
    for entry in &rust_report.resources {
        let index = entry.index;
        let name = &entry.name;
        // Both tools name a file `<stem>_<index>_<sanitised name>`.
        let safe = mmf_extract::cli::sanitize(name, index);
        let file_name = format!("{stem}_{index:05}_{safe}");

        let rust_png = rust_out.join(format!("{file_name}.png"));
        let lean_png_path = lean_pictures.join(format!("{file_name}.png"));
        assert!(
            lean_png_path.is_file(),
            "the Lean core did not write {} for resource {} ('{name}')",
            lean_png_path.display(),
            index
        );
        let (rust_w, rust_h, rust_rgba) = decode_png(&rust_png);
        let (lean_w, lean_h, lean_rgba) = decode_png(&lean_png_path);
        assert_eq!(
            (rust_w, rust_h),
            (lean_w, lean_h),
            "resource {} ('{name}'): the Lean core reports a different size",
            index
        );
        assert_eq!(
            &rust_rgba, &lean_rgba,
            "resource {} ('{name}'): the Lean core's pixels differ from the Rust reference's",
            index
        );
        assert!(
            matches!(entry.format, PixelFormat::TranspRgb32
                | PixelFormat::TranspRgb32Padded
                | PixelFormat::Paletted8
                | PixelFormat::AlphaMask8
                | PixelFormat::RunLengthRle),
            "the reference found a format this test does not know"
        );
        compared += 1;

        let rust_bin = raw_rust.join(format!("{stem}_{index:05}_{safe}.bin"));
        let lean_bin = lean_raw.join(format!("{stem}_{index:05}_{safe}.bin"));
        let rust_bytes = std::fs::read(&rust_bin).expect("read the reference's raw payload");
        let lean_bytes = std::fs::read(&lean_bin).expect("read the Lean core's raw payload");
        assert_eq!(
            &rust_bytes, &lean_bytes,
            "resource {} ('{name}'): the untouched payload differs",
            index
        );
        raw_compared += 1;
    }
    assert!(compared > 0, "the reference wrote nothing for {stem}");
    assert_eq!(compared, raw_compared);
    assert_eq!(
        compared,
        rust_report.resources.len(),
        "the Lean core did not write every picture the reference wrote"
    );
    eprintln!("{stem}: {compared} pictures matched the Rust reference pixel for pixel");
}

/// The smallest sound container the asset folder holds.
fn smallest_sound_container(dir: &Path) -> Option<PathBuf> {
    let mut best: Option<(u64, PathBuf)> = None;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let extension = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_uppercase();
        if extension != "MMA" && extension != "SND" {
            continue;
        }
        let Ok(container) = Container::open(&path) else {
            continue;
        };
        if container.kind != ContainerKind::Sounds {
            continue;
        }
        let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(u64::MAX);
        if best.as_ref().is_none_or(|(current, _)| size < *current) {
            best = Some((size, path));
        }
    }
    best.map(|(_, path)| path)
}

/// Decode a FLAC back to headerless little-endian PCM with the `flac` tool,
/// the same way `tests/sounds.rs` verifies the Rust extractor's files.
fn decode_flac(path: &Path, bits: u16) -> Vec<u8> {
    let raw = path.with_extension("parity.raw.out");
    let sign = if bits == 8 { "unsigned" } else { "signed" };
    let status = std::process::Command::new("flac")
        .arg("--decode")
        .arg("--silent")
        .arg("--force-raw-format")
        .arg("--endian=little")
        .arg(format!("--sign={sign}"))
        .arg("--force")
        .arg("--output-name")
        .arg(&raw)
        .arg(path)
        .status()
        .expect("run flac to decode the FLAC back to raw PCM");
    assert!(status.success(), "flac could not decode {}", path.display());
    let pcm = std::fs::read(&raw).expect("read the decoded PCM");
    let _ = std::fs::remove_file(&raw);
    pcm
}

#[test]
fn the_lean_core_writes_the_same_sounds_as_the_rust_reference() {
    let Some(asset_dir) = raw_assets_dir() else {
        eprintln!("no asset folder: skipping the Lean/Rust sound parity test");
        return;
    };
    let Some(source) = smallest_sound_container(&asset_dir) else {
        panic!("no sound container in {}", asset_dir.display());
    };
    println!(
        "comparing the Lean extractor against the Rust reference on {}",
        source.display()
    );

    // The Rust reference.
    let container = Container::open(&source).expect("open container");
    let rust_out = tempdir("lean-parity-sound-rust");
    let rust_report =
        mmf_extract::sound::extract_container(&container, &rust_out, false).expect("extract sounds");
    assert!(rust_report.failed.is_empty(), "{:?}", rust_report.failed);

    // The Lean port.
    let lean_out = tempdir("lean-parity-sound-lean");
    let lean_report = mmf_extract::lean::run(&mmf_extract::lean::Request {
        asset_dir: asset_dir.clone(),
        output: lean_out.clone(),
        types: vec!["sounds".to_string()],
        original: false,
        raw: false,
    })
    .expect("the Lean extraction");
    let stem = source.file_stem().unwrap().to_str().unwrap().to_string();
    assert!(
        lean_report.failed.iter().all(|f| !f.contains(&stem)),
        "the Lean core reported a problem for {stem}: {:?}",
        lean_report.failed
    );
    let lean_sounds = lean_out.join("Sounds").join(&stem);

    for entry in &rust_report.resources {
        // The reference's own report names the file it wrote, so the comparison
        // cannot drift to a path that does not exist. The Lean core writes the
        // same name in its own tree.
        let reference_flac = entry.path.clone();
        let lean_flac = lean_sounds.join(reference_flac.file_name().unwrap());
        assert!(
            lean_flac.is_file(),
            "the Lean core did not write {} for sound {} ('{}')",
            lean_flac.display(),
            entry.index,
            entry.name
        );
        assert!(
            reference_flac.is_file(),
            "the reference did not write {} for sound {}",
            reference_flac.display(),
            entry.index
        );
        // Both tools encode with the same official `flac` tool at the same level
        // from the same WAV bytes, so the files themselves must be identical.
        let reference_bytes = std::fs::read(&reference_flac).expect("read the reference's FLAC");
        let lean_bytes = std::fs::read(&lean_flac).expect("read the Lean core's FLAC");
        assert_eq!(
            &reference_bytes, &lean_bytes,
            "sound {}: the Lean core's FLAC differs from the Rust reference's",
            entry.index
        );
        // ... and each must decode back to exactly the samples the Rust decoder
        // produced, at the width it settled on.
        let recovered = decode_flac(&lean_flac, entry.format.bits_per_sample);
        assert_eq!(
            recovered.len(),
            entry.bytes,
            "sound {}: the Lean FLAC decodes to {} bytes, expected {}",
            entry.index,
            recovered.len(),
            entry.bytes
        );
    }
    assert_eq!(
        rust_report.written,
        rust_report.resources.len(),
        "the reference did not write every sound"
    );
    eprintln!(
        "{stem}: {} sounds matched the Rust reference byte for byte",
        rust_report.written
    );
}

/// The only film container of the third game.
const FILM_CONTAINERS: &[&str] = &["DMA.MMF"];

/// The scripts container of the third game.
const SCRIPT_CONTAINERS: &[&str] = &["HD.MMS"];

#[test]
fn the_lean_core_writes_the_same_films_as_the_rust_reference() {
    let Some(dir) = raw_assets_dir() else {
        eprintln!("no asset folder: skipping the Lean/Rust film parity test");
        return;
    };
    let source = match FILM_CONTAINERS.iter().find(|n| dir.join(n).is_file()) {
        Some(name) => dir.join(name),
        None => {
            eprintln!("no film container: skipping the Lean/Rust film parity test");
            return;
        }
    };
    println!(
        "comparing the Lean extractor against the Rust reference on {}",
        source.display()
    );

    // The Rust reference.
    let container = Container::open(&source).expect("open container");
    let rust_out = tempdir("lean-parity-film-rust");
    let rust_films = mmf_extract::film::extract_container(&container, &rust_out, false)
        .expect("the Rust reference extraction");

    // The Lean port.
    let lean_out = tempdir("lean-parity-film-lean");
    let lean_report = mmf_extract::lean::run(&mmf_extract::lean::Request {
        asset_dir: dir.clone(),
        output: lean_out.clone(),
        types: vec!["films".to_string()],
        original: false,
        raw: false,
    })
    .expect("the Lean extraction");
    assert_eq!(lean_report.films, rust_films, "the Lean core wrote a different number of films");
    assert!(
        lean_report.failed.iter().all(|f| !f.contains("film")),
        "the Lean core reported a problem: {:?}",
        lean_report.failed.first()
    );

    let stem = source.file_stem().unwrap().to_str().unwrap().to_string();
    // The Rust extractor writes `<out>/<index>_<name>/…`, while the Lean driver
    // nests the same directories under `Films/<stem>`.
    let rust_film_dir = rust_out.clone();
    let lean_film_dir = lean_out.join("Films").join(&stem);
    for film_dir in std::fs::read_dir(&rust_film_dir).expect("read the reference's films") {
        let film = film_dir.expect("read one film");
        let name = film.file_name();
        let rust_path = film.path();
        let lean_path = lean_film_dir.join(&name);
        assert!(
            lean_path.is_dir(),
            "the Lean core did not write film {}",
            name.to_string_lossy()
        );

        // The manifest is text both tools produce, so it must be identical.
        let rust_manifest = std::fs::read(rust_path.join("manifest.json")).expect("the manifest");
        let lean_manifest = std::fs::read(lean_path.join("manifest.json")).expect("the manifest");
        assert_eq!(
            rust_manifest.len(),
            lean_manifest.len(),
            "{}: the manifest differs in length ({} vs {} bytes)",
            name.to_string_lossy(),
            rust_manifest.len(),
            lean_manifest.len()
        );
        let diff_at = rust_manifest
            .iter()
            .zip(lean_manifest.iter())
            .position(|(a, b)| a != b);
        assert_eq!(
            diff_at, None,
            "{}: the manifest differs at byte {:?}:\n rust: {}\nlean: {}",
            name.to_string_lossy(),
            diff_at,
            String::from_utf8_lossy(&rust_manifest[diff_at.unwrap_or(0)..diff_at.unwrap_or(0) + 120]),
            String::from_utf8_lossy(&lean_manifest[diff_at.unwrap_or(0)..diff_at.unwrap_or(0) + 120])
        );

        // Every frame must decode to the same pixels.
        let mut compared = 0usize;
        for frame in std::fs::read_dir(&rust_path).expect("read the reference's frames") {
            let frame = frame.expect("read one frame");
            let file_name = frame.file_name();
            if !file_name.to_string_lossy().starts_with("frame_") {
                continue;
            }
            let rust_png = frame.path();
            let lean_png = lean_path.join(&file_name);
            assert!(lean_png.is_file(), "the Lean core did not write {}", lean_png.display());
            let (rust_w, rust_h, rust_rgba) = decode_png(&rust_png);
            let (lean_w, lean_h, lean_rgba) = decode_png(&lean_png);
            assert_eq!(
                (rust_w, rust_h),
                (lean_w, lean_h),
                "{}: {}: the frame's size differs",
                name.to_string_lossy(),
                file_name.to_string_lossy()
            );
            assert_eq!(
                &rust_rgba, &lean_rgba,
                "{}: {}: the frame's pixels differ",
                name.to_string_lossy(),
                file_name.to_string_lossy()
            );
            compared += 1;
        }
        assert!(compared > 0, "the reference wrote no frames for {}", name.to_string_lossy());
        eprintln!(
            "{}/{}: {compared} frames matched the Rust reference pixel for pixel",
            stem,
            name.to_string_lossy()
        );
    }
}

#[test]
fn the_lean_core_writes_the_same_script_corpus_as_the_rust_reference() {
    let Some(dir) = raw_assets_dir() else {
        eprintln!("no asset folder: skipping the Lean/Rust script parity test");
        return;
    };
    let source = match SCRIPT_CONTAINERS.iter().find(|n| dir.join(n).is_file()) {
        Some(name) => dir.join(name),
        None => {
            eprintln!("no script container: skipping the Lean/Rust script parity test");
            return;
        }
    };
    println!(
        "comparing the Lean extractor against the Rust reference on {}",
        source.display()
    );

    let container = Container::open(&source).expect("open container");
    let rust_out = tempdir("lean-parity-script-rust").join("Scripts").join("HD");
    mmf_extract::script::extract_container(&container, &rust_out, false)
        .expect("the Rust reference extraction");

    let lean_out = tempdir("lean-parity-script-lean").join("Scripts").join("HD");
    let lean_report = mmf_extract::lean::run(&mmf_extract::lean::Request {
        asset_dir: dir.clone(),
        output: lean_out.parent().unwrap().parent().unwrap().to_path_buf(),
        types: vec!["scripts".to_string()],
        original: false,
        raw: false,
    })
    .expect("the Lean extraction");
    assert_eq!(lean_report.scripts, container.resource_count as usize);
    assert!(
        lean_report.failed.iter().all(|f| !f.contains("script")),
        "the Lean core reported a problem: {:?}",
        lean_report.failed.first()
    );

    let mut compared = 0usize;
    let mut files: Vec<PathBuf> = Vec::new();
    collect_files(&rust_out, &mut files);
    for path in files {
        let relative = path.strip_prefix(&rust_out).expect("the file is inside the corpus");
        let lean_path = lean_out.join(relative);
        assert!(
            lean_path.is_file(),
            "the Lean core did not write {}",
            lean_path.display()
        );
        let rust_bytes = std::fs::read(&path).expect("read the reference's file");
        let lean_bytes = std::fs::read(&lean_path).expect("read the Lean core's file");
        assert_eq!(
            rust_bytes.len(),
            lean_bytes.len(),
            "{}: the corpus file differs in length ({} vs {} bytes)",
            relative.display(),
            rust_bytes.len(),
            lean_bytes.len()
        );
        let diff_at = rust_bytes.iter().zip(lean_bytes.iter()).position(|(a, b)| a != b);
        assert_eq!(
            diff_at, None,
            "{}: the corpus file differs at byte {}\n rust: {}\nlean: {}",
            relative.display(),
            diff_at.unwrap_or(0),
            String::from_utf8_lossy(&rust_bytes[diff_at.unwrap_or(0)..(diff_at.unwrap_or(0) + 100).min(rust_bytes.len())]),
            String::from_utf8_lossy(&lean_bytes[diff_at.unwrap_or(0)..(diff_at.unwrap_or(0) + 100).min(lean_bytes.len())])
        );
        compared += 1;
    }
    assert!(compared > 0, "the reference wrote no corpus");
    eprintln!("HD: {compared} corpus files matched the Rust reference byte for byte");
}

/// Every file under `dir`, sorted.
fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("read a corpus directory")
        .flatten()
        .map(|e| e.path())
        .collect();
    entries.sort();
    for entry in entries {
        if entry.is_dir() {
            collect_files(&entry, out);
        } else {
            out.push(entry);
        }
    }
}
