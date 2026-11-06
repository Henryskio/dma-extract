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

pub mod anim;
pub mod gltf;
pub mod modfile;
pub mod render;
pub mod texfile;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Length of the metadata region of a `Models` container: `4 * N` u32
/// timestamps followed by a 100-byte trailer.
///
/// Measured on `DMA3D.MMM` (N=460): the region between `names_end` (16602)
/// and `offsets[0]` (18542) is 1940 bytes and `4*460 + 100 == 1940`.
///
/// The trailer is not a per-resource array, and its content is read verbatim,
/// not decoded — which is why the two descriptions this function has carried
/// could both be wrong without changing a byte of output. On the English disc
/// it was read as a reference to another container (`u16 2`, `u16 1`, `u32 0`,
/// a timestamp, a 30-byte `DMA3D.MMX` name, `0xff` padding); `lean/BINARY-AUDIT.md`
/// §5 reads the Italian disc's same 100 bytes as 32-byte-padded `.mod` names, an
/// `0xe2ee` marker and a 16-byte GUID. Nothing here parses either reading, so
/// only the length is asserted.
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
///
/// The intermediate split is a hypothesis only, and `lean/BINARY-AUDIT.md` §5
/// reads the Italian disc's 4,232 bytes as 32-byte-padded `.TGA` names followed
/// by a column of `0xb5b5*` values, which contradicts it. Only the total is
/// asserted here; the bytes are read verbatim.
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
    /// Textures written whose palette/index split the payload does not pin, so
    /// the palette read is the smallest admissible one (`docs/DMA3D.md` §11).
    pub ambiguous: usize,
    /// Payloads written as a glTF line loop, i.e. drawing profiles rather than
    /// meshes (`docs/DMA3D.md` §11.25).
    pub profiles: usize,
    /// Payloads written as JSON+text, i.e. `mod II` record streams whose record
    /// schema the bytes do not determine (`docs/DMA3D.md` §11.38).
    pub records: usize,
    /// Payloads written as JSON+text, i.e. parameter tables of small values at
    /// alternating byte positions with zero fill (`docs/DMA3D.md` §11.37, §11.39).
    pub params: usize,
    /// Payloads that are neither a mesh nor a drawing profile, and are therefore
    /// written through as the original bytes rather than as modern geometry.
    /// Counted, not skipped: `AGENTS.md` requires a per-resource report.
    pub unconverted: usize,
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
///   the bytes the mesh decoder does not read and a `.anim.txt` rendering the
///   same bytes with the measured anchors, which is what keeps the modern
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
        return extract(container, out_dir);
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
        return extract(container, out_dir);
    }
    extract_pngs(container, out_dir)
}

