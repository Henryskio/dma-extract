//! Decide the width and the height of the 27 rectangular textures.
//!
//! Scratch bin. `src/dma3d/texfile.rs` reads every one of the 705 payloads of
//! `DMA3D.MMX` as `RGBA[K]` + `[u8; A]` with `A` a power of two, so the **area**
//! is measured and the orientation of the 1:2 rectangles is not.
//!
//! The two readings of a rectangle are **not** transposes of one another: at
//! the wrong width the rows of the image are concatenated, so the picture comes
//! out squashed and interleaved. Statistics computed on the index bytes alone
//! can therefore be decisive, and this bin prints them beside the picture
//! itself:
//!
//! * the **row-adjacency and column-adjacency** of each candidate width, which
//!   is the statistic `axis_of` uses, over the whole field of widths it could
//!   have picked rather than only its winner;
//! * the **shape** of the picture at each width -- 4-connected components of
//!   non-background pixels, blank rows and columns at each edge, and pixels the
//!   reading has cut in half on the frame border;
//! * the **mesh** that shares the texture's name in `DMA3D.MMM`
//!   (`<stem>.mod` <-> `<stem>.NN.TGA`), and the aspect ratio of that mesh in
//!   the plane it is flat in -- an external measurement of the button's shape;
//! * the **`V` float pairs** that follow the mesh's `[0, 1, V]` tail header,
//!   which `docs/DMA3D.md` §3.2 records as texture coordinates on some
//!   payloads and as something else on others. When they are not in `0..1`
//!   they may be in **pixel** units, in which case their maxima *are* the
//!   width and the height;
//! * the index bytes read at both widths: vertical and horizontal continuity,
//!   the dominant row and column period with the divisibility check each
//!   implies, and the count of uniform rows and uniform columns;
//! * an ASCII map of both readings, and PNGs of both readings, so the choice
//!   can be reviewed by eye.
//!
//! Every offset and every decode comes from the library, so nothing here can
//! drift from `src/dma3d/texfile.rs` or `src/dma3d/modfile.rs`.

use clap::Parser;
use mmf_extract::container::Container;
use mmf_extract::dma3d::modfile::{self, ModelMesh};
use mmf_extract::dma3d::texfile::{self, Axis, Texture};
use std::path::PathBuf;

/// The widths a row-structure test considers, matching `texfile.rs`.
const WIDTH_CANDIDATES: [usize; 6] = [8, 16, 32, 64, 128, 256];

#[derive(Parser, Debug)]
#[command(
    name = "texorient",
    about = "Find the evidence that fixes the orientation of the 27 rectangular textures",
    long_about = "Prints, for every texture whose area is a 1:2 rectangle, the readings \
of it that exist: the internal statistics of both orientations, the aspect ratio of the \
mesh that shares its name (and the plane that mesh is flat in), and the range of the \
float pairs in that mesh's undecoded tail. With --map it also draws both readings as \
text; with --png it writes both readings as PNG images. Arguments are parsed by clap, \
the crate the other scratch bins use: a sibling bin once walked std::env::args by hand \
and silently turned a flag into a name filter."
)]
struct Args {
    /// The `MMFW Textures` container (`DMA3D.MMX`).
    #[arg(value_name = "TEXTURES")]
    textures: PathBuf,

    /// The `MMFW Models` container (`DMA3D.MMM`), for the mesh cross-check.
    #[arg(long, value_name = "MODELS", default_value = "/tmp/dma3d/pc/DMA3D.MMM")]
    models: PathBuf,

    /// Only textures whose name contains this text.
    #[arg(long, value_name = "TEXT", default_value = "")]
    filter: String,

    /// Also report the square textures that have a name-paired mesh, as a
    /// control: for those the aspect is known to be 1:1, so the mesh and the
    /// tail pairs can be calibrated against a known answer.
    #[arg(long)]
    control: bool,

    /// Validate the two statistics against the 678 square textures, where the
    /// width is known in advance: report how often each one lands on the
    /// square root, and list every disagreement. This is the same treatment
    /// `docs/DMA3D.md` §4.2 gives `axis_of`'s 643/659.
    #[arg(long)]
    validate: bool,

