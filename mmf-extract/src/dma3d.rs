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

pub mod gltf;
pub mod render;
pub mod modfile;
pub mod texfile;
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
/// Two modes, with different output:
///
/// * `--original` writes each payload byte for byte, and is the mode the
///   `tests/dma3d.rs` round-trip covers.
/// * the modern mode writes a **glTF** for every payload that carries the
///   measured mesh chain, and the payload verbatim for the rest. That split is
///   the honest state: 144 of the 460 payloads carry a mesh
///   (`docs/DMA3D.md` §3.2), and every one of those becomes a viewable model.
///   The 316 that do not -- animation records, lights, UI references -- are
///   written through unchanged. A glTF is accompanied by a `.tail` file with
///   the bytes the mesh decoder does not read, which is what keeps the modern
///   output from being a lossy subset.
///
/// `texture_dir` is where the `Textures` container's PNGs live. A model whose
/// payload names a texture gets it embedded; the rest come out untextured,
/// because no payload that carries a mesh has been found to name one.
pub fn extract_models(
    container: &Container,
    out_dir: &Path,
    original: bool,
    texture_dir: Option<&Path>,
) -> Result<Extract3DReport> {
    if original {
        return extract(container, out_dir, "mod");
    }
    extract_gltf(container, out_dir, texture_dir)
}

/// Extract every texture payload of a `Textures` container.
///
/// `--original` writes the payload byte for byte. The modern mode decodes
/// every one of the 705 textures as the measured indexed image
/// (`docs/DMA3D.md` §2.1) and writes it as a PNG; a texture that does not fit
/// the layout is reported as a failure rather than written as something it is
/// not.
pub fn extract_textures(
    container: &Container,
    out_dir: &Path,
    original: bool,
) -> Result<Extract3DReport> {
    if original {
        return extract(container, out_dir, "tex");
    }
    extract_pngs(container, out_dir)
}

/// The `--original` path: every payload written exactly as stored.
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

/// The modern texture path: decode each payload and write a PNG.
fn extract_pngs(container: &Container, out_dir: &Path) -> Result<Extract3DReport> {
    std::fs::create_dir_all(out_dir)?;
    let mut report = Extract3DReport::default();

    for index in 0..container.resource_count as usize {
        let name = container.name(index);
        let payload = container.payload(index);
        let stem = output_stem(container, index);
        let path = out_dir.join(format!("{index:05}_{stem}.png"));

        let outcome = texfile::parse(payload)
            .map_err(|e| format!("{e:#}"))
            .and_then(|texture| {
                let rgba = texture.to_rgba();
                let png = gltf::encode_png(&rgba, texture.width, texture.height)
                    .map_err(|e| format!("{e:#}"))?;
                Ok((png, texture.width * texture.height * 4))
            })
            .and_then(|(png, _)| {
                std::fs::write(&path, &png)
                    .map(|_| png.len())
                    .map_err(|e| format!("{e:#}"))
            });

        match outcome {
            Ok(bytes) => {
                report.written += 1;
                report.resources.push(Extracted3D {
                    index,
                    name: name.to_string(),
                    path,
                    bytes,
                });
            }
            Err(why) => {
                eprintln!(
                    "warning: {}: texture {index} '{name}': {why}",
                    container.path.display()
                );
                report.failed.push((index, name.to_string(), why));
            }
        }
    }

    Ok(report)
}