/// The `--original` path: every payload written exactly as stored.
///
/// No extension is chosen: the resource name carries the game's own, which is
/// why this is the one 3D writer that does not name a format.
fn extract(container: &Container, out_dir: &Path) -> Result<Extract3DReport> {
    std::fs::create_dir_all(out_dir)?;
    let mut report = Extract3DReport::default();

    for index in 0..container.resource_count as usize {
        let name = container.name(index);
        let payload = container.payload(index);
        let stem = output_stem(container, index);
        let path = out_dir.join(format!("{index:05}_{stem}"));

        let written =
            std::fs::write(&path, payload).with_context(|| format!("writing {}", path.display()));
        if let Err(error) = written {
            eprintln!(
                "warning: {}: 3D resource {index} '{name}': {error:#}",
                container.path.display()
            );
            report
                .failed
                .push((index, name.to_string(), error.to_string()));
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

        // A payload is a mesh chain, or a `createMod` drawing profile, or
        // neither. The mesh reading is tried first because it is the shape the
        // 17 shipped geometry resources carry; a drawing profile is the f32
        // record stream of §11.25, written as a glTF line loop; and neither is
        // written through as the original bytes, counted as unconverted rather
        // than skipped (`AGENTS.md` requires the report).
        let mesh = match modfile::parse_mesh(payload) {
            Ok(m) => m,
            Err(mesh_err) => {
                match modfile::parse_profile(payload) {
                    Ok(profile) => {
                        let path = out_dir.join(format!("{index:05}_{stem}.gltf"));
                        gltf::write_profile_gltf(&path, name, &profile)
                            .with_context(|| format!("writing the profile of {name}"))?;
                        // The preview, so the index page shows the profile's
                        // own outline beside it rather than a broken image.
                        // Drawn from the point stream, not from a surface: the
                        // engine's tessellation of a profile is not in the
                        // payload and inventing one would be a guess.
                        let preview = render::render_profile(&profile);
                        let preview_path =
                            out_dir.join(format!("{index:05}_{stem}.preview.png"));
                        gltf::encode_png(
                            &preview.rgba,
                            render::PREVIEW_PIXELS,
                            render::PREVIEW_PIXELS,
                        )
                        .and_then(|png| {
                            std::fs::write(&preview_path, png)
                                .map_err(|e| anyhow::anyhow!("{e}"))
                                .map_err(Into::into)
                        })
                        .with_context(|| format!("writing the preview of {name}"))?;
                        if preview.covered == 0 {
                            eprintln!(
                                "warning: {}: model {index} '{name}': the profile preview \
                                 covers no pixels",
                                container.path.display()
                            );
                            report
                                .failed
                                .push((index, name.to_string(), "preview covers no pixels".into()));
                        }
                        report.written += 1;
                        report.profiles += 1;
                        report.resources.push(Extracted3D {
                            index,
                            name: name.to_string(),
                            path,
                            bytes: payload.len(),
                        });
                        continue;
                    }
                    Err(profile_err) => {
                        // Neither a mesh nor a drawing profile. It may still carry
                        // a `mod II` record stream, which is written as JSON+text
                        // the shape is measured (`docs/DMA3D.md` §11.38) and the
                        // record schema is not, so the document says so rather
                        // than asserting a meaning the bytes do not carry.
                        match modfile::parse_magic_record(payload) {
                            Ok(record) => {
                                let path = out_dir.join(format!("{index:05}_{stem}.json"));
                                gltf::write_record_json(&path, name, &record)
                                    .with_context(|| format!("writing the record of {name}"))?;
                                report.written += 1;
                                report.records += 1;
                                report.resources.push(Extracted3D {
                                    index,
                                    name: name.to_string(),
                                    path,
                                    bytes: payload.len(),
                                });
                                continue;
                            }
                            Err(record_err) => {
                                // Neither a mesh, a profile nor a record stream.
                                // It may still be a parameter table -- small
                                // values at alternating byte positions with zero
                                // fill (`docs/DMA3D.md` §11.37, §11.39) -- which is
                                // written as JSON+text for the same reason the
                                // record stream is: it is data, not geometry.
                                match modfile::parse_param_table(payload) {
                                    Ok(table) => {
                                        let path =
                                            out_dir.join(format!("{index:05}_{stem}.json"));
                                        gltf::write_param_json(&path, name, &table)
                                            .with_context(|| {
                                                format!("writing the table of {name}")
                                            })?;
                                        report.written += 1;
                                        report.params += 1;
                                        report.resources.push(Extracted3D {
                                            index,
                                            name: name.to_string(),
                                            path,
                                            bytes: payload.len(),
                                        });
                                        continue;
                                    }
                                    Err(table_err) => {
                                        report.unconverted += 1;
                                        report.failed.push((
                                            index,
                                            name.to_string(),
                                            format!(
                                                "{mesh_err:#}; the profile reading also fails \
                                                 ({profile_err}); the record reading also fails \
                                                 ({record_err}); the parameter-table reading also \
                                                 fails ({table_err})"
                                            ),
                                        ));
                                        // Nothing modern was produced, so the
                                        // payload is written through unchanged.
                                        let path =
                                            out_dir.join(format!("{index:05}_{stem}.mod"));
                                        std::fs::write(&path, payload).with_context(|| {
                                            format!("writing {}", path.display())
                                        })?;
                                        report.written += 1;
                                        report.resources.push(Extracted3D {
                                            index,
                                            name: name.to_string(),
                                            path,
                                            bytes: payload.len(),
                                        });
                                        continue;
                                    }
                                }
                            }
                        }
                    }
                }
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

        // The texture coordinates, carried only where the payload's own float
        // pairs are all inside 0..1. That is the condition under which they
        // read as coordinates rather than as something else, and it holds for
        // 58 of the 94 payloads that carry the `[0, 1, V]` header; the rest
        // come out without them rather than with guesses.
        let records = anim::locate(payload, &mesh);
        let texcoords = texcoords_of(payload, &records);
        let path = out_dir.join(format!("{index:05}_{stem}.gltf"));
        gltf::write_gltf(&path, name, &mesh, &materials, texcoords.as_deref())
            .with_context(|| format!("writing {}", path.display()))?;
        // The undecoded part goes beside the glTF, in two forms: the bytes
        // verbatim, and a readable document that carries the measured anchors
        // and renders the records the way the game stores them. So a modern
        // model is the mesh plus everything the decoder did not read, and
        // nothing is quietly dropped.
        let suffix = format!("{index:05}_{stem}");
        gltf::write_tail(
            &out_dir.join(gltf::tail_name(&format!("{suffix}.gltf"))),
            &payload[mesh.tail_offset()..],
        )
        .with_context(|| format!("writing the tail of {name}"))?;
        let document = anim::dump(name, payload, &mesh, &records);
        let records_path = out_dir.join(anim::records_name(&format!("{suffix}.gltf")));
        std::fs::write(&records_path, document)
            .with_context(|| format!("writing the records of {name}"))?;
        // A preview beside the glTF, so the index page can show what each
        // object is without the reader opening a 3D tool first.
        let preview = render::render(&mesh);
        let preview_path = out_dir.join(format!("{index:05}_{stem}.preview.png"));
        gltf::encode_png(
            &preview.rgba,
            render::PREVIEW_PIXELS,
            render::PREVIEW_PIXELS,
        )
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
        let bytes = std::fs::metadata(&path)
            .map(|m| m.len() as usize)
            .unwrap_or(0);
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

/// The `V` float pairs as texture coordinates, when they are all in 0..1.
///
/// Returns `None` when the payload does not carry the `[0, 1, V]` header or when
/// any pair falls outside `0..1`, because then the pairs are something else and
/// emitting them as coordinates would be a guess.
fn texcoords_of(payload: &[u8], records: &anim::Records) -> Option<Vec<[f32; 2]>> {
    if records.pairs_in_unit_range != Some(records.pair_count) {
        return None;
    }
    let v = records.pair_count;
    let mut out = Vec::with_capacity(v);
    for k in 0..v {
        let at = records.tail_offset + 12 + 8 * k;
        if at + 8 > payload.len() {
            return None;
        }
        let a = f32::from_le_bytes([
            payload[at],
            payload[at + 1],
            payload[at + 2],
            payload[at + 3],
        ]);
        let b = f32::from_le_bytes([
            payload[at + 4],
            payload[at + 5],
            payload[at + 6],
            payload[at + 7],
        ]);
        // The record is *detected* with a tolerance (`anim.rs` accepts
        // `-0.001..=1.001`), because a float stored by the game is not exact at
        // the boundary. What is written must be exact: a texture coordinate of
        // `-0.000001` is the float representation of zero and a viewer reads it
        // as a sample from outside the texture. Clamp into `0..=1` on the way
        // out, so the detection stays loose and the output stays well-formed.
        out.push([a.clamp(0.0, 1.0), b.clamp(0.0, 1.0)]);
    }
    Some(out)
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

/// The output file name's stem: the resource name, sanitised.
///
/// It already ends in the game's own extension, which is why the `--original`
/// path does not append one — appending the format's extension produced
/// `00033_od.mod.mod`, a doubled extension the container never spells. The
/// modern paths give the file the extension of the format they write, so they
/// are the only ones that choose one.
fn output_stem(container: &Container, index: usize) -> String {
    sanitize(container.name(index), index)
}