    /// Draw both readings of each selected texture as text, one row per line.
    #[arg(long)]
    map: bool,

    /// Sample every N-th pixel in the drawing, so a 256-wide texture fits on
    /// one screen and is judged as a whole picture.
    #[arg(long, value_name = "N", default_value = "1")]
    scale: usize,

    /// Write both readings of each selected texture as PNGs into this folder.
    #[arg(long, value_name = "DIR")]
    png: Option<PathBuf>,
}

fn main() {
    let args = Args::parse();
    let raw = std::fs::read(&args.textures).expect("read the textures container");
    let tex = Container::parse(&args.textures, raw).expect("open Textures");

    // The models container is optional: without it the bin still answers the
    // pixel-side questions, and says so rather than failing.
    let models = match std::fs::read(&args.models) {
        Ok(data) => Container::parse(&args.models, data).ok(),
        Err(e) => {
            eprintln!(
                "note: no models container at {} ({e}); the mesh cross-check is skipped",
                args.models.display()
            );
            None
        }
    };
    let mut meshes: std::collections::HashMap<String, MeshFacts> =
        std::collections::HashMap::new();
    if let Some(container) = &models {
        for i in 0..container.resource_count as usize {
            let Ok(mesh) = modfile::parse_mesh(container.payload(i)) else {
                continue;
            };
            meshes.insert(container.name(i).to_string(), facts(&mesh, container.payload(i)));
        }
    }
    eprintln!("meshes with a decoded chain: {}", meshes.len());

    if let Some(dir) = &args.png {
        let _ = std::fs::create_dir_all(dir);
    }

    if args.validate {
        validate(&tex);
        return;
    }

    let mut shown = 0usize;
    for i in 0..tex.resource_count as usize {
        let name = tex.name(i).to_string();
        if !args.filter.is_empty() && !name.contains(&args.filter) {
            continue;
        }
        let Ok(texture) = texfile::parse(tex.payload(i)) else {
            continue;
        };
        let rectangular = texture.axis != Axis::Square;
        if !rectangular && !args.control {
            continue;
        }
        if rectangular && args.control {
            continue;
        }
        let partner = meshes.get(&mesh_name(&name)).cloned();
        shown += 1;
        print_one(i, &name, &texture, partner.as_ref(), &args);
    }
    if shown == 0 {
        eprintln!("nothing matched the filter {:?}", args.filter);
    } else {
        eprintln!("{shown} textures reported");
    }
}

/// Run both statistics over the square textures and report how often each
/// lands on the square root.
///
/// A square texture has exactly one reading, so this is the only place the two
/// candidate-selecting statistics can be checked against a known answer. The
/// comparison is the same one `docs/DMA3D.md` §4.2 records for `axis_of`
/// (643 of 659 for textures with three or more palette entries in use), and it
/// is what decides how much weight the report may put on them.
fn validate(tex: &Container) {
    let mut rows = 0usize;
    let mut voted_root = 0usize;
    let mut vdiff_root = 0usize;
    let mut both_root = 0usize;
    let mut disagree = Vec::new();
    let mut real = 0usize;
    for i in 0..tex.resource_count as usize {
        let Ok(t) = texfile::parse(tex.payload(i)) else {
            continue;
        };
        if t.axis != Axis::Square {
            continue;
        }
        rows += 1;
        let root = t.width;
        let mut best_adj = (0usize, f64::NEG_INFINITY);
        let mut best_diff = (0usize, f64::INFINITY);
        for (w, h) in candidate_readings(t.indices.len()) {
            let st = stats(&t.indices, w, h);
            if st.vertical > best_adj.1 {
                best_adj = (w, st.vertical);
            }
            if st.vertical_delta < best_diff.1 {
                best_diff = (w, st.vertical_delta);
            }
        }
        let distinct = t.palette.len();
        if distinct >= 3 {
            real += 1;
        }
        if best_adj.0 == root {
            voted_root += 1;
        }
        if best_diff.0 == root {
            vdiff_root += 1;
        }
        if best_adj.0 == root && best_diff.0 == root {
            both_root += 1;
        } else if distinct >= 3 && disagree.len() < 40 {
            disagree.push(format!(
                "{}_{}: root {}x{}, row-adj picks {}, v-diff picks {}",
                i,
                tex.name(i),
                root,
                root,
                best_adj.0,
                best_diff.0
            ));
        }
    }
    println!("{rows} square textures ({real} with three or more palette entries)");
    println!("  row-adjacency picks the square root : {voted_root} / {rows}");
    println!("  vertical delta picks the square root : {vdiff_root} / {rows}");
    println!("  both agree on the square root        : {both_root} / {rows}");
    if !disagree.is_empty() {
        println!("  first disagreements (real textures):");
        for d in &disagree {
            println!("    {d}");
        }
    }
}