/// The modern model path: a glTF per payload that carries a mesh.
fn extract_gltf(
    container: &Container,
    out_dir: &Path,
    texture_dir: Option<&Path>,
) -> Result<Extract3DReport> {
    std::fs::create_dir_all(out_dir)?;
    let mut report = Extract3DReport::default();

    for index in 0..container.resource_count as usize {
        let name = container.name(index);
        let payload = container.payload(index);
        let stem = output_stem(container, index);

        let mesh = match modfile::parse_mesh(payload) {
            Ok(m) => m,
            Err(_) => {
                // No mesh in this payload: written through unchanged so nothing
                // is lost, and counted as a model that is not geometry.
                let path = out_dir.join(format!("{index:05}_{stem}.mod"));
                std::fs::write(&path, payload)
                    .with_context(|| format!("writing {}", path.display()))?;
                report.written += 1;
                report.resources.push(Extracted3D {
                    index,
                    name: name.to_string(),
                    path,
                    bytes: payload.len(),
                });
                continue;
            }
        };

        // Materials: the textures this payload names, embedded from the PNGs
        // the `Textures` container produced. Six payloads name theirs and none
        // of those carries a mesh, so the meshes are untextured until the
        // material record is read out of the payload (`docs/DMA3D.md` §3.4).
        let mut materials = Vec::new();
        for wanted in texture_names_in(payload) {
            let stem = sanitize(wanted.trim_end_matches(".TGA"), 0);
            let path = texture_dir.map(|dir| dir.join(format!("{stem}.png")));
            if let Some(path) = path {
                if let Ok(png) = std::fs::read(&path) {
                    materials.push(gltf::Material {
                        name: wanted.clone(),
                        png,
                    });
                }
            }
        }

        let path = out_dir.join(format!("{index:05}_{stem}.gltf"));
        gltf::write_gltf(&path, name, &mesh, &materials)
            .with_context(|| format!("writing {}", path.display()))?;
        // The undecoded tail goes beside the glTF, verbatim, so the modern
        // output is the mesh *plus* the bytes still closed rather than a
        // lossy subset. `--original` stays the mode that is byte-exact; this
        // is what makes the modern mode not *destructive*.
        gltf::write_tail(
            &out_dir.join(gltf::tail_name(&format!("{index:05}_{stem}.gltf"))),
            &payload[mesh.tail_offset()..],
        )
        .with_context(|| format!("writing the tail of {name}"))?;
        // A preview beside the glTF, so the index page can show what each
        // object is without the reader opening a 3D tool first.
        let preview = render::render(&mesh);
        let preview_path = out_dir.join(format!("{index:05}_{stem}.preview.png"));
        gltf::encode_png(&preview.rgba, render::PREVIEW_PIXELS, render::PREVIEW_PIXELS)
            .and_then(|png| {
                std::fs::write(&preview_path, png)
                    .map_err(|e| anyhow::anyhow!("{e}"))
                    .map_err(Into::into)
            })
            .with_context(|| format!("writing the preview of {name}"))?;
        if preview.covered == 0 {
            eprintln!(
                "warning: {}: model {index} '{name}': the preview covers no pixels",
                container.path.display()
            );
            report
                .failed
                .push((index, name.to_string(), "preview covers no pixels".into()));
        }
        let bytes = std::fs::metadata(&path).map(|m| m.len() as usize).unwrap_or(0);
        report.written += 1;
        report.resources.push(Extracted3D {
            index,
            name: name.to_string(),
            path,
            bytes,
        });
    }

    Ok(report)
}

/// The texture names a model payload carries, in payload order.
///
/// The table is `[u32 count][count * 32-byte NUL-padded names]` (measured on
/// `createBtDraw.mod`, which holds 31 of them). A candidate that does not
/// validate -- a record that is empty, or does not name a `.TGA` -- is
/// rejected, so a stray `TGA` in the geometry is not read as a material.
fn texture_names_in(payload: &[u8]) -> Vec<String> {
    let mut names = Vec::new();
    let mut at = 0usize;
    while at + 4 <= payload.len() {
        let count = u32::from_le_bytes([
            payload[at],
            payload[at + 1],
            payload[at + 2],
            payload[at + 3],
        ]) as usize;
        if !(1..=64).contains(&count) {
            at += 4;
            continue;
        }
        let table = at + 4;
        if table + count * 32 > payload.len() {
            at += 4;
            continue;
        }
        let mut ok = true;
        let mut found = Vec::new();
        for record in 0..count {
            let rec = &payload[table + record * 32..table + (record + 1) * 32];
            let end = rec.iter().position(|&b| b == 0).unwrap_or(rec.len());
            if end == 0 {
                ok = false;
                break;
            }
            let text = String::from_utf8_lossy(&rec[..end]).to_string();
            if !text.contains("TGA") {
                ok = false;
                break;
            }
            found.push(text);
        }
        if ok {
            names.extend(found);
            at = table + count * 32;
        } else {
            at += 4;
        }
    }
    names
}


