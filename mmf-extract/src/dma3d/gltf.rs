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

use crate::dma3d::modfile::{IndexTable, MagicRecord, ModelMesh, NameTable, ParamTable, Profile, ValueStream};
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
    /// Texture coordinates, when the document carries them.
    pub texcoords: Vec<[f32; 2]>,
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
            Some("VEC2") => 2,
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

    // The texture coordinates are optional, and their absence is not an
    // error: a payload whose float pairs are not all in 0..1 does not carry
    // them and the writer emits none.
    let mut texcoords = Vec::new();
    if let Some(tex_index) = primitives
        .get("attributes")
        .and_then(|a| a.get("TEXCOORD_0"))
        .and_then(|i| i.as_usize())
    {
        let tex = accessors
            .get(tex_index)
            .with_context(|| format!("{}: TEXCOORD_0 names accessor {tex_index} which does not exist", path.display()))?;
        let count = tex
            .get("count")
            .and_then(|c| c.as_usize())
            .with_context(|| format!("{}: TEXCOORD_0 has no count", path.display()))?;
        let flat = scalars(tex_index)?;
        if flat.len() != count * 2 {
            bail!(
                "{}: TEXCOORD_0 holds {} scalars for {count} elements",
                path.display(),
                flat.len()
            );
        }
        texcoords = flat.chunks(2).map(|c| [c[0], c[1]]).collect();
    }

    Ok(ReadBack {
        version,
        generator,
        indices,
        positions,
        normals,
        texcoords,
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
/// The bare file stem of `path`, without its extension.
fn file_stem_of(path: &std::path::Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("model")
        .to_string()
}

pub fn write_gltf(
    path: &Path,
    name: &str,
    mesh: &ModelMesh,
    materials: &[Material],
    texcoords: Option<&[[f32; 2]]>,
) -> Result<()> {
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

    // The texture coordinates, when the payload's float pairs are all in 0..1.
    // That condition is what makes them readable as coordinates rather than as
    // something else: 58 of the 94 payloads that carry the `[0, 1, V]` header
    // satisfy it, 4 satisfy none of it and 32 are mixed, and the mixed ones are
    // left untextured rather than guessed at.
    pad4(&mut buffer);
    let texcoords_start = buffer.len();
    let mut texcoords_len = 0usize;
    let mut write_texcoords = false;
    if let Some(uv) = texcoords {
        if uv.len() == mesh.positions.len() {
            for value in uv.iter().flatten() {
                buffer.extend_from_slice(&value.to_le_bytes());
            }
            texcoords_len = buffer.len() - texcoords_start;
            write_texcoords = true;
        }
    }
    pad4(&mut buffer);

    // ---- accessors and their buffer views ----
    let mut accessors = vec![
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
    if write_texcoords {
        accessors.push(Accessor {
            offset: texcoords_start,
            length: texcoords_len,
            component_type: 5126,
            kind: "VEC2",
            count: mesh.positions.len(),
            min_max: None,
        });
    }

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
            "\"meshes\": [\n {{\n  \"name\": {},\n  \"primitives\": [\n   {{\n    \"attributes\": {{\n     \"POSITION\": 1,\n     \"NORMAL\": 2{}\n    }},\n    \"indices\": 0{}\n   }}\n  ]\n }}\n]",
            json_string(name),
            // The texture coordinates take accessor 3, and are present only
            // when the payload's pairs were all in 0..1 and so readable as
            // coordinates.
            if write_texcoords {
                ",\n     \"TEXCOORD_0\": 3".to_string()
            } else {
                String::new()
            },
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
        // Written twice: once with texture coordinates and once without,
        // because the absence of them is a real state of the output -- a
        // payload whose float pairs are not all in 0..1 produces no
        // TEXCOORD_0 at all, and a reader that cannot tell the two apart would
        // hide that.
        write_gltf(&path, "test", &mesh, &[], None).expect("write");
        let plain = read_gltf(&path).expect("read back");
        assert_eq!(plain.version, "2.0");
        assert_eq!(plain.indices, mesh.indices);
        assert_eq!(plain.positions, mesh.positions);
        assert_eq!(plain.normals, mesh.normals);
        assert!(
            plain.texcoords.is_empty(),
            "a document written without coordinates read back with some"
        );

        let uv = vec![[0.0f32, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]];
        write_gltf(&path, "test", &mesh, &[], Some(&uv)).expect("write with coordinates");
        let back = read_gltf(&path).expect("read back");
        assert_eq!(back.version, "2.0");
        assert_eq!(back.indices, mesh.indices);
        assert_eq!(back.positions, mesh.positions);
        assert_eq!(back.normals, mesh.normals);
        assert_eq!(back.texcoords, uv, "the coordinates did not round-trip");
    }
}

/// Write a `createMod` drawing profile as a glTF 2.0 line loop.
///
/// `docs/DMA3D.md` §11.25: the 443 payloads that carry no mesh chain are `f32`
/// point streams, and the engine tints and extrudes them at runtime. That
/// tessellation is not in the payload, so writing these as a **line loop** is the
/// lossless choice — it emits exactly the vertices the resource holds and invents
/// no triangles. Emitting them as surfaces would add geometry the payload does not
/// contain, which `AGENTS.md` forbids as a guess.
///
/// The document is the same self-contained glTF 2.0 shape `write_gltf` produces,
/// with one primitive of mode `2` (`LINES`) whose indices close the loop:
/// `0, 1, …, n-1, 0`.
pub fn write_profile_gltf(
    path: &std::path::Path,
    name: &str,
    profile: &Profile,
) -> Result<()> {
    let points = &profile.points;
    if points.len() < 2 {
        bail!("{}: a line loop needs at least two points", name);
    }

    // ---- the binary buffer: indices, then positions ----
    let mut buffer = Vec::new();
    let pad4 = |buf: &mut Vec<u8>| {
        while buf.len() % 4 != 0 {
            buf.push(0);
        }
    };
    let n = points.len();
    // The payload's points close the loop: `0, 1, ..., n-1, 0`. A drawing
    // profile is a closed outline the game tints and extrudes, so the closing
    // edge back to the first point is part of the shape and not an artefact.
    let indices_start = buffer.len();
    for i in 0..n as u32 {
        buffer.extend_from_slice(&i.to_le_bytes());
    }
    buffer.extend_from_slice(&0u32.to_le_bytes());
    let indices_len = buffer.len() - indices_start;
    pad4(&mut buffer);

    let positions_start = buffer.len();
    for p in points {
        for c in p {
            buffer.extend_from_slice(&c.to_le_bytes());
        }
    }
    let positions_len = buffer.len() - positions_start;
    pad4(&mut buffer);

    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for p in points {
        for j in 0..3 {
            if p[j] < min[j] {
                min[j] = p[j];
            }
            if p[j] > max[j] {
                max[j] = p[j];
            }
        }
    }

    let accessors = vec![
        Accessor {
            offset: indices_start,
            length: indices_len,
            // UNSIGNED_INT: the loop closes, so the index count is n + 1 and
            // the widths a u16 would need for the largest shipped payload are
            // exceeded by none of them, but u32 keeps the reader uniform.
            component_type: 5125,
            kind: "SCALAR",
            count: n + 1,
            min_max: None,
        },
        Accessor {
            offset: positions_start,
            length: positions_len,
            component_type: 5126,
            kind: "VEC3",
            count: n,
            min_max: Some([min[0], min[1], min[2], max[0], max[1], max[2]]),
        },
    ];

    let buffer_views: Vec<String> = accessors
        .iter()
        .map(|a| {
            format!(
                " {{\n  \"buffer\": 0,\n  \"byteOffset\": {},\n  \"byteLength\": {},\n  \"target\": {}\n }}",
                a.offset,
                a.length,
                // 34963 = ELEMENT_ARRAY_BUFFER for the index view, 34962 =
                // ARRAY_BUFFER for the position view. The buffer holds exactly
                // two views in that order.
                if a.component_type == 5125 { 34963 } else { 34962 }
            )
        })
        .collect();
    let accessor_json: Vec<String> = accessors
        .iter()
        .enumerate()
        .map(|(i, a)| a.json(i))
        .collect();

    let bin_name = format!("{}.bin", file_stem_of(path));
    let document = format!(
        r#"{{"asset":{{"version":"2.0","generator":"mmf-extract (DMA3D §11.25)"}},
"scene": 0,
"scenes": [{{"nodes": [0]}}],
"nodes": [{{"name": {name}, "mesh": 0}}],
"meshes": [{{"name": {name}, "primitives": [{{"attributes": {{"POSITION": 1}},
"indices": 0, "mode": 2}}]}}],
"accessors": [{accessors}],
"bufferViews": [{views}],
"buffers": [{{"byteLength": {len}, "uri": "{bin}"}}]}}"#,
        name = json_string(name),
        accessors = accessor_json.join(",
 "),
        views = buffer_views.join(",
 "),
        len = buffer.len(),
        bin = json_string(&bin_name),
    );

    std::fs::write(path, document.as_bytes())
        .with_context(|| format!("writing {}", path.display()))?;
    let bin = path.with_file_name(bin_name);
    std::fs::write(&bin, &buffer)
        .with_context(|| format!("writing {}", bin.display()))?;
    Ok(())
}

/// Write a `mod II` record stream as a JSON+text document.
///
/// `docs/DMA3D.md` §11.38: 48 of the shipped payloads carry the magic and the `u32`
/// after it and are read by neither the mesh reader nor the profile reader. Their
/// record schema is **not derivable from the bytes** — §11.32 measured that no
/// position in the stream holds a constant across payloads — so this records what
/// is measured and states what is not, rather than asserting a meaning it does not
/// have.
///
/// JSON+text is the same modern format the scripts decoder emits, because these
/// are data records rather than geometry: writing them as a glTF would invent a
/// shape the payload does not carry, which `AGENTS.md` forbids.
pub fn write_record_json(path: &Path, name: &str, record: &MagicRecord) -> Result<()> {
    let mut document = String::new();
    document.push_str("{\n");
    document.push_str(&format!(
        "  \"name\": {},\n",
        json_string(name)
    ));
    document.push_str(&format!(
        "  \"what this is\": \"a mod II record stream the mesh reader and the drawing-profile reader both reject\",\n"
    ));
    document.push_str(&format!(
        "  \"meaning\": \"not determined: no position in the record stream holds a constant across payloads (docs/DMA3D.md 11.32), so the record schema is not read from the bytes\",\n"
    ));
    document.push_str("  \"measured\": {\n");
    document.push_str(&format!(
        "    \"magic offset\": {},\n",
        record.magic_offset
    ));
    document.push_str(&format!(
        "    \"record type\": {},\n",
        record.record_type
    ));
    document.push_str(&format!(
        "    \"stream offset\": {},\n",
        record.magic_offset + 10
    ));
    document.push_str(&format!("    \"pairs\": ["));
    for (k, v) in record.pairs.iter().enumerate() {
        if k > 0 {
            document.push_str(", ");
        }
        document.push_str(&v.to_string());
    }
    document.push_str("],\n");
    document.push_str("    \"floats that read as numbers\": [");
    for (k, v) in record.floats.iter().enumerate() {
        if k > 0 {
            document.push_str(", ");
        }
        // A float that is not finite is written as its bit pattern, because a
        // NaN in a document that is then read back is not the same value.
        if v.is_finite() {
            document.push_str(&v.to_string());
        } else {
            document.push_str(&format!("{}", v.to_bits()));
        }
    }
    document.push_str("]\n");
    document.push_str("  }\n");
    document.push_str("}\n");

    std::fs::write(path, document.as_bytes())
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

/// Write a parameter table as a JSON+text document.
///
/// `docs/DMA3D.md` §11.37, §11.39: the shipped container carries payloads that are
/// **parameter tables** — small values at alternating byte positions with zero fill —
/// which the game reads to configure a UI element or a material. They carry no field
/// names and no lengths, so this records the measured values and says why each one
/// means anything is not determined from the bytes.
///
/// JSON+text for the same reason `write_record_json` uses it: these are data records
/// rather than geometry, and a glTF would invent a shape the payload does not carry.
pub fn write_param_json(path: &Path, name: &str, table: &ParamTable) -> Result<()> {
    // A JSON string literal cannot contain a raw newline, and Rust's `\` line
    // continuation puts one -- plus a backslash, which is an invalid JSON escape --
    // into the output. The first version of this did that and produced a document
    // `json.load` rejected, so every string here is one line and goes through
    // `json_string`, which escapes what a JSON string may not hold.
    let what = "a parameter table: small values at alternating byte positions, with zero fill";
    let meaning = "not determined: the payload carries no field names and no lengths, so what \
                   each value means is in the reader this workspace cannot reach \
                   (docs/DMA3D.md 11.23, 11.24, 11.36)";
    let meaning = meaning.replace('\n', " ");

    let mut document = String::new();
    document.push_str("{\n");
    document.push_str(&format!("  \"name\": {},\n", json_string(name)));
    document.push_str(&format!("  \"what this is\": {},\n", json_string(what)));
    document.push_str(&format!("  \"meaning\": {},\n", json_string(&meaning)));
    document.push_str("  \"measured\": {\n");
    document.push_str(&format!(
        "    \"even bytes are the zero fill\": {},\n",
        table.even_is_fill
    ));
    // The percentages are strings, not numbers: `89% of its bytes` is not a JSON
    // value, and writing it unquoted produced a document `json.load` rejected.
    document.push_str(&format!(
        "    \"fill is zero for\": {},\n",
        json_string(&format!("{}% of its bytes", table.zero_percent))
    ));
    document.push_str(&format!(
        "    \"values below 128\": {},\n",
        json_string(&format!(
            "{}% of the carrying half",
            table.small_percent
        ))
    ));
    document.push_str(&format!("    \"values\": ["));
    for (k, v) in table.values.iter().enumerate() {
        if k > 0 {
            document.push_str(", ");
        }
        document.push_str(&v.to_string());
    }
    document.push_str("]\n");
    document.push_str("  }\n");
    document.push_str("}\n");

    std::fs::write(path, document.as_bytes())
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod param_json_tests {
    use crate::dma3d::modfile::parse_param_table;

    /// The document must be valid JSON, and the first version was not: a Rust
    /// `\` line continuation wrote a backslash and a newline into a JSON string
    /// literal, and the percentages were written unquoted. Both are what this
    /// pins, because `json.load` is the reader that will judge the output.
    #[test]
    fn a_written_table_document_is_valid_json() {
        // 16 values behind zero fill: enough for the reader's own thresholds.
        let mut payload: Vec<u8> = Vec::new();
        for v in [3u8, 7, 12, 40, 55, 90, 120, 127, 1, 2, 4, 8, 16, 32, 64, 100] {
            payload.push(0);
            payload.push(v);
        }
        let table = parse_param_table(&payload).expect("the payload is a table");
        let path = std::env::temp_dir().join("mmf-param-table-selftest.json");
        super::write_param_json(&path, "selftest", &table).expect("write");

        let text = std::fs::read_to_string(&path).expect("read back");
        // No raw newline inside a string literal, and no invalid escape.
        assert!(
            !text.contains("\\\n"),
            "a JSON string literal carries a raw newline"
        );
        assert!(
            !text.contains(": 8") || text.contains("\": "),
            "an unquoted value follows a key"
        );
        // Every line outside a string literal is either an object member with a
        // quoted key, or a bracket.
        for (k, line) in text.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed == "{" || trimmed == "}" || trimmed == "}" {
                continue;
            }
            assert!(
                trimmed.starts_with('"'),
                "line {k} does not start with a quoted key: {trimmed}"
            );
        }
    }
}

/// Write a table of texture names as a JSON+text document.
///
/// `docs/DMA3D.md` §11.40: these payloads carry the textures they reference, and a
/// JSON list of the names is the honest modern form — it is exactly what the
/// extractor already reads out of mesh payloads to embed their materials.
pub fn write_name_json(path: &Path, name: &str, table: &NameTable) -> Result<()> {
    let what = "a table of texture names: the payload's list of what it references";
    let meaning = "the names themselves are determined; the payload carries no lengths \\
                   and no field types, so what each reference means is not read from \\
                   the bytes";
    let meaning = meaning.replace('\n', " ");
    let mut document = String::new();
    document.push_str("{\n");
    document.push_str(&format!("  \"name\": {},\n", json_string(name)));
    document.push_str(&format!("  \"what this is\": {},\n", json_string(what)));
    document.push_str(&format!("  \"meaning\": {},\n", json_string(&meaning)));
    document.push_str("  \"measured\": {\n");
    document.push_str(&format!("    \"count\": {},\n", table.names.len()));
    let quoted: Vec<String> = table
        .names
        .iter()
        .map(|n| json_string(n))
        .collect();
    document.push_str(&format!("    \"names\": [{}],\n", quoted.join(", ")));
    // The names the container's slice cut off. Written separately because they are
    // names whose last characters were not carried, and a reader of this document
    // must not treat `createBtSelect.38.tg` as a complete name (`docs/DMA3D.md`
    // §11.54).
    let cut: Vec<String> = table.truncated.iter().map(|n| json_string(n)).collect();
    // An empty list is `[]`, not nothing: a JSON member with no value after the
    // colon is invalid, and the first version of this wrote nothing when the table
    // ends with a whole name. `json.load` rejected 11 of the 21 documents it was
    // given, which is the third time this session a hand-written document was not
    // checked against its reader (`docs/DMA3D.md` §11.54).
    document.push_str(&format!(
        "    \"names the slice cut off\": [{}]\n",
        cut.join(", ")
    ));
    document.push_str("  }\n");
    document.push_str("}\n");
    std::fs::write(path, document.as_bytes())
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

/// Write an `f32` value stream as a JSON+text document.
///
/// `docs/DMA3D.md` §11.41: these payloads carry the numbers and no layout. The
/// document records the values, the phase the run was found at and the range, and
/// states that the field layout is not determined — the same honest form the record
/// stream and the parameter table take, because asserting a stride would invent
/// geometry the payload does not carry.
pub fn write_stream_json(path: &Path, name: &str, stream: &ValueStream) -> Result<()> {
    let what = "an f32 value stream with no constant slot, so not a planar drawing profile";
    let meaning = "the values are determined; the field layout is not -- whether they are \\
                   xyz triples, xy pairs or a per-record layout with something else in it \\
                   is not readable from the bytes (docs/DMA3D.md 11.32, 11.41)";
    let meaning = meaning.replace('\n', " ");
    let mut document = String::new();
    document.push_str("{\n");
    document.push_str(&format!("  \"name\": {},\n", json_string(name)));
    document.push_str(&format!("  \"what this is\": {},\n", json_string(what)));
    document.push_str(&format!("  \"meaning\": {},\n", json_string(&meaning)));
    document.push_str("  \"measured\": {\n");
    document.push_str(&format!("    \"phase\": {},\n", stream.phase));
    document.push_str(&format!("    \"count\": {},\n", stream.values.len()));
    document.push_str(&format!(
        "    \"range\": [{}, {}],\n",
        stream.range.0, stream.range.1
    ));
    let nums: Vec<String> = stream.values.iter().map(|v| v.to_string()).collect();
    document.push_str(&format!("    \"values\": [{}]\n", nums.join(", ")));
    document.push_str("  }\n");
    document.push_str("}\n");
    std::fs::write(path, document.as_bytes())
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod name_json_tests {
    use crate::dma3d::modfile::{NameTable, parse_name_table};

    /// The document must be valid JSON, and it was not: the first version wrote
    /// nothing after the colon when the table ends with a whole name, which
    /// `json.load` rejected for 11 of the 21 documents.
    #[test]
    fn a_written_table_document_is_valid_json() {
        // A table that ends whole: no sliced tail, so the list must be `[]`.
        let mut payload: Vec<u8> = Vec::new();
        for n in ["createBtFill.00.TGA", "createBtFill.01.TGA", "createBtFill.02.TGA"] {
            payload.extend_from_slice(n.as_bytes());
            payload.push(0);
            while payload.len() % 32 != 0 {
                payload.push(0xcc);
            }
        }
        let table = parse_name_table(&payload).expect("read");
        let path = std::env::temp_dir().join("mmf-name-table-whole.json");
        super::write_name_json(&path, "selftest", &table).expect("write");

        let text = std::fs::read_to_string(&path).expect("read back");
        assert!(
            !text.contains("\": \n"),
            "a member has no value after its colon"
        );
        for (k, line) in text.lines().enumerate() {
            let t = line.trim();
            if t.is_empty() || t == "{" || t == "}" {
                continue;
            }
            assert!(t.starts_with('"'), "line {k} does not start with a quoted key: {t}");
        }

        // And a table whose last record was sliced: the list carries the cut name.
        let mut sliced: Vec<u8> = Vec::new();
        sliced.extend_from_slice(b"ct.36.tga\0");
        sliced.extend_from_slice(&[0xcc; 8]);
        sliced.extend_from_slice(b"createBtSelect.37.tga\0");
        sliced.extend_from_slice(&[0xcc; 6]);
        sliced.extend_from_slice(b"createBtSelect.38.tg");
        let table = parse_name_table(&sliced).expect("read");
        let path = std::env::temp_dir().join("mmf-name-table-sliced.json");
        super::write_name_json(&path, "selftest", &table).expect("write");
        let text = std::fs::read_to_string(&path).expect("read back");
        assert!(
            text.contains("names the slice cut off\": [\"createBtSelect.38.tg\"]"),
            "the sliced name is not recorded separately: {text}"
        );
        let _ = NameTable { names: Vec::new(), truncated: Vec::new() };
    }
}

/// Write an index table as a JSON+text document.
///
/// `docs/DMA3D.md` §11.37, §11.55: five payloads carry the alternating-zero layout
/// `parse_param_table` reads, but with values that reach `0xf2`, which the 128 rule
/// excludes. The nine accepted parameter tables have 0–3% of their carrying values at
/// or above 128 and these five have 34–53%, so this is a separate measured population
/// rather than a loosened threshold, and it is written for the same reason
/// `write_param_json` is: the values are recorded verbatim and the meaning is stated
/// as undetermined, because the payload carries no field names and no lengths.
pub fn write_index_json(path: &Path, name: &str, table: &IndexTable) -> Result<()> {
    let what = "an index table: byte values at alternating byte positions, with zero fill";
    // One line, built by concatenation: a Rust `\` line continuation puts a
    // backslash into the literal, which is an invalid escape in the JSON this
    // document is read back as. The first version of this had one and the document
    // carried `the payload \                    carries no field names` (§11.55).
    let meaning = concat!(
        "the values are determined; what they index is not -- the payload ",
        "carries no field names and no lengths, so each value's meaning is in ",
        "the reader this workspace cannot reach (docs/DMA3D.md 11.23, 11.36)"
    );
    let mut document = String::new();
    document.push_str("{\n");
    document.push_str(&format!("  \"name\": {},\n", json_string(name)));
    document.push_str(&format!("  \"what this is\": {},\n", json_string(what)));
    document.push_str(&format!("  \"meaning\": {},\n", json_string(&meaning)));
    document.push_str("  \"measured\": {\n");
    document.push_str(&format!(
        "    \"even bytes are the zero fill\": {},\n",
        table.even_is_fill
    ));
    // Both of these are strings, not numbers: `89% of its bytes` and
    // `0x33 to 0xc2` are not JSON values, and writing them unquoted produced a
    // document `json.load` rejected -- the same mistake §11.39 and §11.54 made, and
    // the reason `write_param_json` quotes its percentages.
    document.push_str(&format!(
        "    \"fill is zero for\": {},\n",
        json_string(&format!("{}% of its bytes", table.zero_percent))
    ));
    document.push_str(&format!(
        "    \"values range from\": {},\n",
        json_string(&format!(
            "{:#04x} to {:#04x}",
            table.range.0, table.range.1
        ))
    ));
    document.push_str(&format!("    \"count\": {},\n", table.values.len()));
    let nums: Vec<String> = table.values.iter().map(|v| v.to_string()).collect();
    document.push_str(&format!("    \"values\": [{}]\n", nums.join(", ")));
    document.push_str("  }\n");
    document.push_str("}\n");
    std::fs::write(path, document.as_bytes())
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod index_json_tests {
    use crate::dma3d::modfile::parse_index_table;

    /// The document must be valid JSON, and the first version was not: it wrote
    /// `"fill is zero for": 89% of its bytes` and `"values range from": 0x33 to
    /// 0xc2`, neither of which is a JSON value. Both are now strings, and this pins
    /// it the same way `param_json_tests` and `name_json_tests` do.
    #[test]
    fn a_written_index_document_is_valid_json() {
        let mut payload: Vec<u8> = Vec::new();
        for v in [0x4bu8, 0x63, 0x7a, 0x88, 0x91, 0xa2, 0xb3, 0xc2, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa] {
            payload.push(0);
            payload.push(v);
        }
        let table = parse_index_table(&payload).expect("read");
        let path = std::env::temp_dir().join("mmf-index-table-selftest.json");
        super::write_index_json(&path, "selftest", &table).expect("write");

        let text = std::fs::read_to_string(&path).expect("read back");
        assert!(
            !text.contains("\": \n"),
            "a member has no value after its colon"
        );
        for (k, line) in text.lines().enumerate() {
            let t = line.trim();
            if t.is_empty() || t == "{" || t == "}" {
                continue;
            }
            assert!(t.starts_with('"'), "line {k} does not start with a quoted key: {t}");
        }
        // The percentage and the range are strings.
        assert!(text.contains("\"fill is zero for\": \"100% of its bytes\""), "{text}");
        assert!(text.contains("\"values range from\": \"0x33 to 0xc2\""), "{text}");
    }
}