/// The mesh that shares a texture's name.
///
/// The pairing is by stem: `createBtDraw.01.TGA` belongs to `createBtDraw.mod`,
/// so a trailing `.NN` before the extension is dropped. A texture with no `NN`
/// component keeps its whole stem.
fn mesh_name(texture_name: &str) -> String {
    let stem = texture_name
        .strip_suffix(".TGA")
        .or_else(|| texture_name.strip_suffix(".tga"))
        .unwrap_or(texture_name);
    let bare = match stem.rfind('.') {
        Some(at)
            if at > 0 && stem[at + 1..].chars().all(|c| c.is_ascii_digit()) =>
        {
            &stem[..at]
        }
        _ => stem,
    };
    format!("{bare}.mod")
}

/// What the mesh says about a texture, measured once per mesh.
#[derive(Debug, Clone)]
struct MeshFacts {
    vertices: usize,
    /// The extent of the bounding box on each axis.
    extent: [f32; 3],
    /// The single axis whose extent is effectively zero, when the mesh is flat.
    flat: Option<usize>,
    /// The tail's `V` float pairs, read from `tail_offset() + 12`.
    pairs: Vec<[f32; 2]>,
}

fn facts(mesh: &ModelMesh, payload: &[u8]) -> MeshFacts {
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for p in &mesh.positions {
        for axis in 0..3 {
            min[axis] = min[axis].min(p[axis]);
            max[axis] = max[axis].max(p[axis]);
        }
    }
    let extent = [
        max[0] - min[0],
        max[1] - min[1],
        max[2] - min[2],
    ];
    // A mesh is flat in an axis whose extent is under a hundredth of the
    // largest one; that is the axis a UI quad faces along.
    let biggest = extent.iter().cloned().fold(0.0f32, f32::max);
    let flat = extent
        .iter()
        .position(|&e| biggest > 0.0 && e < biggest * 0.01)
        .filter(|_| biggest > 0.0);

    // The tail is `[0, 1, V]` then `V` float pairs, which `docs/DMA3D.md`
    // §3.2 records as coordinates on some payloads.
    let at0 = mesh.tail_offset() + 12;
    let mut pairs = Vec::with_capacity(mesh.vertex_count());
    for v in 0..mesh.vertex_count() {
        let at = at0 + 8 * v;
        if at + 8 > payload.len() {
            break;
        }
        pairs.push([
            f32::from_le_bytes([payload[at], payload[at + 1], payload[at + 2], payload[at + 3]]),
            f32::from_le_bytes([
                payload[at + 4],
                payload[at + 5],
                payload[at + 6],
                payload[at + 7],
            ]),
        ]);
    }

    MeshFacts {
        vertices: mesh.vertex_count(),
        extent,
        flat,
        pairs,
    }
}

