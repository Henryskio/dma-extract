//! Command-line driver: walks an asset folder and extracts what it finds.
//!
//! Two output modes are supported:
//!
//! * **modern** (the default) — pictures become PNG, sounds become FLAC or
//!   WAV. Nothing is lost.
//! * **original** (`--original`) — each resource is written out in the game's
//!   own on-disk format: the decompressed picture payload exactly as the
//!   runtime would hand it to the blitter, or the sound's sample data in
//!   its original encoding.

use anyhow::{Context, Result};
use crate::container::{Container, ContainerKind};
use crate::lz77;
use crate::picture;
use std::path::{Path, PathBuf};

/// Which asset types to extract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssetSelection {
    pub pictures: bool,
    pub sounds: bool,
    pub films: bool,
    pub scripts: bool,
    /// Opaque payload containers (`Blobs`) — written verbatim in both modes.
    pub blobs: bool,
}

impl AssetSelection {
    /// Parse a comma-separated list of type names.
    ///
    /// `"all"` selects everything. Unknown names are an error, so a typo
    /// does not silently extract nothing.
    pub fn parse(names: &[String]) -> Result<Self> {
        let mut sel = Self {
            pictures: false,
            sounds: false,
            films: false,
            scripts: false,
            blobs: false,
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
                    }
                    "pictures" | "picture" | "images" | "image" => sel.pictures = true,
                    "sounds" | "sound" | "audio" => sel.sounds = true,
                    "films" | "film" | "videos" | "video" => sel.films = true,
                    "scripts" | "script" => sel.scripts = true,
                    "blobs" | "blob" => sel.blobs = true,
                    other => anyhow::bail!(
                        "unknown asset type '{other}' (expected all, pictures, sounds, films, scripts or blobs)"
                    ),
                }
            }
        }
        if !(sel.pictures || sel.sounds || sel.films || sel.scripts || sel.blobs) {
            anyhow::bail!("no asset types selected");
        }
        Ok(sel)
    }

    /// Whether a container of this kind should be extracted.
    pub fn wants(&self, kind: ContainerKind) -> bool {
        match kind {
            ContainerKind::Pictures => self.pictures,
            ContainerKind::Sounds => self.sounds,
            ContainerKind::Films => self.films,
            ContainerKind::Scripts => self.scripts,
            ContainerKind::Blobs => self.blobs,
            ContainerKind::Unknown => false,
        }
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
    pub failed: Vec<String>,
}

impl Summary {
    pub fn total(&self) -> usize {
        self.pictures + self.sounds + self.films + self.scripts + self.blobs
    }
}

/// Extract everything the selection asks for out of `asset_dir`.
pub fn run(
    asset_dir: &Path,
    output: &Path,
    selection: &AssetSelection,
    original: bool,
    raw: bool,
) -> Result<Summary> {
    let mut summary = Summary::default();

    let mut entries: Vec<PathBuf> = std::fs::read_dir(asset_dir)
        .with_context(|| format!("reading {}", asset_dir.display()))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    entries.sort();

    for path in entries {
        let container = match Container::open(&path) {
            Ok(c) => c,
            Err(_) => continue, // not one of ours
        };

        if !selection.wants(container.kind) {
            continue;
        }

        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("resource")
            .to_string();

        match container.kind {
            ContainerKind::Pictures => {
                let out_dir = output.join("Pictures").join(&stem);
                let raw_dir = output.join("Raw").join(&stem);
                let report = if original {
                    extract_pictures_original(&container, &out_dir)?
                } else {
                    picture::extract_container(&path, &out_dir, &raw_dir)?
                };
                summary.pictures += report.png_written;
                for (i, name, why) in report.failed {
                    summary
                        .failed
                        .push(format!("{}: resource {i} '{name}': {why}", path.display()));
                }
                if raw && !original {
                    // `extract_container` already wrote the raw payloads.
                }
            }
            ContainerKind::Sounds => {
                let out_dir = output.join("Sounds").join(&stem);
                let report = crate::sound::extract_container(&container, &out_dir, original)?;
                summary.sounds += report.written;
                for (i, name, why) in report.failed {
                    summary
                        .failed
                        .push(format!("{}: resource {i} '{name}': {why}", path.display()));
                }
            }
            ContainerKind::Films => {
                let out_dir = output.join("Films").join(&stem);
                // A film that cannot be fully decoded is reported and skipped
                // rather than aborting the whole run.
                match crate::film::extract_container(&container, &out_dir, original) {
                    Ok(n) => summary.films += n,
                    Err(e) => summary
                        .failed
                        .push(format!("{}: {e:#}", path.display())),
                }
            }
            ContainerKind::Scripts => {
                let out_dir = output.join("Scripts").join(&stem);
                let n = crate::script::extract_container(&container, &out_dir, original)?;
                summary.scripts += n;
            }
            ContainerKind::Blobs => {
                // Opaque payloads. The first game's Blobs container is
                // undecoded, so both modes write the stored bytes verbatim;
                // the extension records which container a payload came from.
                let out_dir = output.join("Blobs").join(&stem);
                std::fs::create_dir_all(&out_dir)?;
                for index in 0..container.resource_count as usize {
                    let name = sanitize(container.name(index), index);
                    let file = out_dir.join(format!("{index:05}_{name}.blb"));
                    if let Err(e) = std::fs::write(&file, container.payload(index)) {
                        summary
                            .failed
                            .push(format!("{}: resource {index} '{name}': {e}", path.display()));
                    } else {
                        summary.blobs += 1;
                    }
                }
            }
            ContainerKind::Unknown => {}
        }
    }

    Ok(summary)
}

/// Write each picture out in the game's original format.
///
/// The file written is the decompressed payload byte for byte — exactly what
/// the game's runtime would hand to its blitter, before any of the optional
/// byte-swap transforms. A resource whose size-table entry is zero was stored
/// uncompressed and is copied through untouched.
pub fn extract_pictures_original(
    container: &Container,
    out_dir: &Path,
) -> Result<picture::ExtractReport> {
    std::fs::create_dir_all(out_dir)?;
    let mut report = picture::ExtractReport::default();
    report.container = container.path.display().to_string();

    for index in 0..container.resource_count as usize {
        let name = container.name(index);
        let safe = sanitize(name, index);
        let file = out_dir.join(format!("{index:05}_{safe}.dat"));

        // A zero in the size table means the payload is stored uncompressed.
        let data = if container.sizes[index] == 0 {
            container.payload(index).to_vec()
        } else {
            match lz77::decompress(container.payload(index)) {
                Ok(d) => d,
                Err(e) => {
                    report.failed.push((index, name.to_string(), e.to_string()));
                    continue;
                }
            }
        };
        std::fs::write(&file, &data)
            .with_context(|| format!("writing {}", file.display()))?;
        report.raw_written += 1;
        report.png_written += 1;
        report.resources.push(picture::ResourceEntry {
            index,
            name: name.to_string(),
            width: container.dimension(index).width,
            height: container.dimension(index).height,
            bytes: data.len(),
            format: picture::PixelFormat::TranspRgb32,
            png: file.display().to_string(),
            raw: file.display().to_string(),
        });
    }

    Ok(report)
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
    eprintln!("  total:    {}", summary.total());
    if !summary.failed.is_empty() {
        eprintln!("  failed:   {}", summary.failed.len());
        for f in summary.failed.iter().take(20) {
            eprintln!("    {f}");
        }
    }
}
