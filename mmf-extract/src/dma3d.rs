//! The third game's 3D assets: the `Models` and `Textures` container kinds.
//!
//! *Disney Creazioni in 3D* / Disney's Magic Artist 3D ships its own
//! containers in the same `MMFW` family as the games already supported --
//! same 16-byte signature block, same endian marker at `0x10`, same version
//! word at `0x12` (**version 3**, the container version the fourth game's
//! build uses), same `4*(N+1)` offset table and same 32-byte names. Its
//! engine names itself `MM3D*` in the executable's RTTI, and the two new
//! kinds are selected by the tag in the signature: `MMFW Models` and
//! `MMFW Textures`.
//!
//! What is *not* shared is the per-kind metadata region between the names
//! table and the first payload, which every other kind sizes differently.
//! Both new kinds are measured below, on the shipped containers.
//!
//! | container | tag | version | N | region |
//! |---|---|---|---|---|
//! | `DMA3D.MMM` | `Models` | 3 | 460 | `4*N + 100` = 1940 |
//! | `DMA3D.MMX` | `Textures` | 3 | 705 | `6*N + 2` = 4232 |
//!
//! The tables were verified structurally rather than by arithmetic alone:
//! `offsets[0]` lands exactly at `names_end + region_len` for both, the
//! `4*N` u32 block at the head of each region holds Mac-style timestamps in
//! the `0xb5..0xb5` range the sound containers use, and `offsets[N]` equals
//! the file length in every container measured.
//!
//! ## What is measured and what is not
//!
//! **Measured** (this module): the skeleton above, the region sizes, the
//! 460/705 resource counts, and the byte-exact payload of every resource.
//!
//! **Not yet decoded** (the honest state): the *payload* formats. The model
//! payloads begin with the six bytes `mod II` and then a little-endian
//! stream of `u32`s and IEEE floats -- a self-describing object graph, whose
//! records recur (a `mod II` header appears again ~100 bytes into
//! `createAmbientLight.mod`). The texture payloads are 4-byte-aligned and
//! carry a 256-entry RGBA prefix in most resources, but not all (a 260-byte
//! resource cannot hold one), so the per-resource form is selected by
//! something in the metadata that is not yet identified. Until the reader in
//! `DMA3D.exe` is traced, [`extract_models`] and [`extract_textures`] write
//! the payload **verbatim**: lossless, byte-exact, and honest about what has
//! not been decoded. See `docs/DMA3D.md`.

use crate::cli::sanitize;
use crate::container::Container;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Length of the metadata region of a `Models` container: `4 * N` u32
/// timestamps followed by a 100-byte trailer.
///
/// Measured on `DMA3D.MMM` (N=460): the region between `names_end` (16602)
/// and `offsets[0]` (18542) is 1940 bytes and `4*460 + 100 == 1940`.
///
/// The trailer is not a per-resource array -- its content is a *reference to
/// another container*: `u16 2`, `u16 1`, `u32 0`, the 4-byte timestamp of the
/// textures container, a 30-byte NUL-padded name field holding `DMA3D.MMX`,
/// then 58 bytes of `0xff` padding. That is the models' record of which
/// texture file their materials come from. It is measured, not decoded.
pub fn models_meta_len(n: usize) -> usize {
    4 * n + MODELS_META_TRAILER
}

/// The byte length of the `Models` trailer described in [`models_meta_len`].
pub const MODELS_META_TRAILER: usize = 100;

/// Length of the metadata region of a `Textures` container: `4 * N` u32
/// timestamps, `2 * N` bytes of two N-byte arrays, and 2 bytes of padding.
///
/// Measured on `DMA3D.MMX` (N=705): the region is 4232 bytes and
/// `6*705 + 2 == 4232`. The two N-byte arrays are not yet identified; they
/// are read here as `attr` and `flags`, the same positions the picture
/// containers use, and that reading is a hypothesis the payload decoder has
/// not confirmed.
pub fn textures_meta_len(n: usize) -> usize {
    6 * n + 2
}

/// One resource written by [`extract_models`] or [`extract_textures`].
#[derive(Debug, Clone)]
pub struct Extracted3D {
    pub index: usize,
    pub name: String,
    pub path: PathBuf,
    pub bytes: usize,
}

/// What happened while extracting one 3D container.
#[derive(Debug, Default)]
pub struct Extract3DReport {
    pub written: usize,
    pub failed: Vec<(usize, String, String)>,
    pub resources: Vec<Extracted3D>,
}

/// Extract every model payload of a `Models` container.
///
/// Each resource is written as a `.mod` file holding the payload exactly as
/// the container stores it. `_original` is accepted for the uniform dispatch
/// signature in `cli.rs` and deliberately ignored: the modern path is a
/// passthrough while the payload format is being decoded, so the two modes
/// write the *same* bytes today. `tests/game1_payloads.rs`-style coverage for
/// this contract lives in `tests/dma3d.rs`, which asserts both the byte-exact
/// `--original` output and that the modern path agrees with it -- that second
/// assertion is the tripwire that fires when a real decoder lands.
pub fn extract_models(
    container: &Container,
    out_dir: &Path,
    _original: bool,
) -> Result<Extract3DReport> {
    extract(container, out_dir, "mod")
}

/// Extract every texture payload of a `Textures` container.
///
/// See [`extract_models`] for why the payload is written unchanged.
pub fn extract_textures(
    container: &Container,
    out_dir: &Path,
    _original: bool,
) -> Result<Extract3DReport> {
    extract(container, out_dir, "tex")
}

fn extract(container: &Container, out_dir: &Path, extension: &str) -> Result<Extract3DReport> {
    std::fs::create_dir_all(out_dir)?;
    let mut report = Extract3DReport::default();

    for index in 0..container.resource_count as usize {
        let name = container.name(index);
        let payload = container.payload(index);
        let stem = output_stem(container, index);
        let path = out_dir.join(format!("{index:05}_{stem}.{extension}"));

        let written = std::fs::write(&path, payload)
            .with_context(|| format!("writing {}", path.display()));
        if let Err(error) = written {
            eprintln!(
                "warning: {}: 3D resource {index} '{name}': {error:#}",
                container.path.display()
            );
            report.failed.push((index, name.to_string(), error.to_string()));
            continue;
        }

        report.written += 1;
        report.resources.push(Extracted3D {
            index,
            name: name.to_string(),
            path,
            bytes: payload.len(),
        });
    }

    Ok(report)
}

/// Build a filename stem from the resource name, dropping the source
/// extension so the output extension is unambiguous.
fn output_stem(container: &Container, index: usize) -> String {
    sanitize(container.name(index), index)
}