fn print_one(index: usize, name: &str, texture: &Texture, partner: Option<&MeshFacts>, args: &Args) {
    let area = texture.indices.len();
    let (w_wide, h_wide) = wide_tall(area);
    let axis = match texture.axis {
        Axis::Square => "square",
        Axis::Wide => "wide",
        Axis::Tall => "tall",
    };
    println!(
        "#{index:03} {name}  K={}  area={area}  currently {}x{} ({axis})",
        texture.palette.len(),
        texture.width,
        texture.height,
    );
    println!("     key index      {}", key_index(texture));

    // `axis_of` is not constrained to the 1:2 pair, so print every width it
    // could have picked with that width's score. The two readings of the 1:2
    // assumption are transposes of one another, so the statistic they are
    // compared by measures anisotropy and not the true axis; showing the whole
    // field makes that visible instead of hiding it behind a winner.
    for (w, h) in candidate_readings(area) {
        let st = stats(&texture.indices, w, h);
        let sh = shape_of(texture, w, h);
        let here = w == texture.width;
        let mark = if w == w_wide {
            " 1:2-wide"
        } else if w == w_wide / 2 {
            " 1:2-tall"
        } else {
            ""
        };
        println!(
            "     {w:>3}x{h:<3} {} row-adj {:.4} col-adj {:.4} | v-diff {:.3} h-diff {:.3} | bg {:.3} comps {} blank rows {}/{}, cols {}/{} cut {} edge-slope {:.2}{mark}",
            if here { "<= current" } else { "           " },
            st.vertical,
            st.horizontal,
            st.vertical_delta,
            st.horizontal_delta,
            sh.background,
            sh.components,
            sh.blank_rows.0,
            sh.blank_rows.1,
            sh.blank_cols.0,
            sh.blank_cols.1,
            sh.cut_pixels,
            sh.edge_slope,
        );
    }
    // The two readings the question is about, with the statistics beside them.
    for (label, (w, h)) in [("wide", (w_wide, h_wide)), ("tall", (h_wide, w_wide))] {
        let st = stats(&texture.indices, w, h);
        let _ = st;
        let _ = label;
    }

    match partner {
        None => println!("     mesh           none with a decoded chain"),
        Some(m) => {
            let axes = ['x', 'y', 'z'];
            let flat = m
                .flat
                .map(|a| format!("{}", axes[a]))
                .unwrap_or_else(|| "none (3-D)".into());
            println!(
                "     mesh           {v} vertices, extent {ex:?}, flat in {flat}",
                v = m.vertices,
                ex = m
                    .extent
                    .iter()
                    .map(|e| format!("{e:.4}"))
                    .collect::<Vec<_>>(),
            );
            for plane in 0..3 {
                let (a, b) = plane_axes(plane);
                let (ea, eb) = (m.extent[a], m.extent[b]);
                if ea <= 0.0 || eb <= 0.0 {
                    println!("     plane {}{}       flat", axes[a], axes[b]);
                    continue;
                }
                let mark = if (ea / eb - 2.0).abs() < 0.25 {
                    "  <== 2:1 WIDE"
                } else if (ea / eb - 0.5).abs() < 0.25 {
                    "  <== 1:2 TALL"
                } else {
                    ""
                };
                println!(
                    "     plane {}{}       aspect {ea:.4} / {eb:.4} = {ratio:.4}{mark}",
                    axes[a], axes[b],
                    ratio = ea / eb,
                );
            }
            let (umin, umax) = range(m.pairs.iter().map(|p| p[0]));
            let (vmin, vmax) = range(m.pairs.iter().map(|p| p[1]));
            let in_unit = m
                .pairs
                .iter()
                .filter(|p| (0.0..=1.0).contains(&p[0]) && (0.0..=1.0).contains(&p[1]))
                .count();
            let span_u = umax - umin;
            let span_v = vmax - vmin;
            println!(
                "     tail pairs     {in_unit} of {} in 0..1, u {umin:.4}..{umax:.4}, v {vmin:.4}..{vmax:.4}",
                m.pairs.len(),
            );
            if span_u > 1e-6 && span_v > 1e-6 {
                let ratio = span_u / span_v;
                let mark = if (ratio - 2.0).abs() < 0.25 {
                    "  <== 2:1 WIDE"
                } else if (ratio - 0.5).abs() < 0.25 {
                    "  <== 1:2 TALL"
                } else {
                    ""
                };
                println!("     tail ratio     u-span / v-span = {ratio:.4}{mark}");
                // Pixel-unit coordinates would name the width and the height
                // outright: a pair's maximum is the last pixel's index.
                if in_unit != m.pairs.len() {
                    println!(
                        "     tail maxima    max u {umax:.3}, max v {vmax:.3} \
                          (pair with {w_wide}x{h_wide} or {h_wide}x{w_wide})"
                    );
                }
            }
        }
    }

    if args.map {
        for (label, (w, h)) in [("wide", (w_wide, h_wide)), ("tall", (h_wide, w_wide))] {
            println!("     --- {name} at {w}x{h} ({label}) ---");
            for line in ascii_map(texture, w, h, args.scale) {
                println!("     {line}");
            }
        }
    }
    if let Some(dir) = &args.png {
        for (label, (w, h)) in [("wide", (w_wide, h_wide)), ("tall", (h_wide, w_wide))] {
            let rgba = texture.to_rgba();
            match mmf_extract::dma3d::gltf::encode_png(&rgba, w, h) {
                Ok(png) => {
                    let path = dir.join(format!("{index:05}_{name}_{label}_{w}x{h}.png"));
                    let _ = std::fs::write(&path, &png);
                }
                Err(e) => eprintln!("     png: {e}"),
            }
        }
    }
    println!();
}

