//! Render a `.mod` mesh to a PNG, so a model can be looked at without a 3D
//! tool.
//!
//! The modern output for this game is glTF, which is right — it carries the
//! geometry, the animation slots and the materials, and it opens in Blender and
//! in any viewer. But a folder of 144 `.gltf` files is not something a person
//! who has never opened a 3D program can browse, which is the point of the
//! modern mode.
//!
//! So each mesh is also rasterised here into a PNG that goes into the index
//! page next to it. This is a *preview*, not a renderer: flat-shaded
//! triangles, one light, a z-buffer, orthographic projection fitted to the
//! mesh's own bounding box. It exists to answer "what is this object", not to
//! look like the game — and it is verified by decoding the pixels back and
//! checking that a triangle-shaped mesh actually covers area
//! (`tests/dma3d.rs`), which is the assertion that stops a broken raster from
//! silently producing blank images.
//!
//! Everything is integer and fixed-size, so a model of any vertex count
//! rasterises in bounded time.

use crate::dma3d::modfile::ModelMesh;

/// Side of the square preview in pixels.
pub const PREVIEW_PIXELS: usize = 256;
/// The light direction, normalised once at start-up.
const LIGHT: [f32; 3] = [0.45, 0.75, 0.49];

/// The cross product.
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// The dot product.
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Scale to unit length; a zero vector stays zero.
fn normalise(v: [f32; 3]) -> [f32; 3] {
    let len = dot(v, v).sqrt();
    if len < 1.0e-12 {
        return v;
    }
    [v[0] / len, v[1] / len, v[2] / len]
}

/// A rasterised preview.
#[derive(Debug, Clone, PartialEq)]
pub struct Preview {
    /// RGBA, `PREVIEW_PIXELS` square.
    pub rgba: Vec<u8>,
    /// How many pixels carry a drawn triangle.
    pub covered: usize,
}

/// Rasterise a mesh into a square RGBA preview.
///
/// The triangles come from the payload's index list, three indices at a time.
/// A model whose index count is not a multiple of three has a ragged tail; the
/// extra indices are dropped rather than read as a triangle, because glTF's
/// `TRIANGLES` mode does the same and a half-triangle is not a face.
pub fn render(mesh: &ModelMesh) -> Preview {
    let width = PREVIEW_PIXELS;
    let height = PREVIEW_PIXELS;
    let mut rgba = vec![0u8; width * height * 4];
    let mut depth = vec![-f32::INFINITY; width * height];
    let mut covered = 0usize;

    // Fit the projection to the mesh's own extents, so a 0.2-unit prop and a
    // 200-unit one both fill the frame.
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for v in &mesh.positions {
        for axis in 0..3 {
            if v[axis] < min[axis] {
                min[axis] = v[axis];
            }
            if v[axis] > max[axis] {
                max[axis] = v[axis];
            }
        }
    }
    let centre = [
        (min[0] + max[0]) * 0.5,
        (min[1] + max[1]) * 0.5,
        (min[2] + max[2]) * 0.5,
    ];
    let extent = (max[0] - min[0])
        .max(max[1] - min[1])
        .max(max[2] - min[2])
        .max(1.0e-6);
    let scale = (width.min(height) as f32) * 0.42 / extent;
    let half = width.min(height) as f32 * 0.5;

    // The light is normalised once, because the constants above are its
    // direction in "enough light to see the shapes" units rather than a unit
    // vector.
    let light = normalise(LIGHT);

    // A three-quarter view. The payload stores world-space positions, so the
    // mesh's own axes are the scene's; looking straight down one axis would put
    // every horizontal face edge-on and draw nothing, which is why the eye sits
    // off the corner instead of on an axis.
    let eye = normalise([1.0, 0.8, 1.0]);
    let right = normalise(cross([0.0, 1.0, 0.0], eye));
    let up = cross(eye, right);
    let project = |p: [f32; 3]| -> (f32, f32, f32) {
        let d = [
            p[0] - centre[0],
            p[1] - centre[1],
            p[2] - centre[2],
        ];
        let sx = dot(d, right) * scale + half;
        let sy = half - dot(d, up) * scale;
        let sz = dot(d, eye);
        (sx, sy, sz)
    };

    let triangles = mesh.indices.len() / 3;
    for t in 0..triangles {
        let corners: Vec<[f32; 3]> = (0..3)
            .map(|k| {
                let index = usize::from(mesh.indices[t * 3 + k]);
                mesh.positions.get(index).copied().unwrap_or([0.0; 3])
            })
            .collect();
        // The face normal, for a flat shade. Back-face culling is left out so a
        // single-sided or inverted model still draws something.
        let (a, b, c) = (corners[0], corners[1], corners[2]);
        let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let mut n = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        let n_len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        if !n_len.is_finite() || n_len < 1.0e-12 {
            continue; // a degenerate face has no orientation
        }
        n = [n[0] / n_len, n[1] / n_len, n[2] / n_len];
        let facing = n[0] * light[0] + n[1] * light[1] + n[2] * light[2];
        // Two-sided shading: the absolute value keeps the far side of a
        // single-sided mesh visible instead of black.
        let lambert = facing.abs().clamp(0.0, 1.0);
        let shade = 0.28 + 0.72 * lambert;
        let colour = [
            (200.0 * shade).clamp(0.0, 255.0) as u8,
            (206.0 * shade).clamp(0.0, 255.0) as u8,
            (216.0 * shade).clamp(0.0, 255.0) as u8,
        ];

        let (ax, ay, az) = project(a);
        let (bx, by, bz) = project(b);
        let (cx, cy, cz) = project(c);
        // A triangle wholly outside the frame is skipped.
        if [ax, bx, cx].iter().all(|&x| x < 0.0 || x >= width as f32)
            || [ay, by, cy].iter().all(|&y| y < 0.0 || y >= height as f32)
        {
            continue;
        }

        let min_x = ax.min(bx).min(cx).max(0.0) as usize;
        let max_x = (ax.max(bx).max(cx).min(width as f32 - 1.)) as usize;
        let min_y = ay.min(by).min(cy).max(0.0) as usize;
        let max_y = (ay.max(by).max(cy).min(height as f32 - 1.)) as usize;
        let area = (bx - ax) * (cy - ay) - (by - ay) * (cx - ax);
        if area.abs() < 1.0e-9 {
            continue; // an edge-on triangle covers no pixels
        }
        let inv = 1.0 / area;

        for py in min_y..=max_y {
            for px in min_x..=max_x {
                let sx = px as f32 + 0.5;
                let sy = py as f32 + 0.5;
                // Barycentric weights.
                let w0 = ((bx - ax) * (sy - ay) - (by - ay) * (sx - ax)) * inv;
                let w1 = ((cx - bx) * (sy - by) - (cy - by) * (sx - bx)) * inv;
                let w2 = ((ax - cx) * (sy - cy) - (ay - cy) * (sx - cx)) * inv;
                if w0 < -1.0e-6 || w1 < -1.0e-6 || w2 < -1.0e-6 {
                    continue;
                }
                let z = w0 * az + w1 * bz + w2 * cz;
                let at = py * width + px;
                if z <= depth[at] {
                    continue;
                }
                depth[at] = z;
                let base = at * 4;
                rgba[base] = colour[0];
                rgba[base + 1] = colour[1];
                rgba[base + 2] = colour[2];
                rgba[base + 3] = 255;
            }
        }
    }

    for alpha in rgba.chunks(4) {
        if alpha[3] != 0 {
            covered += 1;
        }
    }

    Preview { rgba, covered }
}
