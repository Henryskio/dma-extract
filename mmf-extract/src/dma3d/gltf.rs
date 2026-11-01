//! Write a `.mod` mesh out as a glTF 2.0 document.
//!
//! glTF was chosen over glb because the animation and the material are the
//! parts of this format that are *not* measured yet (`docs/DMA3D.md` §3), and
//! glTF's text form is where those belong -- a binary blob would have to be
//! produced before they can be corrected.
//!
//! The document is **self-contained**: the geometry is one base64 data URI and
//! each material's texture is a base64 PNG data URI. One file per model, no
//! sidecar `.bin` and no folder to keep together, which is what a desktop user
//! wants: drop the `.gltf` into any viewer.
//!
//! ```text
//! asset / scene / node / mesh / primitive
//!   attributes.POSITION  VEC3 f32, min and max
//!   attributes.NORMAL    VEC3 f32
//!   indices              u16, one-based indices read from the payload minus 1
//!   material             the texture table, when the payload names one
//! ```
//!
//! The indices in the payload are **one-based** (measured: every index is
//! `>= 1` and `< V`, and `V` itself never appears), so the writer subtracts
//! one. glTF's index base is zero.

use crate::dma3d::modfile::ModelMesh;
use anyhow::{Context, Result, bail};
use std::path::Path;