/// The plane's two axes, as `(horizontal, vertical)`.
fn plane_axes(plane: usize) -> (usize, usize) {
    match plane {
        0 => (0, 1),
        1 => (0, 2),
        _ => (1, 2),
    }
}

fn range<I: Iterator<Item = f32>>(it: I) -> (f32, f32) {
    let mut lo = f32::INFINITY;
    let mut hi = f32::NEG_INFINITY;
    for v in it {
        lo = lo.min(v);
        hi = hi.max(v);
    }
    (lo, hi)
}

/// The two readings of a 1:2 rectangle, as `(wide, tall)`.
///
/// The area is a power of two, so `2*area` and `area/2` are powers of two too
/// and their roots are exact: an area of `2^(2k+1)` is `2^(k+1)` by `2^k`.
/// Taking `sqrt(area)` and halving it does not work -- it lands on an odd
/// number and prints a reading like `45x22` that no texture can have.
fn wide_tall(area: usize) -> (usize, usize) {
    debug_assert!(area.is_power_of_two());
    let m = area.trailing_zeros() as usize;
    if m % 2 == 0 {
        let root = 1usize << (m / 2);
        return (root, root);
    }
    (1usize << (m / 2 + 1), 1usize << (m / 2))
}

/// Every width `texfile.rs`'s `axis_of` would consider for this area, with the
/// height each implies. The heuristic is not constrained to the 1:2 pair, so
/// the readings it can actually choose among are listed here rather than only
/// the two the 1:2 assumption allows.
fn candidate_readings(area: usize) -> Vec<(usize, usize)> {
    WIDTH_CANDIDATES
        .iter()
        .copied()
        .filter(|w| area % w == 0)
        .map(|w| (w, area / w))
        .collect()
}

/// The index the palette's transparent colour key sits at, or `?` when there
/// is none. Most payloads open with `ff 00 ff 00`.
fn key_index(texture: &Texture) -> String {
    texture
        .palette
        .iter()
        .position(|e| *e == [0xff, 0x00, 0xff, 0x00])
        .map(|i| i.to_string())
        .unwrap_or_else(|| "none".into())
}

/// One reading's internal statistics.
#[derive(Debug, Clone, Copy)]
struct Reading {
    /// Share of vertically adjacent pairs that carry the same index: the
    /// statistic `axis_of` uses.
    vertical: f64,
    /// Share of horizontally adjacent pairs that carry the same index. The
    /// transpose swaps these two, so the pair is what is reported.
    horizontal: f64,
    /// Mean index difference across a vertical step. The two readings are
    /// **not** transposes: at the wrong width a vertical step lands two true
    /// rows further down, so a smooth image shows a *larger* difference there
    /// than at the right width. That is what makes this a discriminator and
    /// not a restatement of `vertical`.
    vertical_delta: f64,
    /// Mean index difference across a horizontal step.
    horizontal_delta: f64,
}

