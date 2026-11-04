//! Command-line driver: walks an asset folder and extracts what it finds.
//!
//! Two output modes are supported:
//!
//! * **modern** (the default) — pictures become PNG, sounds become FLAC.
//!   Nothing is lost.
//! * **original** (`--original`) — each resource is written out in the game's
//!   own on-disk format: the decompressed picture payload exactly as the
//!   runtime would hand it to the blitter, or the sound's sample data in
//!   its original encoding.
//!
//! The extraction itself is in Lean (`lean/src/MmfExtract/`): `run` below hands
//! a request to the Lean core and reads its report back, so the parsing, the
//! decoders and the output writers have one home.

use crate::container::{Container, ContainerKind};
use crate::lean;
use crate::picture::Decoded;
use crate::v1;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// The picture decoder to use for `container`.

/// This is the one place that knows which container version needs which
/// decoder, and it belongs to the *reference* implementation: version 1 is the
/// first game's format, whose payloads are compressed with a different routine
/// and whose palettes live outside the payload, so it gets its own path in
/// [`crate::v1`]; every other version uses the shared one.

/// Keeping the choice in the wiring layer is deliberate: neither decoder has
/// to mention the other's container, so editing one cannot change the other's
/// output. The Lean extraction core picks the same two decoders, in
/// `lean/src/MmfExtract/Extract.lean`.
pub fn picture_decoder(container: &crate::container::Container) -> fn(&crate::container::Container, usize) -> Result<Decoded> {
    if container.version == 1 {
        v1::picture::decode
    } else {
        crate::picture::decode
    }
}

/// The payload function to use for `container`.

/// The companion to [`picture_decoder`], for the `--original` mode, which has
/// to write the payload *after* whatever decompression step the container's
/// version applies — the bytes the game's runtime would hand to its blitter.
pub fn picture_payload(
    container: &crate::container::Container,
) -> fn(&crate::container::Container, usize) -> Result<Vec<u8>> {
    if container.version == 1 {
        v1::picture::payload
    } else {
        crate::picture::payload
    }
}

/// Which asset types to extract.
///
/// The Lean core reads the same list, so this struct is the CLI's own view: it
/// is what `--types` was validated against and what the request carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetSelection {
    pictures: bool,
    sounds: bool,
    films: bool,
    scripts: bool,
    /// `Blobs` containers — the first game's `goofy 2.6.3` animation records.
    blobs: bool,
    /// The 3D game's `Models` and `Textures` containers. One flag drives both
    /// because one name ("models") is what the game calls the pair; the
    /// textures ride along since they are the models' material source.
    models: bool,
    /// The names the user asked for, so the Lean core can apply the same rules.
    names: Vec<String>,
}

impl AssetSelection {
    /// Parse a comma-separated list of type names.
    ///
    /// `"all"` selects everything. Unknown names are an error, so a typo does
    /// not silently extract nothing.
    pub fn parse(names: &[String]) -> Result<Self> {
        let mut sel = Self {
            pictures: false,
            sounds: false,
            films: false,
            scripts: false,
            blobs: false,
            models: false,
            names: Vec::new(),
        };
        for raw in names {
            for part in raw.split(',') {
                let name = part.trim().to_ascii_lowercase();
                if name.is_empty() {
                    continue;
                }
                match name.as_str() {
                    "all" => {
                        sel.pictures = true;
                        sel.sounds = true;
                        sel.films = true;
                        sel.scripts = true;
                        sel.blobs = true;
                        sel.models = true;
                    }
                    "pictures" | "picture" | "images" | "image" => sel.pictures = true,
                    "sounds" | "sound" | "audio" => sel.sounds = true,
                    "films" | "film" | "videos" | "video" => sel.films = true,
                    "scripts" | "script" => sel.scripts = true,
                    "blobs" | "blob" => sel.blobs = true,
                    "models" | "model" | "textures" | "texture" | "3d" => sel.models = true,
                    other => anyhow::bail!(
                        "unknown asset type '{other}' (expected all, pictures, sounds, films, scripts, blobs or models)"
                    ),
                }
                if !sel.names.contains(&name) {
                    sel.names.push(name);
                }
            }
        }
        if !(sel.pictures || sel.sounds || sel.films || sel.scripts || sel.blobs || sel.models) {
            anyhow::bail!("no asset types selected");
        }
        Ok(sel)
    }

    /// The names `--types` was given, for the request the Lean core reads.
    pub fn names(&self) -> &[String] {
        &self.names
    }
}

/// What was written, per asset type.
#[derive(Debug, Default)]
pub struct Summary {
    pub pictures: usize,
    pub sounds: usize,
    pub films: usize,
    pub scripts: usize,
    pub blobs: usize,
    /// The 3D game's models and textures, counted together.
    pub models: usize,
    pub failed: Vec<String>,
}