/// The base64 alphabet, RFC 4648.
const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Encode bytes as base64, with padding.
fn base64(data: &[u8]) -> String {
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(BASE64[(n >> 18) as usize & 63] as char);
        out.push(BASE64[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            BASE64[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            BASE64[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// Encode RGBA pixels as a PNG, in memory.
pub fn encode_png(rgba: &[u8], width: usize, height: usize) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, width as u32, height as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().context("PNG header")?;
        writer.write_image_data(rgba).context("PNG image data")?;
    }
    Ok(out)
}

/// Decode base64, ignoring nothing but rejecting malformed input.
///
/// The inverse of [`base64`]. Whitespace is skipped and padding is optional in
/// orientation but the length of the core groups decides how many bytes the
/// final group carries.
fn base64_decode(text: &str) -> Result<Vec<u8>> {
    fn value(c: u8) -> Option<u8> {
        BASE64.iter().position(|&b| b == c).map(|v| v as u8)
    }
    let mut sextets: Vec<u8> = Vec::new();
    for c in text.bytes() {
        if c == b'=' || c.is_ascii_whitespace() {
            continue;
        }
        let v = value(c).with_context(|| format!("not a base64 character: {:?}", c as char))?;
        sextets.push(v);
    }
    let mut out = Vec::with_capacity(sextets.len() / 4 * 3 + 3);
    for group in sextets.chunks(4) {
        let n = group.iter().fold(0u32, |acc, &s| (acc << 6) | u32::from(s));
        // A full group is 24 bits in three bytes. A *short* group is left
        // aligned, so its bytes sit higher in the word: three sextets carry 18
        // bits (two bytes at shifts 10 and 2) and two sextets carry 12 bits
        // (one byte at shift 4). Reading a short group with the full group's
        // shifts silently corrupts the tail of every buffer.
        let shifts: &[u32] = match group.len() {
            4 => &[16, 8, 0],
            3 => &[10, 2],
            2 => &[4],
            _ => bail!("a base64 group of {} sextets", group.len()),
        };
        for &shift in shifts {
            out.push(((n >> shift) & 0xff) as u8);
        }
    }
    Ok(out)
}

/// A minimal JSON reader.
///
/// glTF is a JSON document and the project writes one, so reading one back --
/// which is how the written output is verified in `tests/dma3d.rs` -- has to be
/// possible. There is no JSON dependency available to this build, and a
/// hand-rolled reader for the document this project emits is smaller and more
/// honest than reaching for a half-supported crate.
mod json {
    use anyhow::{Context, Result, bail};
    /// A decoded JSON value.
    #[derive(Debug, Clone, PartialEq)]
    pub enum Value {
        Null,
        Bool(bool),
        Number(f64),
        String(String),
        Array(Vec<Value>),
        Object(Vec<(String, Value)>),
    }

    impl Value {
        /// Look a key up in an object.
        pub fn get(&self, key: &str) -> Option<&Value> {
            match self {
                Value::Object(entries) => entries.iter().find(|(k, _)| k == key).map(|(_, v)| v),
                _ => None,
            }
        }
        /// An unsigned integer field.
        pub fn as_u64(&self) -> Option<u64> {
            match self {
                Value::Number(n) if *n >= 0.0 => Some(*n as u64),
                _ => None,
            }
        }
        /// An array index.
        pub fn as_usize(&self) -> Option<usize> {
            self.as_u64().map(|n| n as usize)
        }
        /// A string field.
        pub fn as_str(&self) -> Option<&str> {
            match self {
                Value::String(s) => Some(s),
                _ => None,
            }
        }
        /// An array field.
        pub fn as_array(&self) -> Option<&[Value]> {
            match self {
                Value::Array(a) => Some(a),
                _ => None,
            }
        }
    }

    /// Parse a whole JSON document.
    pub fn parse(text: &str) -> Result<Value> {
        let bytes = text.as_bytes();
        let mut at = 0usize;
        let value = parse_value(bytes, &mut at)?;
        skip(bytes, &mut at);
        if at != bytes.len() {
            bail!("trailing content at byte {at}");
        }
        Ok(value)
    }

    fn skip(b: &[u8], at: &mut usize) {
        while *at < b.len() && b[*at].is_ascii_whitespace() {
            *at += 1;
        }
    }

    fn expect(b: &[u8], at: &mut usize, c: u8) -> Result<()> {
        skip(b, at);
        if *at >= b.len() || b[*at] != c {
            bail!("expected {:?} at byte {}", c as char, at);
        }
        *at += 1;
        Ok(())
    }

    fn parse_value(b: &[u8], at: &mut usize) -> Result<Value> {
        skip(b, at);
        if *at >= b.len() {
            bail!("unexpected end of input");
        }
        match b[*at] {
            b'{' => parse_object(b, at),
            b'[' => parse_array(b, at),
            b'"' => Ok(Value::String(parse_string(b, at)?)),
            b't' => literal(b, at, "true", Value::Bool(true)),
            b'f' => literal(b, at, "false", Value::Bool(false)),
            b'n' => literal(b, at, "null", Value::Null),
            _ => parse_number(b, at),
        }
    }

    fn literal(b: &[u8], at: &mut usize, word: &str, value: Value) -> Result<Value> {
        if b[*at..].starts_with(word.as_bytes()) {
            *at += word.len();
            Ok(value)
        } else {
            bail!("expected the literal {word} at byte {at}");
        }
    }

    fn parse_string(b: &[u8], at: &mut usize) -> Result<String> {
        expect(b, at, b'"')?;
        let mut out = String::new();
        loop {
            if *at >= b.len() {
                bail!("unterminated string");
            }
            match b[*at] {
                b'"' => {
                    *at += 1;
                    return Ok(out);
                }
                b'\\' => {
                    *at += 1;
                    if *at >= b.len() {
                        bail!("unterminated escape");
                    }
                    match b[*at] {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            if *at + 4 >= b.len() {
                                bail!("truncated \\u escape");
                            }
                            let hex = std::str::from_utf8(&b[*at + 1..*at + 5])
                                .context("\\u escape is not hex")?;
                            let code = u32::from_str_radix(hex, 16).context("\\u escape")?;
                            out.push(char::from_u32(code).unwrap_or('\u{fffd}'));
                            *at += 4;
                        }
                        other => bail!("unknown escape {:?}", other as char),
                    }
                    *at += 1;
                }
                _ => {
                    // Copy one UTF-8 sequence, so a multi-byte character is not
                    // split.
                    let start = *at;
                    let width = utf8_width(b[*at]);
                    *at += width;
                    let end = (*at).min(b.len());
                    out.push_str(&String::from_utf8_lossy(&b[start..end]));
                }
            }
        }
    }

    fn utf8_width(first: u8) -> usize {
        match first {
            0x00..=0x7f => 1,
            0xc0..=0xdf => 2,
            0xe0..=0xef => 3,
            _ => 4,
        }
    }

    fn parse_number(b: &[u8], at: &mut usize) -> Result<Value> {
        let start = *at;
        if *at < b.len() && (b[*at] == b'-' || b[*at] == b'+') {
            *at += 1;
        }
        while *at < b.len()
            && (b[*at].is_ascii_digit()
                || b[*at] == b'.'
                || b[*at] == b'e'
                || b[*at] == b'E'
                || b[*at] == b'-'
                || b[*at] == b'+')
        {
            *at += 1;
        }
        let text = std::str::from_utf8(&b[start..*at]).context("number is not UTF-8")?;
        let n: f64 = text
            .parse()
            .with_context(|| format!("bad number {text:?}"))?;
        Ok(Value::Number(n))
    }

    fn parse_array(b: &[u8], at: &mut usize) -> Result<Value> {
        expect(b, at, b'[')?;
        let mut items = Vec::new();
        skip(b, at);
        if *at < b.len() && b[*at] == b']' {
            *at += 1;
            return Ok(Value::Array(items));
        }
        loop {
            items.push(parse_value(b, at)?);
            skip(b, at);
            if *at >= b.len() {
                bail!("unterminated array");
            }
            match b[*at] {
                b',' => {
                    *at += 1;
                }
                b']' => {
                    *at += 1;
                    return Ok(Value::Array(items));
                }
                _ => bail!("expected ',' or ']' at byte {}", at),
            }
        }
    }

    fn parse_object(b: &[u8], at: &mut usize) -> Result<Value> {
        expect(b, at, b'{')?;
        let mut entries: Vec<(String, Value)> = Vec::new();
        skip(b, at);
        if *at < b.len() && b[*at] == b'}' {
            *at += 1;
            return Ok(Value::Object(entries));
        }
        loop {
            let key = parse_string(b, at)?;
            expect(b, at, b':')?;
            let value = parse_value(b, at)?;
            entries.push((key, value));
            skip(b, at);
            if *at >= b.len() {
                bail!("unterminated object");
            }
            match b[*at] {
                b',' => {
                    *at += 1;
                }
                b'}' => {
                    *at += 1;
                    return Ok(Value::Object(entries));
                }
                _ => bail!("expected ',' or '}}' at byte {}", at),
            }
        }
    }
}

/// Write the part of a `.mod` payload the glTF does not carry.
///
/// The mesh decoder reads the index list, the vertices and the normals, and
/// stops. What follows it -- `u32 0`, `u32 1`, `u32 V`, then the texture
/// coordinates and the animation tracks -- is **not decoded**
/// (`docs/DMA3D.md` §3.3), so writing only the mesh would drop it and the
/// modern output would be lossy for models.
///
/// So the tail is written next to the glTF, under its own name, verbatim. A
/// modern model is therefore the mesh plus the bytes still closed, and the
/// round trip is: glTF for the geometry, `.tail` for the rest, and
/// concatenating them reproduces the payload the game stored. Nothing is
/// invented and nothing is thrown away.
pub fn write_tail(path: &Path, tail: &[u8]) -> Result<()> {
    std::fs::write(path, tail).with_context(|| format!("writing {}", path.display()))
}

/// The filename a payload's undecoded tail is written under, beside the glTF
/// for the same resource.
pub fn tail_name(stem: &str) -> String {
    format!("{stem}.tail")
}

/// A glTF document read back off disk, for the tests that verify the writer.
#[derive(Debug, Clone, PartialEq)]
pub struct ReadBack {
    /// The asset version.
    pub version: String,
    /// The generator.
    pub generator: Option<String>,
    /// Index list, zero-based.
    pub indices: Vec<u16>,
    /// Positions.
    pub positions: Vec<[f32; 3]>,
    /// Normals.
    pub normals: Vec<[f32; 3]>,
    /// How many materials the document declares.
    pub material_count: usize,
}

/// Read a glTF document written by [`write_gltf`] back off disk.
pub fn read_gltf(path: &Path) -> Result<ReadBack> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let document = json::parse(&text)?;

    let version = document
        .get("asset")
        .and_then(|a| a.get("version"))
        .and_then(|v| v.as_str())
        .with_context(|| format!("{}: no asset.version", path.display()))?
        .to_string();
    let generator = document
        .get("asset")
        .and_then(|a| a.get("generator"))
        .and_then(|v| v.as_str())
        .map(str::to_string);

    let material_count = document
        .get("materials")
        .and_then(|m| m.as_array())
        .map(|a| a.len())
        .unwrap_or(0);

    let accessors = document
        .get("accessors")
        .and_then(|a| a.as_array())
        .with_context(|| format!("{}: no accessors", path.display()))?;
    let buffer_views = document
        .get("bufferViews")
        .and_then(|a| a.as_array())
        .with_context(|| format!("{}: no bufferViews", path.display()))?;
    let buffers = document
        .get("buffers")
        .and_then(|a| a.as_array())
        .with_context(|| format!("{}: no buffers", path.display()))?;
    let uri = buffers
        .first()
        .and_then(|b| b.get("uri"))
        .and_then(|u| u.as_str())
        .with_context(|| format!("{}: no buffer uri", path.display()))?;
    let data = uri
        .strip_prefix("data:application/octet-stream;base64,")
        .with_context(|| format!("{}: buffer uri is not a base64 data uri", path.display()))?;
    let binary = base64_decode(data)?;
    let declared = buffers
        .first()
        .and_then(|b| b.get("byteLength"))
        .and_then(|n| n.as_u64())
        .with_context(|| format!("{}: no buffer byteLength", path.display()))?;
    if binary.len() < declared as usize {
        bail!(
            "{}: buffer holds {} bytes but declares {}",
            path.display(),
            binary.len(),
            declared
        );
    }

    let primitives = document
        .get("meshes")
        .and_then(|m| m.as_array())
        .and_then(|m| m.first())
        .and_then(|m| m.get("primitives"))
        .and_then(|p| p.as_array())
        .and_then(|p| p.first())
        .with_context(|| format!("{}: no mesh primitive", path.display()))?;

    let attributes = primitives
        .get("attributes")
        .with_context(|| format!("{}: no attributes", path.display()))?;
    let position_index = attributes
        .get("POSITION")
        .and_then(|i| i.as_usize())
        .with_context(|| format!("{}: no POSITION", path.display()))?;
    let normal_index = attributes
        .get("NORMAL")
        .and_then(|i| i.as_usize())
        .with_context(|| format!("{}: no NORMAL", path.display()))?;
    let index_index = primitives
        .get("indices")
        .and_then(|i| i.as_usize())
        .with_context(|| format!("{}: no indices", path.display()))?;

    let scalars = |index: usize| -> Result<Vec<f32>> {
        let accessor = accessors
            .get(index)
            .with_context(|| format!("accessor {index} does not exist"))?;
        let view = buffer_views
            .get(
                accessor
                    .get("bufferView")
                    .and_then(|v| v.as_usize())
                    .with_context(|| {
                        format!("{}: accessor {index} has no bufferView", path.display())
                    })?,
            )
            .with_context(|| format!("bufferView for accessor {index} does not exist"))?;
        let offset = view.get("byteOffset").and_then(|o| o.as_u64()).unwrap_or(0) as usize;
        let count = accessor
            .get("count")
            .and_then(|c| c.as_usize())
            .with_context(|| format!("{}: accessor {index} has no count", path.display()))?;
        let component = accessor
            .get("componentType")
            .and_then(|c| c.as_u64())
            .with_context(|| {
                format!("{}: accessor {index} has no componentType", path.display())
            })?;
        let width = match component {
            5123 => 2,
            5126 => 4,
            other => bail!("component type {other} is not supported"),
        };
        // `count` is a count of *elements*, and a VEC3 element is three
        // components, so a VEC3 accessor holds count*3 scalars.
        let parts = match accessor.get("type").and_then(|t| t.as_str()) {
            Some("SCALAR") => 1,
            Some("VEC3") => 3,
            other => bail!("accessor type {other:?} is not supported"),
        };
        let mut out = Vec::with_capacity(count * parts);
        for k in 0..count * parts {
            let at = offset + k * width;
            let raw = match component {
                5123 => f64::from(u16::from_le_bytes([binary[at], binary[at + 1]])),
                _ => {
                    let bytes = [binary[at], binary[at + 1], binary[at + 2], binary[at + 3]];
                    f64::from(f32::from_le_bytes(bytes))
                }
            };
            out.push(raw as f32);
        }
        Ok(out)
    };

    let index_count = accessors
        .get(index_index)
        .and_then(|a| a.get("count"))
        .and_then(|c| c.as_usize())
        .with_context(|| format!("{}: index accessor has no count", path.display()))?;
    let index_scalars = scalars(index_index)?;
    let indices: Vec<u16> = index_scalars
        .iter()
        .take(index_count)
        .map(|f| *f as u16)
        .collect();
    let positions_flat = scalars(position_index)?;
    let normals_flat = scalars(normal_index)?;
    if positions_flat.len() % 3 != 0 || normals_flat.len() % 3 != 0 {
        bail!("{}: accessor length is not a multiple of 3", path.display());
    }
    let positions = positions_flat
        .chunks(3)
        .map(|c| [c[0], c[1], c[2]])
        .collect();
    let normals = normals_flat.chunks(3).map(|c| [c[0], c[1], c[2]]).collect();

    Ok(ReadBack {
        version,
        generator,
        indices,
        positions,
        normals,
        material_count,
    })
}

/// A JSON-escaped string, for the `name` fields glTF documents are read by.
fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A material to attach to the mesh: a texture image, already encoded.
pub struct Material {
    /// The texture's name, which becomes the material's name.
    pub name: String,
    /// The PNG bytes of the texture.
    pub png: Vec<u8>,
}

impl Material {
    /// Build one from a decoded texture.
    pub fn from_texture(name: &str, rgba: &[u8], width: usize, height: usize) -> Result<Self> {
        Ok(Self {
            name: name.to_string(),
            png: encode_png(rgba, width, height)?,
        })
    }
}

/// The layout of one accessor's binary payload inside the shared buffer.
struct Accessor {
    /// Byte offset into the buffer.
    offset: usize,
    /// Byte length.
    length: usize,
    /// The glTF component type (5126 f32, 5123 u16).
    component_type: u32,
    /// The glTF type scalar/vec3.
    kind: &'static str,
    /// How many components.
    count: usize,
    /// The min and max of a POSITION accessor.
    min_max: Option<[f32; 6]>,
}

impl Accessor {
    /// The accessor's JSON object.
    fn json(&self, buffer_view: usize) -> String {
        let mut fields = vec![
            format!("  \"bufferView\": {buffer_view}"),
            format!("  \"componentType\": {}", self.component_type),
            format!("  \"count\": {}", self.count),
            format!("  \"type\": {}", json_string(self.kind)),
        ];
        if let Some([minx, miny, minz, maxx, maxy, maxz]) = self.min_max {
            fields.push(format!(
                "  \"min\": [{minx}, {miny}, {minz}],\n  \"max\": [{maxx}, {maxy}, {maxz}]"
            ));
        }
        format!(" {{\n{}\n }}", fields.join(",\n  "))
    }
}

/// Write a mesh out as a self-contained glTF 2.0 document.
///
/// `materials` supplies the textures in payload order; the mesh references
/// them all through its single primitive. When there are none the primitive is
/// written with no material, which is a plain-viewable untextured model.
pub fn write_gltf(path: &Path, name: &str, mesh: &ModelMesh, materials: &[Material]) -> Result<()> {
    // ---- the binary buffer: indices, then positions, then normals ----
    let mut buffer = Vec::new();
    let pad4 = |buf: &mut Vec<u8>| {
        while buf.len() % 4 != 0 {
            buf.push(0);
        }
    };

    // The payload's indices are already zero-based (`docs/DMA3D.md` §3.2:
    // every one of the 87 meshes carries a zero, and none carries its own
    // vertex count), so glTF reads them straight through. A subtler point:
    // write them unchanged, and a payload that *does* use one-based indices
    // shows up in the viewer as an out-of-range index rather than vanishing.
    pad4(&mut buffer);
    let indices_start = buffer.len();
    for &i in &mesh.indices {
        buffer.extend_from_slice(&i.to_le_bytes());
    }
    let indices_len = buffer.len() - indices_start;

    pad4(&mut buffer);
    let positions_start = buffer.len();
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for v in &mesh.positions {
        for (axis, value) in v.iter().enumerate() {
            if value.is_finite() {
                if *value < min[axis] {
                    min[axis] = *value;
                }
                if *value > max[axis] {
                    max[axis] = *value;
                }
            }
        }
    }
    for v in &mesh.positions {
        for value in v {
            buffer.extend_from_slice(&value.to_le_bytes());
        }
    }
    let positions_len = buffer.len() - positions_start;

    pad4(&mut buffer);
    let normals_start = buffer.len();
    for n in &mesh.normals {
        for value in n {
            buffer.extend_from_slice(&value.to_le_bytes());
        }
    }
    let normals_len = buffer.len() - normals_start;
    pad4(&mut buffer);

    // ---- accessors and their buffer views ----
    let accessors = vec![
        Accessor {
            offset: indices_start,
            length: indices_len,
            component_type: 5123,
            kind: "SCALAR",
            count: mesh.indices.len(),
            min_max: None,
        },
        Accessor {
            offset: positions_start,
            length: positions_len,
            component_type: 5126,
            kind: "VEC3",
            count: mesh.positions.len(),
            min_max: Some([min[0], min[1], min[2], max[0], max[1], max[2]]),
        },
        Accessor {
            offset: normals_start,
            length: normals_len,
            component_type: 5126,
            kind: "VEC3",
            count: mesh.normals.len(),
            min_max: None,
        },
    ];

    let buffer_views: Vec<String> = accessors
        .iter()
        .map(|a| {
            format!(
                " {{\n  \"buffer\": 0,\n  \"byteOffset\": {},\n  \"byteLength\": {}\n }}",
                a.offset, a.length
            )
        })
        .collect();
    let accessor_json: Vec<String> = accessors
        .iter()
        .enumerate()
        .map(|(i, a)| a.json(i))
        .collect();

    // ---- materials, textures and images ----
    let mut materials_json = Vec::new();
    let mut textures_json = Vec::new();
    let mut images_json = Vec::new();
    for (i, m) in materials.iter().enumerate() {
        images_json.push(format!(
            " {{\n  \"name\": {},\n  \"uri\": \"data:image/png;base64,{}\"\n }}",
            json_string(&m.name),
            base64(&m.png)
        ));
        textures_json.push(format!(" {{\n  \"source\": {i},\n  \"sampler\": 0\n }}"));
        materials_json.push(format!(
            " {{\n  \"name\": {},\n  \"pbrMetallicRoughness\": {{\n   \"baseColorTexture\": {{\n    \"index\": {i}\n   }},\n   \"metallicFactor\": 0.0,\n   \"roughnessFactor\": 1.0\n  }}\n }}",
            json_string(&m.name)
        ));
    }

    // ---- the document ----
    let mut sections: Vec<String> = vec![
        format!("\"asset\": {{\n \"version\": \"2.0\",\n \"generator\": \"mmf-extract\"\n}}"),
        "\"scene\": 0".to_string(),
        "\"scenes\": [\n {\n  \"name\": \"scene\",\n  \"nodes\": [0]\n }\n]".to_string(),
        format!(
            "\"nodes\": [\n {{\n  \"name\": {},\n  \"mesh\": 0\n }}\n]",
            json_string(name)
        ),
        format!(
            "\"meshes\": [\n {{\n  \"name\": {},\n  \"primitives\": [\n   {{\n    \"attributes\": {{\n     \"POSITION\": 1,\n     \"NORMAL\": 2\n    }},\n    \"indices\": 0{}\n   }}\n  ]\n }}\n]",
            json_string(name),
            if materials.is_empty() {
                String::new()
            } else {
                ",\n    \"material\": 0".to_string()
            }
        ),
    ];
    if !materials_json.is_empty() {
        sections.push(format!(
            "\"materials\": [\n {}\n]",
            materials_json.join(",\n ")
        ));
        sections.push(format!(
            "\"textures\": [\n {}\n]",
            textures_json.join(",\n ")
        ));
        sections.push(format!("\"images\": [\n {}\n]", images_json.join(",\n ")));
        sections.push(
            "\"samplers\": [\n {\n  \"magFilter\": 9729,\n  \"minFilter\": 9987,\n  \"wrapS\": 10497,\n  \"wrapT\": 10497\n }\n]".to_string(),
        );
    }
    sections.push(format!(
        "\"accessors\": [\n {}\n]",
        accessor_json.join(",\n")
    ));
    sections.push(format!(
        "\"bufferViews\": [\n {}\n]",
        buffer_views.join(",\n")
    ));
    sections.push(format!(
        "\"buffers\": [\n {{\n  \"byteLength\": {},\n  \"uri\": \"data:application/octet-stream;base64,{}\"\n }}\n]",
        buffer.len(),
        base64(&buffer)
    ));

    let document = format!("{{\n{}\n}}\n", sections.join(",\n"));

    std::fs::write(path, document).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod gltf_tests {
    use super::*;

    /// Base64 must round-trip every length, including the ones that leave a
    /// one- or two-byte tail group. The writer's own encoder was checked
    /// against Python; this pins the decoder written in `read_gltf`.
    #[test]
    fn base64_round_trips_every_length() {
        for n in [0usize, 1, 2, 3, 4, 5, 6, 7, 726, 728, 3060, 6848] {
            let data: Vec<u8> = (0..n).map(|i| (i * 7 + 3) as u8).collect();
            let text = base64(&data);
            let back = base64_decode(&text).expect("decode");
            assert_eq!(back, data, "base64 round-trip failed at length {n}");
        }
    }

    /// The document written by `write_gltf` reads back with the same geometry.
    #[test]
    fn a_written_document_reads_back() {
        let mesh = ModelMesh {
            index_count_offset: 0,
            indices: vec![0, 1, 2, 1, 2, 3],
            positions: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
            ],
            normals: vec![
                [0.0, 1.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [0.0, 0.0, 1.0],
            ],
            tail_header: Some((0, 1, 4)),
        };
        let path = std::env::temp_dir().join("mmf-gltf-selftest.gltf");
        write_gltf(&path, "test", &mesh, &[]).expect("write");
        let back = read_gltf(&path).expect("read back");
        assert_eq!(back.version, "2.0");
        assert_eq!(back.indices, mesh.indices);
        assert_eq!(back.positions, mesh.positions);
        assert_eq!(back.normals, mesh.normals);
    }
}