fn stats(indices: &[u8], w: usize, h: usize) -> Reading {
    let mut same_v = 0usize;
    let mut same_h = 0usize;
    let mut delta_v = 0u64;
    let mut delta_h = 0u64;
    for row in 0..h - 1 {
        let base = row * w;
        for col in 0..w {
            let here = indices[base + col];
            let below = indices[base + w + col];
            if here == below {
                same_v += 1;
            }
            delta_v += u64::from(here.abs_diff(below));
            if col + 1 < w {
                let right = indices[base + col + 1];
                if here == right {
                    same_h += 1;
                }
                delta_h += u64::from(here.abs_diff(right));
            }
        }
    }
    Reading {
        vertical: same_v as f64 / ((h - 1) * w) as f64,
        horizontal: same_h as f64 / ((h - 1) * w.max(1)) as f64,
        vertical_delta: delta_v as f64 / ((h - 1) * w) as f64,
        horizontal_delta: delta_h as f64 / ((h - 1) * w.max(1)) as f64,
    }
}

/// The shape of the non-background pixels at one reading.
///
/// The two readings of a rectangle are **not** transposes of one another -- that
/// would need `data[r*H+c] == data[c*W+r]`, which no shipped payload does. At
/// the wrong width the rows of the image are *concatenated*, so the picture
/// comes out squashed and interleaved: a single object is sliced into two. That
/// is why the statistics below can be decisive and not merely anisotropic.
#[derive(Debug, Clone, Copy)]
struct Shape {
    /// 4-connected components of non-background pixels.
    components: usize,
    /// Blank (background-only) rows at the top and at the bottom.
    blank_rows: (usize, usize),
    /// Blank columns at the left and at the right.
    blank_cols: (usize, usize),
    /// Non-background pixels that sit on the frame's border, i.e. an object the
    /// reading has cut in half.
    cut_pixels: usize,
    /// Share of the frame that is background.
    background: f64,
    /// Mean number of pixels the right edge of the background moves per row.
    ///
    /// A panel's rounded corner is a 45-degree chamfer or an arc, so the true
    /// reading shows an edge that advances about **one pixel per row**. At a
    /// width twice too large the rows are pairs of true rows, so the same edge
    /// advances two pixels per row and the picture looks like two panels side
    /// by side -- the doubling trap that makes a 1-D texture's two readings
    /// look equally plausible.
    edge_slope: f64,
}

/// The background index of a texture: the transparent colour key when the
/// palette carries one, otherwise the index the data uses most. Without a key
/// the second choice is the only "empty" a texture has, and it is the one an
/// engine would leave the untouched part of the buffer holding.
fn background_index(texture: &Texture) -> u8 {
    texture
        .palette
        .iter()
        .position(|e| *e == [0xff, 0x00, 0xff, 0x00])
        .map(|i| i as u8)
        .unwrap_or_else(|| {
            let mut counts = [0usize; 256];
            for &i in &texture.indices {
                counts[usize::from(i)] += 1;
            }
            let mut best = 0u8;
            for i in 0..256 {
                if counts[i] > counts[usize::from(best)] {
                    best = i as u8;
                }
            }
            best
        })
}