/// Write the index page that makes the 3D output browsable.
///
/// The modern mode exists for people who have never opened a 3D program, and a
/// folder of 705 `.png`s and 144 `.gltf`s is not browsable on its own: nothing
/// connects a texture to the object it belongs to, and nothing says how to open
/// a glTF. This page is one file that does both, and it is deliberately
/// dependency-free — no script, no stylesheet, no network — so it works by
/// double-clicking it from a local folder, which is how a desktop user will
/// actually open it.
///
/// `texture_dir` and `model_dir` hold the PNGs written by [`extract_textures`]
/// and the glTFs plus previews written by [`extract_models`]. The index goes at
/// `base`, beside both, and refers to them by a relative path.
pub fn write_index(
    base: &Path,
    texture_dir: &Path,
    model_dir: &Path,
) -> Result<usize> {
    let mut files = 0usize;

    // List the two directories, so the page is built from what was written
    // rather than from what the container claims it holds.
    let textures = list_dir(texture_dir, "png");
    let models = list_dir(model_dir, "gltf");
    let previews = list_dir(model_dir, "preview.png");

    let relative_to = |dir: &Path| -> String {
        dir.strip_prefix(base)
            .unwrap_or(dir)
            .to_string_lossy()
            .replace('\\', "/")
    };
    let texture_prefix = relative_to(texture_dir);
    let model_prefix = relative_to(model_dir);

    let mut page = String::with_capacity(64 * 1024);
    page.push_str(INDEX_HEAD);
    page.push_str(&format!(
        "<h2>Objects <span class=\"count\">{}</span></h2>\n",
        models.len()
    ));
    page.push_str(
        "<p>Every object below is a rendered picture of the model itself, made from \
         the model's own vertices. The file next to it is a <code>.gltf</code>, \
         which is the model. <a href=\"#how\">How to open one</a>.</p>\n",
    );
    page.push_str("<div class=\"grid\">\n");
    for (i, (index, name)) in models.iter().enumerate() {
        let preview = previews
            .iter()
            .find(|(j, _)| j == index)
            .map(|(_, p)| p.clone())
            .unwrap_or_default();
        page.push_str(&format!(
            "<figure>\
             <a href=\"{model_prefix}/{name}\" title=\"Open {index:05}\" >\
             <img src=\"{model_prefix}/{preview}\" alt=\"{index:05}\" loading=\"lazy\">\
             </a><figcaption>{index:05}</figcaption></figure>\n"
        ));
        let _ = i;
        files += 1;
    }
    page.push_str("</div>\n");

    page.push_str(&format!(
        "<h2>Textures <span class=\"count\">{}</span></h2>\n",
        textures.len()
    ));
    page.push_str(
        "<p>The game stores each texture as a palette and a grid of one-byte \
         indices. They are expanded to ordinary PNGs here.</p>\n",
    );
    page.push_str("<div class=\"grid\">\n");
    for (index, name) in &textures {
        page.push_str(&format!(
            "<figure>\
             <a href=\"{texture_prefix}/{name}\" title=\"Open {index:05}\" >\
             <img src=\"{texture_prefix}/{name}\" alt=\"{index:05}\" loading=\"lazy\">\
             </a><figcaption>{index:05}</figcaption></figure>\n"
        ));
        files += 1;
    }
    page.push_str("</div>\n");
    page.push_str(INDEX_TAIL);

    let path = base.join("index.html");
    std::fs::write(&path, page).with_context(|| format!("writing {}", path.display()))?;
    Ok(files)
}

/// The page's head: no script, no stylesheet, no network.
const INDEX_HEAD: &str = r#"<!DOCTYPE html>
<html lang="en">
<meta charset="utf-8">
<title>3D assets</title>
<style>
 body { font: 15px/1.5 system-ui, sans-serif; margin: 0 auto; max-width: 76rem; padding: 2rem 1rem; background: #16181d; color: #e6e6e6; }
 h1 { font-size: 1.6rem; margin: 0 0 .25rem; }
 h2 { font-size: 1.2rem; margin: 2.5rem 0 .25rem; border-top: 1px solid #2c3038; padding-top: 1.5rem; }
 .count { color: #8b93a3; font-weight: normal; font-size: .9rem; }
 .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(96px, 1fr)); gap: .75rem; margin: 1rem 0; }
 figure { margin: 0; }
 img { width: 100%; height: auto; background: #0b0d10; border: 1px solid #2c3038; border-radius: 4px; display: block; }
 figcaption { font-size: .7rem; color: #8b93a3; text-align: center; margin-top: .2rem; }
 #how { background: #1b1f26; border: 1px solid #2c3038; border-radius: 6px; padding: 1rem 1.25rem; margin: 2rem 0 0; }
 #how h2 { border: 0; margin: 0 0 .5rem; padding: 0; font-size: 1rem; }
 #how code { background: #0b0d10; padding: .1rem .3rem; border-radius: 3px; }
</style>
<h1>3D assets</h1>
<p>Pictures of everything this game shipped, recovered from its own containers.</p>
"#;

/// The page's tail: what a reader does with a glTF they have just been shown.
const INDEX_TAIL: &str = r#"
<div id="how">
<h2>How to open a <code>.gltf</code></h2>
<ul>
 <li><b>In a browser, no install:</b> drag the file onto
     <code>https://gltf-viewer.donmccurdy.com</code>. Nothing is uploaded.</li>
 <li><b>In Blender:</b> <code>File &rarr; Import &rarr; glTF 2.0</code>.</li>
 <li><b>On Windows 10/11:</b> open it in the <i>3D Viewer</i> app.</li>
 <li>Each <code>.gltf</code> has a <code>.tail</code> file beside it holding the
     bytes that are still undecoded (the animation and the texture
     coordinates), so nothing is thrown away.</li>
</ul>
</div>
"#;

/// List the files of `dir` whose names match `<index>_<rest>`, returning the
/// index and the full file name.
fn list_dir(dir: &Path, extension: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.ends_with(extension) {
            continue;
        }
        // The output names are `<index:05>_<rest>.<ext>`, so the leading digits
        // are the container's resource index and the sort order is the game's.
        let digits: String = name.chars().take_while(|c| c.is_ascii_digit()).collect();
        let index = digits.parse::<usize>().unwrap_or(usize::MAX);
        out.push((index, name));
    }
    out.sort();
    out
}

/// Build a filename stem from the resource name, dropping the source
/// extension so the output extension is unambiguous.
fn output_stem(container: &Container, index: usize) -> String {
    sanitize(container.name(index), index)
}