impl Summary {
    pub fn total(&self) -> usize {
        self.pictures + self.sounds + self.films + self.scripts + self.blobs + self.models
    }
}

/// Extract everything the selection asks for out of `asset_dir`.
///
/// The Lean core does the work; this is the request it reads and the report it
/// returns, translated back into the CLI's own [`Summary`]. Every failure the
/// Lean core reports is kept, so the summary prints the same lines whatever
/// half of the program found the problem.
pub fn run(
    asset_dir: &Path,
    output: &Path,
    selection: &AssetSelection,
    original: bool,
    raw: bool,
) -> Result<Summary> {
    let request = lean::Request {
        asset_dir: asset_dir.to_path_buf(),
        output: output.to_path_buf(),
        types: selection.names().to_vec(),
        original,
        raw,
    };
    let report = lean::run(&request)?;

    let mut summary = Summary {
        pictures: report.pictures,
        sounds: report.sounds,
        films: report.films,
        scripts: report.scripts,
        blobs: report.blobs,
        models: report.models,
        failed: Vec::new(),
    };
    if report.total != 0 && report.total != summary.total() {
        // The Lean core counts the same resources this CLI used to count; a
        // mismatch means one of the two drifted, so it is reported rather than
        // silently passed on.
        anyhow::bail!(
            "the Lean core reports {} resources but its per-type counts add up to {}",
            report.total,
            summary.total()
        );
    }
    summary.failed = report.failed;
    // The 3D game's modern model path is the Rust converter's: it turns a
    // payload that carries the measured mesh chain into a glTF, a preview PNG,
    // a `.tail` file and an `.anim.txt`. The Lean core extracts the same
    // container's payloads byte for byte in `--original`, and reports each
    // modern model as a failure naming what is missing, rather than writing a
    // different format under a `.gltf` name. So this is where the halves meet:
    // the Rust driver runs the converter on the container the Lean core just
    // reported, using the textures the Lean core wrote beside it.
    if !original && selection.models {
        convert_models(asset_dir, output, &mut summary)?;
    }
    Ok(summary)
}

/// Turn a resource name into something safe to put in a filename.
pub fn sanitize(name: &str, index: usize) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let cleaned = cleaned.trim_matches('.').to_string();
    if cleaned.is_empty() {
        format!("res{index:05}")
    } else {
        cleaned
    }
}

/// Convert the 3D game's `Models` containers to glTF, the Rust half of the
/// split.
///
/// The Lean core extracts these payloads byte for byte in `--original` and
/// reports each modern model as a failure naming what is missing, rather than
/// writing a different format under a `.gltf` name. So the modern conversion is
/// the Rust converter's, and this is where the halves meet: it runs on the
/// containers the Lean core just reported, using the textures the Lean core
/// wrote beside them.
fn convert_models(asset_dir: &Path, output: &Path, summary: &mut Summary) -> Result<()> {
    let mut entries: Vec<PathBuf> = match std::fs::read_dir(asset_dir) {
        Ok(read) => read
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file())
            .collect(),
        Err(_) => return Ok(()),
    };
    entries.sort();
    for path in entries {
        let container = match Container::open(&path) {
            Ok(c) => c,
            Err(_) => continue, // not one of ours
        };
        if container.kind != ContainerKind::Models {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("resource")
            .to_string();
        let out_dir = output.join("Models").join(&stem);
        // The textures the Lean core wrote beside it, which the converter
        // embeds for the models that name one.
        let texture_dir = output.join("Textures").join(&stem);
        let texture_dir = texture_dir.is_dir().then_some(texture_dir);
        let report = crate::dma3d::extract_models(
            &container,
            &out_dir,
            false,
            texture_dir.as_deref(),
        )
        .with_context(|| format!("converting the models of {} into glTF", path.display()))?;
        summary.models += report.written;
        for (index, name, why) in report.failed {
            summary.failed.push(format!(
                "{}: 3D resource {index} '{name}': {why}",
                path.display()
            ));
        }
    }
    Ok(())
}

/// Print a human-readable summary of what was extracted.
pub fn print_summary(summary: &Summary) {
    eprintln!("Extracted:");
    if summary.pictures > 0 {
        eprintln!("  pictures: {}", summary.pictures);
    }
    if summary.sounds > 0 {
        eprintln!("  sounds:   {}", summary.sounds);
    }
    if summary.films > 0 {
        eprintln!("  films:    {}", summary.films);
    }
    if summary.scripts > 0 {
        eprintln!("  scripts:  {}", summary.scripts);
    }
    if summary.blobs > 0 {
        eprintln!("  blobs:    {}", summary.blobs);
    }
    if summary.models > 0 {
        eprintln!("  models:   {}", summary.models);
    }
    eprintln!("  total:    {}", summary.total());
    if !summary.failed.is_empty() {
        eprintln!("  failed:   {}", summary.failed.len());
        for f in summary.failed.iter().take(20) {
            eprintln!("    {f}");
        }
    }
}