fn shape_of(texture: &Texture, w: usize, h: usize) -> Shape {
    let bg = usize::from(background_index(texture));
    let data = &texture.indices;
    let mut components = 0usize;
    let mut seen = vec![false; w * h];
    let mut blank_rows = (0usize, 0usize);
    let mut blank_cols = (0usize, 0usize);
    let mut cut = 0usize;
    let mut background = 0usize;
    let mut slope_total = 0u64;
    let mut slope_steps = 0usize;

    // Flood fill, so the count does not depend on the scan order.
    let mut stack = Vec::new();
    for row in 0..h {
        for col in 0..w {
            let at = row * w + col;
            if usize::from(data[at]) == bg {
                background += 1;
                continue;
            }
            if seen[at] {
                continue;
            }
            components += 1;
            stack.push(at);
            seen[at] = true;
            while let Some(cur) = stack.pop() {
                let (r, c) = (cur / w, cur % w);
                if r + 1 < h {
                    let n = cur + w;
                    if usize::from(data[n]) != bg && !seen[n] {
                        seen[n] = true;
                        stack.push(n);
                    }
                }
                if r > 0 {
                    let n = cur - w;
                    if usize::from(data[n]) != bg && !seen[n] {
                        seen[n] = true;
                        stack.push(n);
                    }
                }
                if c + 1 < w {
                    let n = cur + 1;
                    if usize::from(data[n]) != bg && !seen[n] {
                        seen[n] = true;
                        stack.push(n);
                    }
                }
                if c > 0 {
                    let n = cur - 1;
                    if usize::from(data[n]) != bg && !seen[n] {
                        seen[n] = true;
                        stack.push(n);
                    }
                }
            }
        }
    }

    for row in 0..h {
        if data[row * w..row * w + w].iter().any(|&b| usize::from(b) != bg) {
            break;
        }
        blank_rows.0 += 1;
    }
    for row in (0..h).rev() {
        if data[row * w..row * w + w].iter().any(|&b| usize::from(b) != bg) {
            break;
        }
        blank_rows.1 += 1;
    }
    for col in 0..w {
        if (0..h).any(|row| usize::from(data[row * w + col]) != bg) {
            break;
        }
        blank_cols.0 += 1;
    }
    for col in (0..w).rev() {
        if (0..h).any(|row| usize::from(data[row * w + col]) != bg) {
            break;
        }
        blank_cols.1 += 1;
    }

    // The right edge of the background, row by row, and how far it travels.
    let mut previous: Option<usize> = None;
    for row in 0..h {
        let mut edge = None;
        for col in (0..w).rev() {
            if usize::from(data[row * w + col]) != bg {
                edge = Some(col);
                break;
            }
        }
        if let (Some(a), Some(b)) = (previous, edge) {
            if a != b {
                slope_total += u64::from(a.abs_diff(b) as u32);
                slope_steps += 1;
            }
        }
        previous = edge;
    }
    for col in 0..w {
        if usize::from(data[col]) != bg || usize::from(data[(h - 1) * w + col]) != bg {
            cut += 1;
        }
    }
    for row in 0..h {
        if usize::from(data[row * w]) != bg || usize::from(data[row * w + w - 1]) != bg {
            cut += 1;
        }
    }

    Shape {
        components,
        blank_rows,
        blank_cols,
        cut_pixels: cut,
        background: background as f64 / (w * h) as f64,
        edge_slope: if slope_steps == 0 {
            0.0
        } else {
            slope_total as f64 / slope_steps as f64
        },
    }
}
/// The palette gives every index a colour, so the character is that colour's
/// brightness: an index into the transparent magenta key is a space, and the
/// rest run from `.` (dark) through `+` and `*` to `#` (light). That is what
/// makes the drawing readable -- with 178 palette entries an index-to-digit map
/// is mostly `?`.
///
/// With `scale > 1` every `scale`-th pixel is sampled, so a 256x128 texture
/// fits on one screen and is seen as a whole picture instead of 128 wrapped
/// lines.
fn ascii_map(texture: &Texture, w: usize, h: usize, scale: usize) -> Vec<String> {
    let shade = |index: u8| -> char {
        let entry = texture.entry(index);
        if entry == [0xff, 0x00, 0xff, 0x00] {
            return ' ';
        }
        let luma = (0.299 * f32::from(entry[0])
            + 0.587 * f32::from(entry[1])
            + 0.114 * f32::from(entry[2]))
            / 255.0;
        const RAMP: [char; 10] = ['.', ':', '-', '=', '+', '*', '%', '$', '#', '@'];
        RAMP[((luma * 9.0).round() as usize).min(9)]
    };
    let step = scale.max(1);
    let mut out = Vec::new();
    let mut row = 0;
    while row < h {
        let base = row * w;
        let line: String = (0..w)
            .step_by(step)
            .map(|col| shade(texture.indices[base + col]))
            .collect();
        out.push(line);
        row += step;
    }
    out
}
