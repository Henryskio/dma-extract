
# Porting specification — `src/script.rs`, `src/film.rs`, `src/dma3d.rs` (+ `src/dma3d/*`) → Lean 4

This document is a **byte-exact porting specification**. It is written so the Lean
modules can be implemented **without reading the Rust**. Every public function is
given with its exact name, signature and return value; every field the code
touches is given with its exact byte offset and endianness; every constant is
named with its value and the doc evidence for it; every error message is
verbatim; every output file is given with its exact name template and exact
bytes; one concrete emitted example per output family is included.

Nothing here is a summary of the algorithm: the arithmetic, the bit twiddling,
the scanning loops and the exact key order of every emitted document are quoted
from the Rust.

**Sources of truth, in order** (when they disagree, the higher one wins):

1. The Rust code in `src/script.rs`, `src/film.rs`, `src/dma3d.rs`,
   `src/dma3d/{texfile,gltf,render,modfile,anim}.rs`, `src/container.rs`,
   `src/picture.rs`, `src/lz77.rs`, `src/cli.rs`.
2. The tests in `tests/{scripts,film_roundtrip,film_3d,dma3d,dma3d_numbers}.rs`
   and the `#[cfg(test)]` modules inside the ported files (they pin the code,
   including where a doc is stale).
3. `docs/SCRIPTS.md`, `docs/FILMS.md`, `docs/DMA3D.md`.
4. The module-level doc comments in the Rust files.

§16 lists every place where a doc comment / `docs/*.md` claim disagrees with the
code, and says which the port must follow.

---

## 0. Scope and module map

| Rust file | Suggested Lean module | Public API to port |
|---|---|---|
| `src/script.rs` | `MmfExtract/Script.lean` | `Layout` (private), `extract_container` |
| `src/film.rs` | `MmfExtract/Film.lean` | `CHUNK_TAG`, `TYPE_AUDIO`, `TYPE_MARKER`, `TYPE_TIMESTAMP`, `TYPE_END`, `TYPE_FULL_FRAME`, `TYPE_FULL_FRAME_V2`, `TYPE_DELTA`, `TYPE_DELTA_V2`, `MAX_RECORD_LEN`, `Chunk`, `AudioChunk`, `Frame`, `Film` (+ `duration_ticks`), `extract_container`, `decode_film` |
| `src/dma3d.rs` | `MmfExtract/Dma3D.lean` | `models_meta_len`, `MODELS_META_TRAILER`, `textures_meta_len`, `Extracted3D`, `Extract3DReport`, `extract_models`, `extract_textures`, `write_index` |
| `src/dma3d/texfile.rs` | `MmfExtract/Dma3D/Texfile.lean` | `PALETTE_ENTRY_BYTES`, `Texture`, `Axis`, `to_rgba`, `entry`, `parse`, `palette_size` |
| `src/dma3d/modfile.rs` | `MmfExtract/Dma3D/Modfile.lean` | `VERTEX_RECORD_BYTES`, `NORMAL_RECORD_BYTES`, `VERTEX_MARKER`, `NORMAL_MARKER`, `ModelMesh` (+ `vertex_count`, `normal_array_offset`, `tail_offset`), `parse_mesh` |
| `src/dma3d/anim.rs` | `MmfExtract/Dma3D/Anim.lean` | `TrackName`, `Records` (+ `remainder_offset`), `locate`, `track_names`, `dump`, `records_name` |
| `src/dma3d/gltf.rs` | `MmfExtract/Dma3D/Gltf.lean` | `encode_png`, `write_tail`, `tail_name`, `ReadBack`, `read_gltf`, `Material` (+ `from_texture`), `write_gltf` |
| `src/dma3d/render.rs` | `MmfExtract/Dma3D/Render.lean` | `PREVIEW_PIXELS`, `Preview`, `render` |

Dependencies the port may assume are already implemented or specified elsewhere
(`PORT-PICTURES.md`, `PORT-SOUND.md`):

* `Container` (all fields, `payload`, `payload_len`, `plain_len`, `name`) —
  §1 below.
* `container::read_u16(d, off, big_endian)` / `read_u32(d, off, big_endian)` —
  §1.2. These **panic on out-of-range**; the ported code relies on that only
  where the Rust does (see §1.3).
* `picture::stride8(width)`, `picture::indexed_rgba_stride(...)`,
  `picture::write_png(path, w, h, rgba)` — signatures in §1.4.
* `lz77::decompress(src)` — signature in §1.5.
* `cli::sanitize(name, index)` — §1.6.

### 0.1 What each entry point is called with, and what it returns

All three entry points take `(container, out_dir, original: bool)`:

| Rust call | `out_dir` content |
|---|---|
| `script::extract_container(&c, dir, original)` | `--original`: `{index:05}_{name}.bin` files. Modern: `container.json`, `types.txt`, `strings.txt`, `objects/{index:05}_{name}.txt` |
| `film::extract_container(&c, dir, original)` | `--original`: `{index:05}_{name}.flm`. Modern: `{index:05}_{stem}/frame_{i:03}.png` + `manifest.json` |
| `dma3d::extract_models(&c, dir, original, texture_dir)` | `--original`: `{index:05}_{stem}.mod`. Modern: `{index:05}_{stem}.gltf` **or** `.mod` (no mesh), plus `.tail`, `.anim.txt`, `.preview.png` |
| `dma3d::extract_textures(&c, dir, original)` | `--original`: `{index:05}_{stem}.tex`. Modern: `{index:05}_{stem}.png` |

`out_dir` itself is created by the caller convention (`<out>/<Kind>/<ContainerStem>/`,
e.g. `Films/DMA/`, `Models/DMA3D/`, `Scripts/HD/`); the module only does
`create_dir_all` on what it needs. `dma3d::write_index(base, texture_dir, model_dir)`
writes `<base>/index.html`.

---

## 1. Shared preconditions

### 1.1 `Container` fields the three modules touch

All multi-byte reads are in the container's own byte order: **big-endian** when
the two bytes at `0x10` are `"MM"`, **little-endian** when they are `"II"`. See
§1.2.

| Field | Type | Where the bytes come from |
|---|---|---|
| `path` | `PathBuf` | the file path (used in every message and in the emitted `source`/`container` fields) |
| `data` | `Vec<u8>` | the whole file image |
| `kind` | `ContainerKind` | tag at `0x05..0x10` → `"Scripts"`/`"3 Script"`/`"Script"` ⇒ `Scripts`; `"Films"` ⇒ `Films`; `"Models"` ⇒ `Models`; `"Textures"` ⇒ `Textures` |
| `big_endian` | `bool` | `data[0x10..0x12] == b"MM"` |
| `version` | `u16` | **u16 at `0x12`**, container order |
| `resource_count` | `u16` | **u16 at `0x22`** (v2/v3) |
| `offsets` | `Vec<u32>` | `N+1` u32s at `0x24`; resource `i` spans `[off[i], off[i+1])`; last == file length |
| `names_end` | `usize` | end of the 32-byte-per-resource names table: `0x24 + 4*(N+1) + 2 + 32*N` |
| `script` | `Option<ScriptMeta>` | `Some` only for `ContainerKind::Scripts` |

Accessors used by this port:

```rust
pub fn payload(&self, i: usize) -> &[u8] {
    let s = self.offsets[i] as usize;
    let e = self.offsets[i + 1] as usize;
    &self.data[s..e]
}
pub fn payload_len(&self, i: usize) -> usize { (self.offsets[i + 1] - self.offsets[i]) as usize }
pub fn name(&self, i: usize) -> &str { self.names.get(i).map(|s| s.as_str()).unwrap_or("") }
```

`ScriptMeta` (needed by `src/script.rs` only) — every field, with the offset it
was read from by `container.rs` (all in the container's byte order, relative to
`metadata_start = 0x24 + 4*(N+1)`):

| Field | Type | Offset in `data` | Size |
|---|---|---|---|
| `version` | `u16` | `metadata_start + 0` | 2 (observed `11`) |
| `unknown_1` | `u16` | `metadata_start + 2` | 2 (observed `0xffff`) |
| `unknown_2` | `u16` | `metadata_start + 4` | 2 (observed `0xffff`) |
| `unknown_3` | `u16` | `metadata_start + 6` | 2 (observed `2490`) |
| `type_count` | `u16` | `metadata_start + 8` | 2 (`T`, observed `89`) |
| `type_aux` | `Vec<u16>` | `metadata_start + 10` | `T × 2` — per-class fixed record size |
| `types` | `Vec<String>` | after `type_aux` | string list: `u16 count`, `u32 blob_len`, `count × u32 offsets`, blob of NUL-terminated strings |
| `element_counts` | `Vec<u32>` | after the type-name list | `N × 4` |
| `object_types` | `Vec<u16>` | after the counts | `N × 2` |
| `object_names` | `Vec<String>` | after the type indices | string list, same wire format; `metadata_end` = its end = `offsets[0]` |
| `metadata_start` | `usize` | — | `0x24 + 4*(N+1)` |
| `metadata_end` | `usize` | — | first payload offset |

Doc evidence, `docs/SCRIPTS.md` §2: "u16 script metadata version (11) / u16
unknown_1 (0xffff) / u16 unknown_2 (0xffff) / u16 unknown_3 (2490) / u16 type
count T (89) / T x u16 type_aux / string list: T type names / N x u32
element_count per object / N x u16 type index per object / string list: N object
names", and §2.2: "u16 count / u32 blob length / count x u32 offsets, relative to
the start of the blob / blob NUL-terminated strings".

### 1.2 `read_u16` / `read_u32` — exact semantics

```rust
pub fn read_u16(d: &[u8], off: usize, big_endian: bool) -> u16 {
    let b = [d[off], d[off + 1]];
    if big_endian { u16::from_be_bytes(b) } else { u16::from_le_bytes(b) }
}
pub fn read_u32(d: &[u8], off: usize, big_endian: bool) -> u32 {
    let b = [d[off], d[off + 1], d[off + 2], d[off + 3]];
    if big_endian { u32::from_be_bytes(b) } else { u32::from_le_bytes(b) }
}
```

`read_u16(data, offset, true)` in `film.rs` is therefore **always big-endian**,
independently of `container.big_endian` — the inner `.flm` stream and the Films
metadata are big-endian in both shipped games.

### 1.3 Panic-on-purpose places (the port must reproduce the checks)

`film.rs` guards every one of these before the read; the port must keep the
guard, not the panic:

| Expression | Guard that precedes it |
|---|---|
| `container.offsets[index]`, `container.offsets[i+1]` | `decode_film` bails on `index >= resource_count`; `extract_container` loops `0..resource_count` |
| `data[rect_at..]`, `data[b1]`, `data[r4..]` | `palettes_start + 1024*palette_count > data.len()` bail, and `rect_at + 8 > palettes_start` bail |
| `payload[168..]` in `decode_delta` | `payload.len() < 168` bail |
| `payload[8..8+lz77_len]` | `lz77_len > payload.len() - 8` bail |
| `payload[168..168+lz77_len]` | `lz77_len > payload.len() - 168` bail |
| `delta[pos+3]` for a pattern | `pos + 4 > delta.len()` bail |
| `frame[start..start+count]` | `checked_span` bail |
| `palette[idx*4 .. idx*4+4]` in `indexed_rgba_stride` | `idx` comes from the frame buffer; `stride*height` is verified equal to the buffer length, and every index is `< palette_count` in every shipped film (asserted in `tests/film_roundtrip.rs`) |

`script.rs` never indexes without `get()`/slicing guards (`payload.get(pos..)`,
`payload[fixed..]` after `fixed <= payload.len()`).

### 1.4 The `picture` helpers this port calls

```rust
pub(crate) fn stride8(width: u16) -> usize { ((width as usize) + 3) / 4 * 4 }
```
Evidence, `docs/FILMS.md` §2: "stride = stride8(546) = 548 (rows padded to a
multiple of four)".

```rust
pub(crate) fn indexed_rgba_stride(
    palette: &[u8], indices: &[u8], width: u16, height: u16, stride: usize,
) -> Vec<u8>
```
Row `y` starts at `y * stride`; if `start >= indices.len()` the whole row is
`[0,0,0,0]`; otherwise the row is `indices[start .. min(start+width, indices.len())]`
expanded through `palette[idx*4 .. idx*4+4]` as `[r, g, b, 255 - t]` where the
stored entry is `[transparency, red, green, blue]`, and any missing tail pixels
are `[0,0,0,0]`. `stride == 0` returns an empty `Vec`.
Doc evidence, `docs/FILMS.md` §9: "RGBA = (palette[i].red, palette[i].green,
palette[i].blue, 255 - palette[i].transparency)".

```rust
pub fn write_png(path: &Path, width: u16, height: u16, rgba: &[u8]) -> Result<()>
```
Requires `rgba.len() >= w*h*4`, else bails
`"{}: picture has {} bytes but {}x{} needs {}"`. Writes an 8-bit RGBA PNG
(whatever `png::Encoder` produces — see §2 on PNG bytes).

### 1.5 `lz77::decompress`

```rust
pub fn decompress(src: &[u8]) -> Result<Vec<u8>>
```
The existing `MmfExtract/Lz77.lean` port; call it with the exact byte slice.
Its error strings are already specified elsewhere (`PORT-SOUNDS.md` / the
LZ77 module); the `with_context` wrappers this port adds are listed per function
below.

### 1.6 `cli::sanitize` — the filename stem

```rust
pub fn sanitize(name: &str, index: usize) -> String {
    let cleaned: String = name.chars().map(|c| {
        if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' { c } else { '_' }
    }).collect();
    let cleaned = cleaned.trim_matches('.').to_string();
    if cleaned.is_empty() { format!("res{index:05}") } else { cleaned }
}
```
Note: `trim_matches('.')` strips **leading and trailing dots only**; interior
dots survive, so `createBtDraw.00.TGA` → `createBtDraw.00.TGA`.

---

## 2. Conventions the port must reproduce exactly

### 2.1 Rust formatting rules that leak into the output

| Rust | Emitted form |
|---|---|
| `{index:05}` | decimal, zero-padded to 5 (`00000`, `01393`) |
| `{i:03}` | decimal, zero-padded to 3 (`frame_000.png`) |
| `{a:04x}` | lowercase hex, at least 4 digits (`0x003c`) |
| `{a:06x}` | lowercase hex, at least 6 digits |
| `{a:08x}` | lowercase hex, at least 8 digits (hexdump addresses) |
| `{a:#x}` | `0x` + lowercase hex, **no padding** (`0xf789`, `0x147460`) |
| `{a:#06x}` | `0x` + lowercase hex, padded so the whole field is 6 wide → `0x` + 4 hex digits (`0xf789`) |
| `{v:>3}`, `{v:>11}`, `{v:>7}`, `{v:>24}` | right-aligned in N |
| `{v:<24}` | left-aligned in N, space-padded on the right |
| `{v:.5}` | fixed 5 decimals (`4.67770`) |
| `{a as i16}` / `{a as i32}` | the value reinterpreted as signed, printed as a signed decimal |
| `{b:02x}` | byte as two lowercase hex digits |
| `{}` on `f32` | Rust's shortest round-trip form; **integral values print without `.0`** (`0`, `1`, `255`) and this is what `write_gltf` puts in `min`/`max` |

### 2.2 anyhow error formatting (affects stderr text)

* `{e}` — only the **outermost** message (the context), no cause chain.
* `{e:#}` — the **full chain**, joined with `": "`.
* `error.to_string()` (used for `Extract3DReport.failed`) — outermost only.

### 2.3 The two JSON escapers are NOT the same

| Module | Escapes |
|---|---|
| `script.rs::json_escape` | `"` → `\"`, `\` → `\\`, `\n`, `\r`, `\t`, `c < 0x20` → `\u%04x` |
| `film.rs::json_escape` | the above **plus** `\u{8}` → `\b` and `\u{c}` → `\f` |
| `dma3d/gltf.rs::json_string` | `script.rs`'s set, and the result is wrapped in `"`…`"` |

`escape_bytes` (`script.rs`, byte-oriented) is a third escaper:

| byte | output |
|---|---|
| `0x5c` `\` | `\\` |
| `0x22` `"` | `\"` |
| `0x00` | `\0` |
| `0x0a` | `\n` |
| `0x0d` | `\r` |
| `0x09` | `\t` |
| `0x20..=0x7e` | the ASCII character itself |
| anything else | `\x%02x` (lowercase) |

(The `strings.txt` header documents exactly this: `# Escapes: \" \\ \0 \n \r \t,
other bytes as \xNN`.)

### 2.4 PNG bytes

`film.rs` writes PNGs through `picture::write_png` (the `png` crate); `dma3d`
writes them through `gltf::encode_png`, which is the same crate in memory:

```rust
pub fn encode_png(rgba: &[u8], width: usize, height: usize) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, width as u32, height as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().context("PNG header")?;
    writer.write_image_data(rgba).context("PNG image data")?;
    Ok(out)
}
```

**No test compares PNG bytes.** Every test that looks at a PNG decodes it and
compares pixels (`tests/film_roundtrip.rs`, `tests/film_3d.rs`,
`tests/dma3d.rs`). The contract the port must meet is therefore:

* 8-bit RGBA, non-interlaced, one IDAT (or a chunking the decoder accepts), each
  row filtered with filter type 0 by default;
* `width`/`height` exactly the mesh or film geometry;
* the decoded bytes equal the palette-expanded pixel array in row-major order.

`encode_png`'s own context strings, which the port must raise on failure:
`PNG header`, `PNG image data`.

---

## 3. Struct inventory

### 3.1 `script.rs`

```rust
enum Layout { Records, Animation, CreditsText, BrushNames, AnimBrush, TextList, StringTable, Opaque }
fn name(self) -> &'static str  // "records" "animation" "credits-text" "brush-names"
                                 // "anim-brush" "text-list" "string-table" "opaque"
```
Private to the module; the Lean port keeps it private but must emit the same
eight lowercase strings, because `container.json`'s `layout` field and the
`layouts` histogram keys are those strings, and `strings.txt`/`types.txt`/the
object dumps depend on the classification.

There are **no public structs** in `script.rs`.

### 3.2 `film.rs` — all public

```rust
pub const CHUNK_TAG: u16 = 0xF789;
pub const TYPE_AUDIO: u16 = 0x02;
pub const TYPE_MARKER: u16 = 0x03;
pub const TYPE_TIMESTAMP: u16 = 0x06;
pub const TYPE_END: u16 = 0x07;
pub const TYPE_FULL_FRAME: u16 = 0x0A;      // third game's key frame
pub const TYPE_FULL_FRAME_V2: u16 = 0x00;   // second game's key frame
pub const TYPE_DELTA: u16 = 0x0B;           // third game's delta
pub const TYPE_DELTA_V2: u16 = 0x01;        // second game's delta
pub const MAX_RECORD_LEN: usize = 26;

pub struct Chunk { pub offset: usize, pub kind: u16, pub len: usize }
pub struct AudioChunk {
    pub chunk_offset: usize, pub chunk_len: usize,
    pub data_len: u32,     // the block's first u32 = PCM byte count
    pub index: u32,        // the block's third u32; u32::MAX marks a sync block
    pub data_offset: usize,
    pub sync: bool,
}
pub struct Frame {
    pub start_tick: u32, pub duration_tick: u32, pub duplicate: bool,
    pub indices: Vec<u8>,          // stride * height raw 8bpp indices
    pub timestamp_chunk: Option<usize>,
    pub delta_chunk: Option<usize>,
    pub full_chunk: Option<usize>,
    pub delta_rects: Vec<[u16; 4]>, // [left, top, right, bottom], advisory
}
pub struct Film {
    pub index: usize, pub name: String,
    pub width: u16, pub height: u16, pub stride: usize,
    pub bounds: [u16; 4],          // [top, left, bottom, right] from the record
    pub record: Vec<u8>,           // the 26 reassembled bytes
    pub palette: Vec<u8>,          // 1024 bytes
    pub palette_count: u16,
    pub frames: Vec<Frame>, pub chunks: Vec<Chunk>,
    pub chunk_kinds: BTreeMap<u16, usize>,
    pub audio: Vec<AudioChunk>,
    pub unique_frames: usize,
}
impl Film { pub fn duration_ticks(&self) -> u64 { self.frames.iter().map(|f| f.duration_tick as u64).sum() } }

struct FilmMeta { record, bounds, width, height, stride, palette, palette_count }  // private
```

Constants' evidence, `src/film.rs` doc comments: "The inner stream starts with a
`0xF789` chunk tag"; "`0x02` audio block: 12-byte header + PCM + padding";
"`0x03` marker (4 zero bytes in this game)"; "`0x06` frame timestamp, one
big-endian `u32`"; "`0x07` end of stream"; "`0x0A` full 8bpp frame,
LZ77-compressed"; "`0x0B` delta frame: 20 rects + LZ77 + cursor-relative RLE";
"Key frame chunk. The third game writes type `0x0A`, the second game `0x00`";
"Delta chunk. The third game writes type `0x0B`, the second game `0x01`";
"Longest per-film record seen in a Films container. The third game's record is 26
bytes; the second game's is 22. The parser solves for the actual length from the
file layout, so this is only a buffer bound."

### 3.3 `dma3d.rs`

```rust
pub const MODELS_META_TRAILER: usize = 100;
pub fn models_meta_len(n: usize) -> usize { 4 * n + MODELS_META_TRAILER }
pub fn textures_meta_len(n: usize) -> usize { 6 * n + 2 }

pub struct Extracted3D { pub index: usize, pub name: String, pub path: PathBuf, pub bytes: usize }
pub struct Extract3DReport {
    pub written: usize,
    pub failed: Vec<(usize, String, String)>,  // (index, name, error message)
    pub resources: Vec<Extracted3D>,
}
```
`Extract3DReport` derives `Default` (all zero/empty).

### 3.4 `dma3d/texfile.rs`

```rust
pub const PALETTE_ENTRY_BYTES: usize = 4;
const WIDTH_CANDIDATES: [usize; 6] = [8, 16, 32, 64, 128, 256];   // private

pub struct Texture {
    pub palette: Vec<[u8; 4]>,  // file order; index i is palette[i]
    pub indices: Vec<u8>,       // row-major, `width` to a row
    pub width: usize, pub height: usize,
    pub axis: Axis, pub undecidable: bool,
}
pub enum Axis { Square, Wide, Tall }
impl Texture {
    pub fn to_rgba(&self) -> Vec<u8>                       // 4 bytes per index
    pub fn entry(&self, index: u8) -> [u8; 4]              // or [0,0,0,0]
}
struct Shape { components: usize, cut: usize }             // private
```

### 3.5 `dma3d/modfile.rs`

```rust
pub const VERTEX_RECORD_BYTES: usize = 16;
pub const NORMAL_RECORD_BYTES: usize = 16;
pub const VERTEX_MARKER: f32 = 1.0;
pub const NORMAL_MARKER: f32 = 0.0;
const MARKER_EPSILON: f32 = 1e-6;        // private
const UNIT_LENGTH_EPSILON: f32 = 1e-3;   // private

pub struct ModelMesh {
    pub index_count_offset: usize,   // where the u32 C was read from
    pub indices: Vec<u16>,           // zero-based exactly as stored
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub tail_header: Option<(u32, u32, u32)>,
}
impl ModelMesh {
    pub fn vertex_count(&self) -> usize { self.positions.len() }
    pub fn normal_array_offset(&self) -> usize {
        self.index_count_offset + 4 + 2 * self.indices.len() + 4 + 16 * self.positions.len()
    }
    pub fn tail_offset(&self) -> usize {
        self.normal_array_offset() + 4 + 16 * self.normals.len()
    }
}
```

### 3.6 `dma3d/anim.rs`

```rust
pub struct TrackName { pub name: String, pub at: usize }
pub struct Records {
    pub tail_offset: usize,
    pub header: Option<(u32, u32, u32)>,
    pub pairs_in_unit_range: Option<usize>,  // Some(count) when all pairs are in 0..1
    pub pair_count: usize,
    pub remainder: Vec<u8>,
    pub tracks: Vec<TrackName>,
}
impl Records { pub fn remainder_offset(&self) -> usize { self.tail_offset + 12 + 8 * self.pair_count } }
```

### 3.7 `dma3d/gltf.rs`

```rust
pub struct Material { pub name: String, pub png: Vec<u8> }
impl Material { pub fn from_texture(name: &str, rgba: &[u8], width: usize, height: usize) -> Result<Self> }

pub struct ReadBack {
    pub version: String, pub generator: Option<String>,
    pub indices: Vec<u16>, pub positions: Vec<[f32; 3]>, pub normals: Vec<[f32; 3]>,
    pub texcoords: Vec<[f32; 2]>, pub material_count: usize,
}
struct Accessor { offset, length, component_type: u32, kind: &'static str, count: usize, min_max: Option<[f32; 6]> }  // private

mod json {  // private
    pub enum Value { Null, Bool(bool), Number(f64), String(String), Array(Vec<Value>), Object(Vec<(String, Value)>) }
    impl Value { get, as_u64, as_usize, as_str, as_array }
    pub fn parse(text: &str) -> Result<Value>
}
```

### 3.8 `dma3d/render.rs`

```rust
pub const PREVIEW_PIXELS: usize = 256;
const LIGHT: [f32; 3] = [0.45, 0.75, 0.49];   // private
pub struct Preview { pub rgba: Vec<u8>, pub covered: usize }
```

---

# PART A — `src/script.rs`

## A1. `extract_container`

```rust
pub fn extract_container(container: &Container, out_dir: &Path, original: bool) -> Result<usize>
```

Returns the number of objects written (not bytes).

Control flow:

1. If `container.kind != ContainerKind::Scripts` → `bail!("{}: expected a Scripts container, found {:?}", container.path.display(), container.kind)`.
   (`{:?}` on `ContainerKind` is the variant name, e.g. `Films`.)
2. `meta = container.script.as_ref()` or `with_context(|| format!("{}: missing script metadata", container.path.display()))`.
3. `std::fs::create_dir_all(out_dir)` with context `format!("creating {}", out_dir.display())`.
4. `original` → `write_originals`, else `write_modern`.

## A2. `write_originals`

```rust
fn write_originals(container: &Container, out_dir: &Path) -> Result<usize>
```

For `index in 0..container.resource_count as usize`:

* `name = sanitize(container.name(index), index)`
* `file = out_dir.join(format!("{index:05}_{name}.bin"))`
* write `container.payload(index)` verbatim; on error `eprintln!("warning: could not write {} (object {index} '{}'): {e}", file.display(), container.name(index))` and `failed += 1`.

After the loop, if `failed > 0`:
`eprintln!("warning: {} of {} script object payloads could not be written", failed, container.resource_count)`.

Returns `written`. **Never returns `Err`** except from `create_dir_all` in the
caller. Doc evidence, `docs/SCRIPTS.md` §6: "The file contains exactly the bytes
in `[off[i], off[i+1])`. This is byte-for-byte identical to the container slice
and to the payload the loader allocates, because Scripts payloads are not
compressed."

Filename template: `{index:05}_{sanitize(name, index)}.bin`.

## A3. `write_modern`

```rust
fn write_modern(container: &Container, meta: &ScriptMeta, out_dir: &Path) -> Result<usize>
```

1. `objects_dir = out_dir.join("objects")`; `create_dir_all` with context `format!("creating {}", objects_dir.display())`.
2. `n = container.resource_count as usize`.
3. `type_counts = vec![0usize; meta.types.len()]`; for each `t` in `meta.object_types`, `if let Some(count) = type_counts.get_mut(t as usize) { *count += 1 }` — **out-of-range type indices are silently skipped**.
4. One classification pass over `0..n`, collecting:
   * `layouts: Vec<Layout>` (index-aligned),
   * `layout_counts: BTreeMap<&'static str, usize>` keyed by `layout.name()` (so the JSON histogram is sorted by the layout name, not by insertion),
   * `payload_bytes += container.payload_len(index)`,
   * if `layout == Opaque`: `opaque_bytes += len`; else `(count, record_size) = element_info(meta, index)`, `fixed = count * record_size`, `record_bytes += fixed`, `tail_bytes += len - fixed`.
5. `write_container_json(...)`, `write_types_txt(...)`, `write_strings_txt(...)` — each `?`-propagated.
6. Per-object loop: `file = objects_dir.join(format!("{index:05}_{name}.txt"))`; a closure creates the file (context `format!("creating {}", file.display())`), wraps it in a `BufWriter`, calls `write_object_dump(&mut writer, container, meta, index, layouts[index])` with context `format!("writing {}", file.display())`, then `writer.flush()`. On error:
   `eprintln!("warning: script object {index} ('{}'): {e:#}", container.name(index))` — **`{e:#}` prints the whole anyhow chain**.
7. If `failed > 0`: `eprintln!("warning: {} of {} script object dumps could not be written", failed, n)`.
8. Returns `written`.

Measured totals for `HD.MMS` (`docs/SCRIPTS.md` §3): `payload_bytes 3,329,791`,
`record_bytes 3,282,998`, `tail_bytes 46,793`, `opaque_bytes 0`.

## A4. `element_info` / `type_info`

```rust
fn element_info(meta: &ScriptMeta, index: usize) -> (usize, usize)
fn type_info(meta: &ScriptMeta, index: usize) -> Option<(usize, &str, usize)>
```

```rust
let count = meta.element_counts.get(index).copied().unwrap_or(0) as usize;
let record_size = meta.object_types.get(index)
    .and_then(|&t| meta.type_aux.get(t as usize))
    .copied().unwrap_or(0) as usize;
```
(`element_info` returns `(count, record_size)`.)

```rust
let type_index = *meta.object_types.get(index)? as usize;
let name = meta.types.get(type_index)?;
let record_size = *meta.type_aux.get(type_index)? as usize;
```
(`type_info` returns `Some((type_index, name, record_size))`.)

## A5. `classify`

```rust
fn classify(container: &Container, meta: &ScriptMeta, index: usize) -> Layout
```

Order matters; reproduce it exactly:

1. `type_info(meta, index)` → `None` ⇒ `Opaque`.
2. `(count, _) = element_info(meta, index)`; `payload = container.payload(index)`.
3. **`record_size == 0`** ⇒ `if payload.is_empty() { Records } else { Opaque }`.
4. `fixed = count.checked_mul(record_size)`; `None`, or `fixed > payload.len()` ⇒ `Opaque`.
   `tail = &payload[fixed..]`.
5. Match `type_name`:

| type name | condition | result | otherwise |
|---|---|---|---|
| `"Animation"` | `nul_strings_exact(tail, 1).is_some()` | `Animation` | `Opaque` |
| `"creditsTextEntry"` | `nul_strings_exact(tail, 1).is_some()` | `CreditsText` | `Opaque` |
| `"StdBrush"` | `nul_strings_exact(tail, count).is_some()` | `BrushNames` | `Opaque` |
| `"AnimBrush"` | `record_size == 8` **and** `anim_brush_entries(payload, count, big_endian).is_some()` **and** `nul_strings_exact(tail, count).is_some()` | `AnimBrush` | `Opaque` |
| `"TextList"` | `text_list_entries(payload, count, big_endian).is_some()` | `TextList` | `Opaque` |
| `"StringTable"` | `!tail.is_empty() && count > 0` | `StringTable` | `Opaque` |
| anything else | `fixed == payload.len()` | `Records` | `Opaque` |

Note the asymmetries the port must keep: `StringTable` is accepted on the tail
length alone (the dump then prints `# could not decode StringTable` if
`string_table_entries` fails), while `TextList`/`AnimBrush` are validated up
front. `"StringTable"`'s type name is a class name, and per `docs/SCRIPTS.md` §5
`StringTable` is type index 42 with record size 10.

## A6. Shared decoders

```rust
fn nul_strings_exact(data: &[u8], expected: usize) -> Option<Vec<&[u8]>>
```
`pos = 0`; `expected` times: `rest = data.get(pos..)?` (so a short tail ⇒ `None`),
`end = rest.iter().position(|&b| b == 0)?`, push `&rest[..end]`, `pos += end + 1`.
`Some(strings)` iff `pos == data.len()` at the end, else `None`.

```rust
fn string_table_entries<'a>(payload: &'a [u8], count: usize, big_endian: bool)
    -> Option<(Vec<(u16, u32, u32)>, &'a [u8])>
```
`fixed = count.checked_mul(10)?`; `count == 0` or `fixed > payload.len()` ⇒ `None`.
For `j in 0..count`: `record = &payload[j*10 .. j*10+10]`; push
`(read_u16(record, 0, be), read_u32(record, 2, be), read_u32(record, 6, be))`.
Returns `(entries, &payload[fixed..])`.

```rust
fn string_at_offset(blob: &[u8], offset: u32) -> Option<&[u8]>
```
`start = offset as usize`; `start > blob.len()` ⇒ `None`; `end = rest[..].position(0)?`
⇒ `Some(&rest[..end])`. Note: an offset exactly at `blob.len()` reaches an empty
string and succeeds.

```rust
fn anim_brush_entries(payload: &[u8], count: usize, big_endian: bool)
    -> Option<(Vec<(u16, u16, u32)>, &[u8])>
```
`fixed = count.checked_mul(8)?`; `count == 0` or `fixed > payload.len()` ⇒ `None`.
`blob = &payload[fixed..]`. For `j in 0..count`: `record = &payload[j*8 .. j*8+8]`,
`name_offset = read_u16(record, 2, be)`, and `string_at_offset(blob, name_offset as u32)?`
— **a name offset that does not resolve rejects the whole payload**. Push
`(read_u16(record, 0, be), name_offset, read_u32(record, 4, be))`.

```rust
fn text_list_entries<'a>(payload: &'a [u8], count: usize, big_endian: bool)
    -> Option<(Vec<u32>, &'a [u8])>
```
`fixed = count.checked_mul(4)?`; `count == 0` or `fixed > payload.len()` ⇒ `None`.
`blob = &payload[fixed..]`. For `j in 0..count`:
`offset = read_u32(payload, j*4, be)`; `string_at_offset(blob, offset)?`; push `offset`.

Doc evidence for the four payload shapes, `docs/SCRIPTS.md` §4.2/§4.3/§4.4:
"`AnimBrush` records are 8 bytes: u16 reference object index / u16 name offset /
u32 flags"; "`StringTable` … u16 id / u32 offset into the shared blob / u32
reserved (always 0 in this file)"; "`TextList` payloads contain `count` u32
offsets followed by a shared NUL-string blob".

## A7. `write_container_json`

```rust
fn write_container_json(container, meta, out_dir, layouts, type_counts, layout_counts,
                        payload_bytes, record_bytes, tail_bytes, opaque_bytes) -> Result<()>
```

Path: `out_dir/container.json`, via `File::create` (context
`format!("creating {}", path.display())`) + `BufWriter`.

Exact emitted text, in this order (each line via `writeln!`; `\n` line endings,
LF only, final line ends with `\n`):

```json
{
  "format": "MMFW Scripts",
  "source": "<json_escape(container.path.display())>",
  "endian": "<big|little>",
  "container_version": <container.version>,
  "resource_count": <container.resource_count>,
  "script_metadata": {
    "version": <meta.version>,
    "unknown_fields": [<unknown_1>, <unknown_2>, <unknown_3>],
    "type_count": <meta.type_count>,
    "offset": <meta.metadata_start>,
    "end": <meta.metadata_end>,
    "bytes": <meta.metadata_end - meta.metadata_start>
  },
  "coverage": {
    "payload_bytes": <payload_bytes>,
    "record_bytes": <record_bytes>,
    "tail_bytes": <tail_bytes>,
    "opaque_bytes": <opaque_bytes>,
    "layouts": {
      "<name>": <count>,      // BTreeMap order: anim-brush, animation, brush-names,
    }                         // credits-text, records, string-table, text-list
  },
  "types": [
    {"index": <i>, "name": "<escaped>", "record_size": <type_aux[i] or 0>, "object_count": <type_counts[i] or 0>},
    ...
  ],
  "objects": [
    {"index": <i>, "name": "<escaped raw name>", "type_index": <t|0>, "type": "<escaped type name|\"\">",
     "record_size": <rs|0>, "elements": <count>, "payload_offset": <container.offsets[i]>,
     "payload_size": <container.payload_len(i)>, "layout": "<layout name|opaque>",
     "file": "objects/<i:05>_<safe>.txt"},
    ...
  ]
}
```

Details the port must not lose:

* `endian` is `"big"` when `container.big_endian` is true, `"little"` otherwise.
* `types` uses `meta.type_aux.get(index).copied().unwrap_or(0)` and `type_counts.get(index).copied().unwrap_or(0)`; one object per line, comma after every line except the last, indented 4 spaces.
* `objects` uses `type_info(meta, index).unwrap_or((0, "", 0))` (**empty string, not `"?"`**), `element_info(meta, index)`'s count, `container.offsets[index]` (the raw `u32`), `container.payload_len(index)`, and `layouts.get(index).copied().unwrap_or(Layout::Opaque).name()`; `file` uses `sanitize(container.name(index), index)`.
* Every string goes through `json_escape` (§2.3) — except the two `format`-literal keys.
* The document ends `  ]\n}\n`.

## A8. `write_types_txt`

```rust
fn write_types_txt(container, meta, out_dir, type_counts) -> Result<()>
```

Path `out_dir/types.txt`. Exact content:

```
# Scripts type table for <container.path.display()>
# idx  record_size  objects  name
<row>
```

where each row is `format!("{:>3}  {:>11}  {:>7}  {}", index, type_aux.get(index).unwrap_or(0), type_counts.get(index).unwrap_or(0), name)` — i.e. **two literal spaces between every column**, the index right-aligned in 3, the record size right-aligned in 11, the object count right-aligned in 7, and the name unescaped. No trailing newline beyond the last row's `\n`.

Example rows (real `HD.MMS` values, `docs/SCRIPTS.md` §5):

```
# Scripts type table for /RawAssets/HD.MMS
# idx  record_size  objects  name
  0           10     2783  Animation
  1           10        7  DMAPicture
 42           10        1  StringTable
 53           48       46  StdBrush
```

## A9. `write_strings_txt`

```rust
fn write_strings_txt(container, meta, out_dir, layouts) -> Result<()>
```

Path `out_dir/strings.txt`. Exact structure:

```
# Strings decoded from <container.path.display()>
# Escapes: \" \\ \0 \n \r \t, other bytes as \xNN
                                   <- one empty line (writeln!(w) with no args)
[type names]
type[<index:3>] "<escape_bytes(name)>"      <- for every meta.types entry
                                   <- empty line
[object names]
object[<index:04>] "<escape_bytes(name)>"   <- for every object 0..N
                                   <- empty line
[payload strings]
<per-object lines, see below>
                                   <- empty line
# <total> payload strings decoded
```

`escape_bytes` is §2.3's byte escaper; the quotes around the strings are literal
and are themselves escaped by `escape_bytes` when the data contains one.

Per-object lines, by `layouts.get(index).unwrap_or(Opaque)`:

| Layout | Code path | Format |
|---|---|---|
| `StringTable` | `string_table_entries(payload, count, be)`; skip if `None`. For each `(id, offset, _)`: **skip when `matches!(offset, 0xfffd..=0xffff)`**; if `string_at_offset(blob, offset)` is `Some(s)` emit and `total += 1` | `StringTable[{index:04}] "{name}" id={id} offset=0x{offset:04x} "{escape_bytes(s)}"` |
| `CreditsText` | recompute `(count, record_size)`, `fixed = count*record_size`; `nul_strings_exact(&payload[fixed..], 1)`; use `strings[0]` | `creditsTextEntry[{index:04}] "{name}" "{escape_bytes(s)}"` |
| `TextList` | `text_list_entries(payload, count, be)`; for `(k, offset)` in order: `string_at_offset` → emit, `total += 1` | `TextList[{index:04}] "{name}" [{k}] offset=0x{offset:04x} "{escape_bytes(s)}"` |
| `BrushNames` | `(count, record_size)`, `fixed`; `nul_strings_exact(&payload[fixed..], count)`; for `(k, s)`: emit, `total += 1` | `StdBrush[{index:04}] "{name}" [{k}] "{escape_bytes(s)}"` |
| `AnimBrush` | `anim_brush_entries(payload, count, be)`; for `(k, (_, offset, _))`: `string_at_offset` → emit, `total += 1` | `AnimBrush[{index:04}] "{name}" [{k}] offset=0x{offset:04x} "{escape_bytes(s)}"` |
| any other | nothing | — |

`name` is `escape_bytes(container.name(index).as_bytes())`, computed once per
object. `total` counts only lines actually written; the trailing line is
`# {total} payload strings decoded` (measured `632` for `HD.MMS`,
`docs/SCRIPTS.md` §7 / `tests/scripts.rs`).

## A10. `write_object_dump`

```rust
fn write_object_dump<W: Write>(w, container, meta, index, layout) -> Result<()>
```

Header, five lines, then the body:

```
# Object <index>: <container.name(index)>          <- raw, unescaped
# Type: <type_name> (index <type_index>)           <- "?" when type_info is None
# Elements: <count> x <record_size> bytes = <fixed>
# Payload: <payload.len()> bytes at container offset 0x<container.offsets[index]:x>
# Layout: <layout.name()>
```

* `type_name` placeholder is **`"?"`** here (contrast `write_container_json`'s `""`).
* `fixed = count * record_size` with `type_info(...).unwrap_or((0, "?", 0))` and `element_info`'s count.

If `layout == Opaque`:

```
# No structural decode; full verbatim hexdump follows.
<hexdump of the whole payload, base 0>
```
and `return Ok(())` — **the final `# All N payload bytes…` line is not printed for
Opaque**.

Otherwise, per layout:

| Layout | Calls |
|---|---|
| `Records` | `write_records(w, payload, count, record_size, type_name, be)` |
| `Animation` | `write_records` + `write_nul_tail(w, &payload[fixed..], 1, "object name", fixed)` |
| `CreditsText` | `write_records` + `write_nul_tail(w, &payload[fixed..], 1, "credits text", fixed)` |
| `BrushNames` | `write_records` + `write_nul_tail(w, &payload[fixed..], count, "brush names", fixed)` |
| `AnimBrush` | `write_anim_brush(w, payload, count, fixed, be)` |
| `TextList` | `write_text_list(w, payload, count, be)` |
| `StringTable` | `write_string_table(w, payload, count, be)` |

then always:

```
# All <payload.len()> payload bytes are represented above.
```

`Opaque` in the inner match is `unreachable!()` (it returned earlier).

### A10.1 `write_records`

```rust
fn write_records<W: Write>(w, payload, count, record_size, type_name, big_endian) -> std::io::Result<()>
```

```
## Fixed records (<count> x <record_size> bytes)
# [index] raw bytes | decoded fields
```
then for `j in 0..count`: `start = j * record_size`, `record = &payload[start .. start+record_size]`,
`write!(w, "  [{j:4}] {}", hex_bytes(record))`, `write_record_fields(w, record, type_name, be)`, `writeln!(w)`.
So each record line is `  [   0] <hex bytes> | <fields>` and the hex bytes are
space-separated lowercase pairs.

### A10.2 `write_record_fields` — every branch, in order

`match record.len()`:

| `record.len()` | Emitted text (appended after the hex bytes, with a leading space) |
|---|---|
| `2` | ` | u16 <a> (i16 <a as i16>)` with `a = read_u16(record, 0, be)` |
| `4` | ` | u32 <a> (i32 <a as i32>)` with `a = read_u32(record, 0, be)` |
| `8` | ` | u16 <a0> <a1> <a2> <a3> | u32 <b0> <b1>` with the four u16 at 0,2,4,6 and the two u32 at 0,4 |
| `10` **and `type_name == "Animation"`** | ` | op <op> args <a0> <a1> <a2> <a3>` with `op` at 0 and the four at 2,4,6,8 |
| `10` (other types) | ` | u16 <a0> <a1> <a2> <a3> <a4>` at 0,2,4,6,8 |
| `12` | ` | u16 <a0..a5> | u32 <b0> <b1> <b2>` at 0,2,4,6,8,10 and 0,4,8 |
| `len` where `len % 2 == 0` | ` | u16` then, for `k in 0..len/2`, ` <read_u16(record, 2*k, be)>` |
| odd length | nothing |

Doc evidence for the `Animation` reading, `docs/SCRIPTS.md` §3.1: "record size 10
bytes = five u16 values; the first u16 takes small values (observed 0–14, mostly
0–7); the remaining four u16 values look like arguments".

Example (object 187 `ISbigBALa`, `docs/SCRIPTS.md` §3.1):

```
  [   5] 05 00 01 00 53 00 00 00 00 00 | op 5 args 1 83 0 0
  [   6] 06 00 00 00 01 00 14 00 f5 ff | op 6 args 0 1 20 65525
```

### A10.3 `write_nul_tail`

```rust
fn write_nul_tail<W: Write>(w, tail, expected, label, base) -> std::io::Result<()>
```

```
## Tail: <label> (<expected> NUL-terminated string(s), <tail.len()> bytes)
```
then for `k in 0..expected`:
`rest = &tail[pos..]`, `end = rest.position(0).unwrap_or(rest.len())`,
`writeln!(w, "  [{k}] +0x{pos:04x} len {end:3} \"{}\"", escape_bytes(&rest[..end]))`, `pos += end + 1`.

Then, if `pos < tail.len()`:
```
  trailing <tail.len() - pos> raw bytes:
<hexdump of &tail[pos..] with base = base + pos>
```

`len {:3}` is **right-aligned in 3** (so `len   7`), and the `+0x` prefix is
literal. Note this loop is *not* bounds-checked like `nul_strings_exact`: a tail
with fewer than `expected` strings simply produces a short final string.

### A10.4 `write_anim_brush`

```rust
fn write_anim_brush<W: Write>(w, payload, count, fixed, big_endian) -> std::io::Result<()>
```

* `anim_brush_entries(payload, count, be)` ⇒ `None` ⇒ `writeln!(w, "# could not decode AnimBrush entries")` then `write_hexdump(w, payload, 0)` and return that result.
* Otherwise:

```
## AnimBrush entries (<count> x 8 bytes)
# [index] raw bytes | ref, name_offset and flags
  [<j:4>] <hex of payload[j*8 .. j*8+8]> | ref=<reference> name_offset=0x<offset:04x> flags=0x<flags:x>[ -> "<escaped name>" | -> <outside name blob>]
## Name blob (<fixed> = 0x<fixed:x>, <blob.len()> bytes)
<hexdump of blob with base = fixed>
```

`flags=0x{flags:x}` is **unpadded** lowercase hex. The name suffix is
` -> "<escape_bytes(name)>"` when `string_at_offset` resolves, else
` -> <outside name blob>`.

### A10.5 `write_text_list`

```rust
fn write_text_list<W: Write>(w, payload, count, big_endian) -> std::io::Result<()>
```

* `None` ⇒ `# could not decode TextList` + hexdump of the payload at base 0.
* Otherwise `fixed = count * 4`:

```
## String offsets (<count> x 4 bytes; relative to the string blob)
  [<k>] <hex of payload[k*4 .. k*4+4]> | offset=0x<offset:04x>[ -> "<escaped>" | -> <outside string blob>]
## Shared string blob (<fixed> = 0x<fixed:x>, <blob.len()> bytes; raw hexdump)
<hexdump of blob, base = fixed>
```

### A10.6 `write_string_table`

```rust
fn write_string_table<W: Write>(w, payload, count, big_endian) -> std::io::Result<()>
```

* `None` ⇒ `# could not decode StringTable` + hexdump at base 0.
* Otherwise:

```
## StringTable entries (<count> x 10 bytes; id, offset, reserved)
  [<j:3>] <hex of payload[j*10 .. j*10+10]> | id=<id> offset=0x<offset:04x> reserved=<reserved><suffix>
## Shared string blob (<blob.len()> bytes, raw hexdump)
<hexdump of blob, base = count * 10>
```

`<suffix>` by `offset`:

| offset | suffix |
|---|---|
| `0xffff` | ` -> <sentinel 0xffff: absent>` |
| `0xfffe` | ` -> <sentinel 0xfffe: absent>` |
| `0xfffd` | ` -> <sentinel 0xfffd: absent>` |
| else, resolves | ` -> "<escaped>"` |
| else | ` -> <offset outside blob>` |

The index here is `{j:3}` — **width 3, not 4** (the only writer that uses 3).
Doc evidence, `docs/SCRIPTS.md` §4.3: "entries 42–44 use offset sentinels
`0xffff`, `0xfffe`, `0xfffd` instead of strings — these three entries have no
text".

### A10.7 `hex_bytes` and `write_hexdump`

```rust
fn hex_bytes(data: &[u8]) -> String
```
Space-separated `{b:02x}`, single space between bytes, no trailing space.

```rust
fn write_hexdump<W: Write>(w: &mut W, data: &[u8], base: usize) -> std::io::Result<()>
```
Empty `data` ⇒ no output at all. Otherwise, for `(line, chunk)` in
`data.chunks(16).enumerate()`:

```
  <base + line*16 :08x>  <16 slots> |<ascii>|
```

* The address is `format!("{:08x}", base + line * 16)` followed by **two spaces**.
* Then 16 slots: `Some(b)` ⇒ `write!(w, "{b:02x} ")`; `None` ⇒ `write!(w, "   ")`
  (three spaces). So each slot is exactly 3 characters wide and there is a
  trailing space after the last real byte.
* Then `write!(w, " |")`.
* The ASCII column: `0x20..0x7f` ⇒ the character, else `.`.
* Then `chunk.len()..16` spaces (padding so the `|` lines up).
* Then `|` and a newline.

Example (the `WhippedCream` name blob, base 40 = 0x28):

```
  00000028  57 68 69 70 70 65 64 20 43 72 65 61 6d 31 00 57                    |Whipped Cream1.W|
  00000038  68 69 70 70 65 64 20 43 72 65 61 6d 32 00 57 68                    |hipped Cream2.Wh|
```

### A10.8 A complete object dump, verbatim

Real `HD.MMS` object 1393 `WhippedCream` (`AnimBrush`, count 5, record size 8,
payload 115 bytes = 40 fixed + 75 tail), `docs/SCRIPTS.md` §4.2 and
`tests/scripts.rs`:

```
# Object 1393: WhippedCream
# Type: AnimBrush (index 55)
# Elements: 5 x 8 bytes = 40
# Payload: 115 bytes at container offset 0x<a real container offset, {:x}>
# Layout: anim-brush
## AnimBrush entries (5 x 8 bytes)
# [index] raw bytes | ref, name_offset and flags
  [   0] 6d 05 00 00 00 00 00 00 | ref=1389 name_offset=0x0000 flags=0x0 -> "Whipped Cream1"
  [   1] 6e 05 0f 00 00 00 00 00 | ref=1390 name_offset=0x000f flags=0x0 -> "Whipped Cream2"
  [   2] 6f 05 1e 00 00 00 00 00 | ref=1391 name_offset=0x001e flags=0x0 -> "Whipped Cream3"
  [   3] 70 05 2d 00 00 00 00 00 | ref=1392 name_offset=0x002d flags=0x0 -> "Whipped Cream4"
  [   4] 71 05 3c 00 00 00 00 00 | ref=1393 name_offset=0x003c flags=0x0 -> "Whipped Cream5"
## Name blob (40 = 0x28, 75 bytes)
  00000028  57 68 69 70 70 65 64 20 43 72 65 61 6d 31 00 57                    |Whipped Cream1.W|
  00000038  68 69 70 70 65 64 20 43 72 65 61 6d 32 00 57 68                    |hipped Cream2.Wh|
  00000048  69 70 70 65 64 20 43 72 65 61 6d 33 00 57 69 70                    |ipped Cream3.Whi|
  00000058  70 70 65 64 20 43 72 65 61 6d 34 00 57 68 69 70                    |pped Cream4.Whip|
  00000068  70 65 64 20 43 72 65 61 6d 35 00                                   |ped Cream5.     |
# All 115 payload bytes are represented above.
```

(The only line that depends on the container is the `# Payload:` line; everything
else is byte-fixed given the payload.)

## A11. `escape_bytes` / `json_escape` (script.rs)

Both are §2.3; they are pure functions of their input. `json_escape` operates on
`&str` chars; `escape_bytes` on `&[u8]`.

---

# PART B — `src/film.rs`

## B1. `extract_container`

```rust
pub fn extract_container(container: &Container, out_dir: &Path, original: bool) -> Result<usize>
```

1. `container.kind != ContainerKind::Films` ⇒ `bail!("{}: expected a Films container, found {:?}", container.path.display(), container.kind)`.
2. `create_dir_all(out_dir)` with context `format!("creating {}", out_dir.display())`.
3. `original` ⇒ for each `index`: `safe = crate::cli::sanitize(container.name(index), index)`,
   `file = out_dir.join(original_filename(index, &safe))`, `std::fs::write(&file, container.payload(index))`
   with context `format!("writing {}", file.display())`, `written += 1`, `return Ok(written)`.
   **A single write failure aborts the whole run with `Err`.**
4. Otherwise for each `index`: `film = decode_film(container, index)?` (**also aborts the run**),
   `safe = sanitize(&film.name, index)`, `dir = out_dir.join(format!("{index:05}_{}", strip_flm(&safe)))`,
   `create_dir_all(&dir)` with context `format!("creating {}", dir.display())`,
   `write_film_modern(container, &film, &dir)?`, `written += 1`.
5. Returns `written` — **films handled, not frames**.

## B2. `is_film_v2` / `full_frame_kind`

```rust
fn is_film_v2(container: &Container) -> bool { container.version == 2 }
fn full_frame_kind(container: &Container) -> u16 {
    if is_film_v2(container) { TYPE_FULL_FRAME_V2 } else { TYPE_FULL_FRAME }
}
```
Note: `is_film_v2` is **only** used to select the chunk types and the two
version-specific decode paths; everything else in `decode_film` is shared. A
version-3 container (including the 3D game's `DMA3D.MMF`) takes the third game's
path.

## B3. `decode_film`

```rust
pub fn decode_film(container: &Container, index: usize) -> Result<Film>
```

Step by step:

1. `container.kind != Films` ⇒ `bail!("{}: expected a Films container, found {:?}", …)`.
2. `index >= container.resource_count as usize` ⇒ `bail!("{}: film index {index} out of range (resource count {})", container.path.display(), container.resource_count)`.
3. `name = container.name(index).to_string()`; `meta = parse_film_metadata(container, index)?`; `data = container.payload(index)`.
4. `chunks = parse_chunks(data)` with context `format!("parsing film {index} ('{name}') chunk stream")`.
5. `chunk_kinds: BTreeMap<u16, usize>` — one count per chunk kind.
6. `full_chunks = chunks.iter().filter(|c| c.kind == full_frame_kind(container)).collect()`.
7. `first_full = full_chunks.first().copied()` or `anyhow!("film {index} ('{name}'): no full-frame chunk (type {:#06x}) in the stream", full_frame_kind(container))`.
8. `buffer = decode_full_frame(data, first_full, is_film_v2(container))` with context `format!("film {index} ('{name}'): decoding full frame")`.
9. `frame_size = meta.stride.checked_mul(meta.height as usize)` or `anyhow!("film {index}: frame size overflow")`.
10. `buffer.len() != frame_size` ⇒ `bail!("film {index} ('{name}'): full frame is {} bytes, expected {}x{} (stride {}) = {}", buffer.len(), meta.width, meta.height, meta.stride, frame_size)`.

**Event assembly** (before any frame is produced, so a bad order reports clearly).
Local `struct Event<'a> { tick: u32, timestamp: &'a Chunk, delta: Option<&'a Chunk>, full: Option<&'a Chunk> }`.
For every `chunk` in `chunks`, match `chunk.kind`:

* `TYPE_TIMESTAMP` (`0x06`): `payload = chunk_payload(data, chunk)?`; `payload.len() < 4` ⇒
  `bail!("film {index} ('{name}'): timestamp chunk at {:#x} is {} bytes", chunk.offset, payload.len())`.
  `tick = read_u32(payload, 0, true)` (**big-endian**). Push `Event { tick, timestamp: chunk, delta: None, full: None }`.
* `kind == full_frame_kind(container)`:
  * `events.last_mut()` is `Some(event)`: if `event.full.is_some() || event.delta.is_some()` ⇒
    `bail!("film {index} ('{name}'): key frame at {:#x} follows a frame that already has one", chunk.offset)`.
    Else `event.full = Some(chunk)`.
  * `None`: if `chunk.offset != first_full.offset` ⇒
    `bail!("film {index} ('{name}'): key frame at {:#x} precedes the first timestamp and is not the first key frame", chunk.offset)`.
    (This is the seeding key frame; nothing else is accepted here.)
* `TYPE_DELTA | TYPE_DELTA_V2` (`0x0B` or `0x01`): `event = events.last_mut()` or
  `anyhow!("film {index} ('{name}'): delta chunk at {:#x} has no preceding timestamp", chunk.offset)`.
  If `event.delta.is_some()` ⇒
  `bail!("film {index} ('{name}'): more than one delta chunk follows timestamp at {:#x}", event.timestamp.offset)`.
  Else `event.delta = Some(chunk)`.
* anything else (`TYPE_AUDIO`, `TYPE_MARKER`, `TYPE_END`, unknown): ignored.

Then, for each adjacent pair `pair` in `events.windows(2)`: if `pair[1].tick < pair[0].tick` ⇒
`bail!("film {index} ('{name}'): timestamps go backwards ({} then {})", pair[0].tick, pair[1].tick)`.

**Frames.** `frames = Vec::with_capacity(events.len() + 1)`.

Frame zero:

```rust
let frame0_duration = events.first().map(|e| e.tick).unwrap_or(0);
frames.push(Frame {
    start_tick: 0,
    duration_tick: frame0_duration,
    duplicate: false,
    indices: buffer.clone(),
    timestamp_chunk: None,
    delta_chunk: None,
    full_chunk: Some(full_chunks[0].offset),   // NOTE: full_chunks[0], i.e. the first key frame
    delta_rects: Vec::new(),
});
```

Then for `(i, event)` in `events.iter().enumerate()`:

```rust
let duration_tick = if i + 1 < events.len() {
    events[i + 1].tick - event.tick
} else if i > 0 {
    event.tick - events[i - 1].tick          // the last frame reuses the previous gap
} else {
    frame0_duration
};
```
(Monotonicity was already checked, so no underflow.)

Then `let mut delta_rects = Vec::new(); let mut wrote = true; let mut full_offset = None;` and:

* `event.full` is `Some(full)`:
  `decoded = decode_full_frame(data, full, is_film_v2(container))` with context
  `format!("film {index} ('{name}'): decoding full frame at {:#x}", full.offset)`;
  `decoded.len() != frame_size` ⇒
  `bail!("film {index} ('{name}'): full frame at {:#x} is {} bytes, expected {}x{} (stride {}) = {}", full.offset, decoded.len(), meta.width, meta.height, meta.stride, frame_size)`;
  `buffer = decoded; full_offset = Some(full.offset);`
* else `event.delta` is `Some(delta)`:
  `match decode_delta(data, delta, &mut buffer, is_film_v2(container))`:
  * `Ok(rects)` ⇒ `delta_rects = rects`
  * `Err(e)` ⇒ **when `!is_film_v2(container)`**: return the error with context
    `format!("film {index} ('{name}'): decoding delta chunk at {:#x}", delta.offset)`.
    **When the container is version 2**: `eprintln!("warning: {}: film {index} ('{name}'): delta chunk at {:#x} not decoded ({e}); reusing the previous frame", container.path.display(), delta.offset)`
    and `wrote = false` — the previous buffer is kept.
* else: nothing is sent; `wrote` stays `true` and the buffer is the previous frame's.

Push:

```rust
Frame {
    start_tick: event.tick,
    duration_tick,
    duplicate: (event.delta.is_none() && event.full.is_none()) || !wrote,
    indices: buffer.clone(),
    timestamp_chunk: Some(event.timestamp.offset),
    delta_chunk: event.delta.map(|c| c.offset),
    full_chunk: full_offset,
    delta_rects,
}
```

**Unique frames.** A `HashSet<&[u8]>` of `frame.indices.as_slice()`;
`unique_frames` = number of successful inserts (first occurrence of each distinct
pixel buffer).

**Audio.** For every `chunk` with `chunk.kind == TYPE_AUDIO`:
`payload = chunk_payload(data, chunk)?`; `payload.len() < 12` ⇒
`bail!("film {index} ('{name}'): audio chunk at {:#x} is {} bytes", chunk.offset, payload.len())`;
`data_len = read_u32(payload, 0, true)` (**BE**); `block_index = read_u32(payload, 8, true)` (**BE**);
`12usize.saturating_add(data_len as usize) > payload.len()` ⇒
`bail!("film {index} ('{name}'): audio chunk at {:#x} declares {} data bytes but holds {}", chunk.offset, data_len, payload.len())`.
Push `AudioChunk { chunk_offset: chunk.offset, chunk_len: chunk.len, data_len, index: block_index, data_offset: chunk.offset + 8 + 12, sync: block_index == u32::MAX }`.

Note `data_offset = chunk.offset + 20` — the chunk header is 8 bytes and the
audio block header 12, both **inside** the chunk payload.

Finally `Ok(Film { index, name, width, height, stride, bounds, record, palette, palette_count, frames, chunks, chunk_kinds, audio, unique_frames })`.

## B4. `parse_film_metadata` (private) — the metadata region

```rust
fn parse_film_metadata(container: &Container, index: usize) -> Result<FilmMeta>
```

1. `n = container.resource_count as usize`; `index >= n` ⇒ `bail!("film index {index} out of range")`.
2. `container.version == 1` ⇒ `bail!("{}: version 1 has no measured Films layout (no Film container ships with the first game; see docs/GAME1.md §1)", container.path.display())`.
3. `block = container.names_end.checked_add(4 * n)` or `anyhow!("film metadata offset overflow")`.
4. `marker = read_u16_checked(data, block)` with context `format!("{}: film metadata marker at {:#x}", container.path.display(), block)`.
   `marker != 7 && marker != 5` ⇒
   `bail!("{}: film metadata marker at {:#x} is {marker}, expected 7 or 5", container.path.display(), block)`.
5. **The length solve.** `payload_start = container.offsets[0] as usize`. For
   `table in (16..=MAX_RECORD_LEN).step_by(2)` — i.e. `16, 18, 20, 22, 24, 26`:

   ```rust
   let count_at = block.checked_add(2).and_then(|v| v.checked_add(table * n))
       .ok_or_else(|| anyhow::anyhow!("film metadata table overflow"))?;
   let palette_count = match read_u16_checked(data, count_at) { Ok(v) => v as usize, Err(_) => continue };
   if palette_count == 0 || palette_count > 8 { continue; }
   let palettes_end = match count_at.checked_add(2).and_then(|v| v.checked_add(1024 * palette_count)) {
       Some(v) => v, None => continue,
   };
   if palettes_end == payload_start { found = Some((table, palette_count)); break; }
   ```
   `found` is `None` ⇒
   `anyhow!("{}: no film table/palette count ends exactly at the payload start {:#x}", container.path.display(), payload_start)`.
6. `tables_start = block + 2`; `palette_count_at = tables_start + table * n`; `palettes_start = palette_count_at + 2`.
   `palettes_start + 1024 * palette_count > data.len()` ⇒
   `bail!("{}: film metadata ends at {:#x}, past the {:#x}-byte file", container.path.display(), palettes_start + 1024 * palette_count, data.len())`.
7. **The rects.** `rect_at = tables_start.checked_add(2 * n).and_then(|v| v.checked_add(8 * index))`
   or `anyhow!("film rect offset overflow")`.
   `rect_at + 8 > palettes_start` ⇒
   `bail!("{}: film {index}'s rect at {rect_at:#x} is outside the metadata region", container.path.display())`.
   Read **four big-endian u16**:

   ```rust
   let bounds = [
       u16::from_be_bytes([data[rect_at],     data[rect_at + 1]]),   // top
       u16::from_be_bytes([data[rect_at + 2], data[rect_at + 3]]),   // left
       u16::from_be_bytes([data[rect_at + 4], data[rect_at + 5]]),   // bottom
       u16::from_be_bytes([data[rect_at + 6], data[rect_at + 7]]),   // right
   ];
   ```
   `right <= left || bottom <= top` ⇒
   `bail!("film {index}: bad bounds top={top} left={left} bottom={bottom} right={right}")`.
   `width = right - left`; `height = bottom - top`.
   `stride = if container.version == 2 { width as usize } else { picture::stride8(width) }`.
8. **The reassembled record** (only when `r8 + 2 <= palettes_start`, else `record` stays empty):

   ```rust
   let mut record = Vec::with_capacity(table);
   let (b1, b2) = (tables_start + index, tables_start + n + index);
   let (r4, r5, r6, r7, r8) = (
       tables_start + 2 * n + 8 * n + 4 * index,
       tables_start + 6 * n + 8 * n + 4 * index,
       tables_start + 10 * n + 8 * n + 2 * index,
       tables_start + 12 * n + 8 * n + 4 * index,
       tables_start + 16 * n + 8 * n + 2 * index,
   );
   if r8 + 2 <= palettes_start {
       record.push(data[b1]);                                   // table 1, 1 byte
       record.push(data[b2]);                                   // table 2, 1 byte
       record.extend_from_slice(&data[rect_at..rect_at + 8]);    // the rect, 8 bytes BE
       record.extend_from_slice(&data[r4..r4 + 4]);              // table 4, BE u32
       record.extend_from_slice(&data[r5..r5 + 4]);              // table 5, 4 raw bytes
       record.extend_from_slice(&data[r6..r6 + 2]);              // table 6, BE u16
       record.extend_from_slice(&data[r7..r7 + 4]);              // table 7, BE u32
       record.extend_from_slice(&data[r8..r8 + 2]);              // table 8, BE u16
   }
   ```
   That is 26 bytes and it is what `manifest.json`'s `record_hex` carries.
9. **The palette.** `palette_index = index.min(palette_count - 1)`;
   `palette_at = palettes_start + 1024 * palette_index`;
   `palette = data[palette_at .. palette_at + 1024].to_vec()`.
   (`palette_count >= 1` is guaranteed by step 5.)
10. `Ok(FilmMeta { record, bounds, width, height, stride, palette, palette_count: palette_count as u16 })`.

The full region layout, `docs/FILMS.md` §2 (and the identical table in the Rust
doc comment, with the disassembly of `MM2DFilmResourceFile` at `0x477940`):

| Offset (from `block`) | Size | Contents |
|---|---|---|
| `0` | `4*N` | generic u32 array (0 in the third game, Mac timestamps in the 3D game) |
| `4*N` | `2` | marker, must be 7 (third/3D game) or 5 (second game) |
| `4*N + 2` | `N` | per-film table 1, one byte each |
| `5*N + 2` | `N` | per-film table 2, one byte each |
| `6*N + 2` | `8*N` | **the rects**, four big-endian u16 per film |
| `14*N + 2` | `4*N` | table 4, big-endian u32 per film |
| `18*N + 2` | `4*N` | table 5, four raw bytes per film |
| `22*N + 2` | `2*N` | table 6, big-endian u16 per film |
| `24*N + 2` | `4*N` | table 7, big-endian u32 per film |
| `28*N + 2` | `2*N` | table 8, big-endian u16 per film |
| `30*N + 2` | `2` | palette count `P` |
| next | `1024*P` | palettes |

`tables_start == block + 2 == 4*N + 2`, so `b1 = tables_start + index` is table 1,
`b2 = tables_start + n + index` is table 2, `rect_at = tables_start + 2n + 8*index`
is the third (rect) table, and `r4..r8` are tables 4..8. The payload must start
exactly at the end of the last palette — that is the identity the solve in step 5
exploits, and the doc evidence for it is: "The payload must start exactly at the
end of the last palette."

Measured confirmation (`docs/FILMS.md` §2): `stride8(right - left) * (bottom - top)`
equals the byte count the film's own key-frame chunk declares it decodes to —
800*600 = 480000, 592*426 = 252192, 640*480 = 307200, 548*410 = 224680.

## B5. `parse_chunks`

```rust
fn parse_chunks(data: &[u8]) -> Result<Vec<Chunk>>
```

```rust
let mut offset = 0usize;
while offset + 8 <= data.len() {
    let tag = read_u16(data, offset, true);
    if tag != CHUNK_TAG { bail!("bad chunk tag {tag:#06x} at {offset:#x}, expected {CHUNK_TAG:#06x}"); }
    let kind = read_u16(data, offset + 2, true);
    let len = read_u32(data, offset + 4, true) as usize;
    let end = offset.checked_add(8).and_then(|v| v.checked_add(len))
        .ok_or_else(|| anyhow::anyhow!("chunk at {offset:#x} length overflow"))?;
    if end > data.len() { bail!("chunk type {kind:#06x} at {offset:#x} length {len} runs past the payload end"); }
    chunks.push(Chunk { offset, kind, len });
    offset = end;
}
if offset != data.len() {
    bail!("{} trailing bytes at {offset:#x} after the last chunk", data.len() - offset);
}
```

All three reads are **big-endian**. `{:#06x}` on `0xF789` prints `0xf789`.
Note the loop condition: a trailing run of fewer than 8 bytes falls out of the
loop and then trips the "trailing bytes" bail.

## B6. `chunk_payload`

```rust
fn chunk_payload<'a>(data: &'a [u8], chunk: &Chunk) -> Result<&'a [u8]>
```

`start = chunk.offset.checked_add(8)` or `anyhow!("chunk offset overflow")`;
`end = start.checked_add(chunk.len)` or `anyhow!("chunk length overflow")`;
`data.get(start..end)` or `anyhow!("chunk at {:#x} is outside the payload", chunk.offset)`.
So the payload is `data[chunk.offset + 8 .. chunk.offset + 8 + chunk.len]`.

## B7. `decode_full_frame`

```rust
fn decode_full_frame(data: &[u8], chunk: &Chunk, v2: bool) -> Result<Vec<u8>>
```

1. `payload = chunk_payload(data, chunk)?`.
2. `v2` (version 2): `payload.len() != 800 * 600` ⇒
   `bail!("full-frame chunk at {:#x} is {} bytes, expected {} for an 800x600 frame", chunk.offset, payload.len(), 800 * 600)`;
   else `return Ok(payload.to_vec())` — raw indices, no header, no LZ77.
3. `payload.len() < 8` ⇒ `bail!("full-frame chunk at {:#x} is too short", chunk.offset)`.
4. `lz77_len = read_u32(payload, 0, true) as usize`; `decoded_len = read_u32(payload, 4, true) as usize` (both **BE**).
5. `lz77_len > payload.len() - 8` ⇒
   `bail!("full-frame chunk at {:#x} declares {lz77_len} compressed bytes but only {} follow the header", chunk.offset, payload.len() - 8)`.
6. `decoded = lz77::decompress(&payload[8..8 + lz77_len])` with context
   `format!("full-frame chunk at {:#x}: LZ77 stream ({} bytes)", chunk.offset, lz77_len)`.
7. `decoded.len() != decoded_len` ⇒
   `bail!("full-frame chunk at {:#x}: decoded {} bytes, header says {}", chunk.offset, decoded.len(), decoded_len)`.
8. `Ok(decoded)`.

Doc evidence, `docs/FILMS.md` §4: "u32 A length of the LZ77 stream / u32 B decoded
length / A bytes LZ77 stream / 0..3 bytes padding".

## B8. `decode_delta`

```rust
fn decode_delta(data: &[u8], chunk: &Chunk, frame: &mut [u8], v2: bool) -> Result<Vec<[u16; 4]>>
```

1. `payload.len() < 168` ⇒ `bail!("delta chunk at {:#x} is too short", chunk.offset)`.
2. `(rect_off, delta)`:
   * `v2`: `(0usize, payload[168..].to_vec())` — rects then the RLE, no LZ77.
   * else: `lz77_len = read_u32(payload, 0, true)`; `decoded_len = read_u32(payload, 4, true)` (**BE**);
     `lz77_len > payload.len() - 168` ⇒
     `bail!("delta chunk at {:#x} declares {lz77_len} compressed bytes but only {} follow the rect table", chunk.offset, payload.len() - 168)`;
     `delta = lz77::decompress(&payload[168..168 + lz77_len])` with context
     `format!("delta chunk at {:#x}: LZ77 stream ({} bytes)", chunk.offset, lz77_len)`;
     `delta.len() != decoded_len` ⇒
     `bail!("delta chunk at {:#x}: decoded {} bytes, header says {}", chunk.offset, delta.len(), decoded_len)`;
     `(8, delta)`.
3. **The 20 rects**, each four **big-endian** u16, `i in 0..20`, `p = rect_off + i * 8`:

   ```rust
   let top    = read_u16(payload, p,     true);
   let left   = read_u16(payload, p + 2, true);
   let bottom = read_u16(payload, p + 4, true);
   let right  = read_u16(payload, p + 6, true);
   rects.push([left, top, right, bottom]);   // stored in the conventional order
   ```
4. `apply_delta(frame, &delta)` with context
   `format!("delta chunk at {:#x}: cursor-relative RLE ({} bytes)", chunk.offset, delta.len())`.
5. `Ok(rects)`.

## B9. `apply_delta` — the RLE codec

```rust
fn apply_delta(frame: &mut [u8], delta: &[u8]) -> Result<()>
```

```rust
let mut pos = 0usize;
let mut cursor: i64 = 0;
while pos + 4 <= delta.len() {
    let displacement = i16::from_be_bytes([delta[pos],     delta[pos + 1]]) as i64;
    let length       = i16::from_be_bytes([delta[pos + 2], delta[pos + 3]]) as i64;
    pos += 4;
    cursor += displacement;
    if length == 0 { continue; }                        // cursor move only
    if length < 0 {
        let count = (-length) as usize;
        if pos + 4 > delta.len() { bail!("pattern record at {pos:#x} has no four-byte pattern"); }
        let pattern = [delta[pos + 3], delta[pos + 2], delta[pos + 1], delta[pos]];   // byte-reversed
        pos += 4;
        let start = checked_span(cursor, count, frame.len())?;
        for (i, byte) in frame[start..start + count].iter_mut().enumerate() { *byte = pattern[i & 3]; }
        cursor += count as i64;
    } else {
        let count = length as usize;
        if pos + count > delta.len() { bail!("literal record at {pos:#x} runs past the delta data"); }
        let start = checked_span(cursor, count, frame.len())?;
        frame[start..start + count].copy_from_slice(&delta[pos..pos + count]);
        pos += count;
        cursor += count as i64;
    }
}
if pos != delta.len() { bail!("{} trailing delta bytes at {pos:#x}", delta.len() - pos); }
Ok(())
```

Key points: both record words are **big-endian i16**; the pattern is byte-reversed
(`FUN_004c396b` in the disassembly reverses the 32-bit word); the literal branch
is taken for `length > 0`; `length == 0` writes nothing and only moves the cursor;
`cursor` is a signed `i64` so a negative displacement is representable and is
rejected by `checked_span` with `"write at negative frame offset {cursor}"`.

## B10. `checked_span`

```rust
fn checked_span(cursor: i64, count: usize, frame_len: usize) -> Result<usize>
```

* `cursor < 0` ⇒ `bail!("write at negative frame offset {cursor}")`.
* `start = cursor as usize`; `end = start.checked_add(count)` or `anyhow!("frame write range overflow")`.
* `end > frame_len` ⇒ `bail!("write {start}..{end} is past the {frame_len}-byte frame")`.
* `Ok(start)`.

## B11. `write_film_modern` and the manifest

```rust
fn write_film_modern(container: &Container, film: &Film, dir: &Path) -> Result<()>
```

For `(i, frame)` in `film.frames.iter().enumerate()`:
`rgba = picture::indexed_rgba_stride(&film.palette, &frame.indices, film.width, film.height, film.stride)`;
`path = dir.join(format!("frame_{i:03}.png"))`;
`picture::write_png(&path, film.width, film.height, &rgba)` with context
`format!("writing {}", path.display())`.

Then `path = dir.join("manifest.json")`, `manifest = manifest_json(container, film)`,
`std::fs::write(&path, manifest)` with context `format!("writing {}", path.display())`.

**No per-frame error tolerance**: any failure is `Err`.

Directory template: `{index:05}_{strip_flm(sanitize(film.name, index))}`;
frame files `frame_{i:03}.png` zero-padded to 3.

## B12. `manifest_json` — exact key order and exact bytes

```rust
fn manifest_json(container: &Container, film: &Film) -> String
```

Built with `String::push_str`/`format!` (not a JSON library), LF line endings,
and the following **exact key order**:

```json
{
  "container": "<json_escape(container.path.display())>",
  "film_index": <film.index>,
  "name": "<json_escape(film.name)>",
  "width": <film.width>,
  "height": <film.height>,
  "stride": <film.stride>,
  "frame_size_bytes": <film.stride * film.height as usize>,
  "pixel_format": "8bpp palette indices; RGBA = palette[transparency, red, green, blue], alpha = 255 - transparency",
  "record_hex": "<hex(film.record)>",
  "record_bounds": [<bounds[0]>, <bounds[1]>, <bounds[2]>, <bounds[3]>],
  "record_note": "bytes +2..+10 are u16 BE {top, left, bottom, right}; the rest is preserved verbatim",
  "palette_count": <film.palette_count>,
  "palette_hex": "<hex(film.palette)>",
  "frame_count": <film.frames.len()>,
  "unique_frame_count": <film.unique_frames>,
  "timebase": "ticks treated as milliseconds (83-tick spacing = 12.05 fps)",
  "duration_ms": <film.duration_ticks()>,
  "chunk_count": <film.chunks.len()>,
  "chunk_types": {"<kind>": <count>, ...},
  "offset_note": "all chunk offsets are relative to the start of the inner .flm payload",
  "frames": [
    {
      "index": <i>,
      "start_ms": <frame.start_tick>,
      "duration_ms": <frame.duration_tick>,
      "duplicate": <true|false>,
      "png": "frame_<i:03>.png",
      "source": <source>,
      "delta_rects": [[l, t, r, b], ...]
    },
    ...
  ],
  "audio": {
    "chunk_count": <film.audio.len()>,
    "data_blocks": <non-sync count>,
    "sync_blocks": <sync count>,
    "note": "12-byte header [data_len, zero, block_index], then PCM bytes; sync blocks use block_index 4294967295. Audio is preserved verbatim by --original.",
    "chunks": [
      {"chunk_offset": <n>, "chunk_len": <n>, "data_offset": <n>, "data_len": <n>, "index": <n>, "sync": <bool>},
      ...
    ]
  }
}
```

Every line above is emitted literally as written; the two long `note` /
`pixel_format` / `record_note` / `timebase` strings are constants. `chunk_types`
is built as `format!("\"{kind}\": {count}")` joined with `", "` and wrapped in
`{}` — one line, keys in `BTreeMap` (numeric) order.

`source`, in this exact match order on `(frame.full_chunk, frame.delta_chunk, frame.timestamp_chunk)`:

| pattern | emitted |
|---|---|
| `(Some(offset), _, _)` | `{"kind": "full", "chunk_offset": <offset>}` |
| `(_, Some(delta), Some(timestamp))` | `{"kind": "delta", "timestamp_chunk_offset": <timestamp>, "delta_chunk_offset": <delta>}` |
| `(_, None, Some(timestamp))` | `{"kind": "duplicate", "timestamp_chunk_offset": <timestamp>}` |
| anything else | `{"kind": "unknown"}` |

Note the object keys inside these one-line objects are in exactly that order, and
there is a space after each `:` and after each `,`.

Frame objects: 6-space `{`, 8-space keys, closing `    }` and a comma on every
frame object except the last (`if i + 1 == film.frames.len() { "    }\n" } else { "    },\n" }`).
`delta_rects` is `[l,t,r,b]` joined with `", "` (no spaces inside the brackets),
one line, empty ⇒ `[]`.

Audio chunk objects are one line each, 6-space indent, comma after every one
except the last.

### B12.1 `hex` and `json_escape` (film.rs)

```rust
fn hex(bytes: &[u8]) -> String      // lowercase "{byte:02x}", no separators
fn json_escape(text: &str) -> String // §2.3, with \b and \f added
```

## B13. `original_filename` / `strip_flm`

```rust
fn original_filename(index: usize, safe: &str) -> String {
    if safe.to_ascii_lowercase().ends_with(".flm") { format!("{index:05}_{safe}") }
    else { format!("{index:05}_{safe}.flm") }
}
fn strip_flm(safe: &str) -> String {
    if safe.to_ascii_lowercase().ends_with(".flm") {
        let stem = &safe[..safe.len() - 4];
        if !stem.is_empty() { return stem.to_string(); }
    }
    safe.to_string()
}
```

`to_ascii_lowercase()` is ASCII-only case folding; `strip_flm` leaves an
all-dot-suffix name (e.g. sanitize gave `.flm`) unchanged, because the stem is
empty. Consequence: `--original` on `Intro.flm` writes `00000_Intro.flm`, and the
modern directory is `00000_Intro`.

## B14. `read_u16_checked`

```rust
fn read_u16_checked(data: &[u8], offset: usize) -> Result<u16> {
    if offset + 2 > data.len() { bail!("read past end of file at {offset:#x}"); }
    Ok(read_u16(data, offset, true))     // always big-endian
}
```

## B15. No subprocess, no external tool

`film.rs` spawns nothing. The only external programs in the whole project are in
`src/sound.rs` (the `flac` tool) — specified in `PORT-SOUND.md` §11 — and they are
out of scope here.

---

# PART C — `src/dma3d.rs`

## C1. The two metadata-region lengths

```rust
pub fn models_meta_len(n: usize) -> usize { 4 * n + MODELS_META_TRAILER }   // MODELS_META_TRAILER = 100
pub fn textures_meta_len(n: usize) -> usize { 6 * n + 2 }
```

These are **public and asserted by `tests/dma3d.rs`** (`the_metadata_regions_are_measured`):
`offsets[0] - (names_end + 32*N)` must equal `4*460 + 100 = 1940` for `DMA3D.MMM`
and `6*705 + 2 = 4232` for `DMA3D.MMX`. Note `docs/DMA3D.md` §2.1 now corrects the
Textures region to `32*N + 4*N + 2*N + 2` = 26,792 bytes, **but the function is
still `6*N + 2`** — see §16. The port must keep `6*N + 2`, because the test
computes `region = first - (names_end + 32*n)` and compares against this function.

(No other code in the ported modules ever calls these two functions; they exist
for the test and for documentation.)

## C2. `extract_models` / `extract_textures`

```rust
pub fn extract_models(container: &Container, out_dir: &Path, original: bool,
                      texture_dir: Option<&Path>) -> Result<Extract3DReport>
pub fn extract_textures(container: &Container, out_dir: &Path, original: bool) -> Result<Extract3DReport>
```

* `models`: `original` ⇒ `extract(container, out_dir, "mod")`; else `extract_gltf(container, out_dir, texture_dir)`.
* `textures`: `original` ⇒ `extract(container, out_dir, "tex")`; else `extract_pngs(container, out_dir)`.

**Neither validates `container.kind`** — the caller is expected to have selected
the container by tag. The port must not add a check.

## C3. `extract` (the `--original` path)

```rust
fn extract(container: &Container, out_dir: &Path, extension: &str) -> Result<Extract3DReport>
```

1. `create_dir_all(out_dir)?` — **no context added**.
2. For `index in 0..container.resource_count as usize`:
   * `name = container.name(index)`; `payload = container.payload(index)`;
     `stem = output_stem(container, index)`; `path = out_dir.join(format!("{index:05}_{stem}.{extension}"))`.
   * `std::fs::write(&path, payload)` with context `format!("writing {}", path.display())`.
   * `Err(e)` ⇒ `eprintln!("warning: {}: 3D resource {index} '{name}': {error:#}", container.path.display())`
     (`{:#}` = the whole chain) and `report.failed.push((index, name.to_string(), error.to_string()))`; `continue`.
   * `Ok` ⇒ `report.written += 1` and `report.resources.push(Extracted3D { index, name: name.to_string(), path, bytes: payload.len() })`.
3. `Ok(report)`.

`bytes` for `--original` is the **payload length**.

## C4. `extract_pngs` (the modern texture path)

```rust
fn extract_pngs(container: &Container, out_dir: &Path) -> Result<Extract3DReport>
```

1. `create_dir_all(out_dir)?`, `report = Extract3DReport::default()`.
2. For each `index`: `path = out_dir.join(format!("{index:05}_{stem}.png"))`, then a
   chain that turns the outcome into `Result<usize, String>`:

   ```rust
   texfile::parse(payload).map_err(|e| format!("{e:#}"))
       .and_then(|texture| {
           let rgba = texture.to_rgba();
           let png = gltf::encode_png(&rgba, texture.width, texture.height).map_err(|e| format!("{e:#}"))?;
           Ok((png, texture.width * texture.height * 4))
       })
       .and_then(|(png, _)| std::fs::write(&path, &png).map(|_| png.len()).map_err(|e| format!("{e:#}")))
   ```
   Note the middle `Ok((png, texture.width * texture.height * 4))` computes a
   value that is **discarded** by the next closure; the reported `bytes` is
   `png.len()`.
3. `Ok(bytes)` ⇒ `written += 1`, `resources.push(Extracted3D { index, name, path, bytes })`.
4. `Err(why)` ⇒ `eprintln!("warning: {}: texture {index} '{name}': {why}", container.path.display())`
   and `report.failed.push((index, name.to_string(), why))`. Note the message is
   the **outermost** error only (`{e:#}` inside the closures produces a chain that
   is then turned into a `String`, so that full chain is what `why` holds — it is
   `format!("{e:#}")` of the original error).

Every shipped texture must decode: `docs/DMA3D.md` §5 "every texture becomes a
PNG — 705 of 705, no failures", pinned by `tests/dma3d.rs`.

## C5. `extract_gltf` (the modern model path)

```rust
fn extract_gltf(container: &Container, out_dir: &Path, texture_dir: Option<&Path>) -> Result<Extract3DReport>
```

1. `create_dir_all(out_dir)?`, `report = default()`.
2. For each `index`:
   * `name = container.name(index)`; `payload = container.payload(index)`; `stem = output_stem(container, index)`.
   * `mesh = match modfile::parse_mesh(payload)`:
     * `Err(_)` — **the error is discarded** — write `out_dir/{index:05}_{stem}.mod`
       with the payload, context `format!("writing {}", path.display())`;
       `written += 1`; `resources.push(Extracted3D { index, name, path, bytes: payload.len() })`; `continue`.
     * `Ok(m)` — continue.
   * **Materials.** `for wanted in texture_names_in(payload)`:
     `stem = sanitize(wanted.trim_end_matches(".TGA"), 0)`;
     `path = texture_dir.map(|dir| dir.join(format!("{stem}.png")))`;
     if the path exists and `std::fs::read(&path)` succeeds ⇒
     `materials.push(gltf::Material { name: wanted.clone(), png })`. A texture whose
     PNG is absent is **silently skipped**. `sanitize(..., 0)` uses index 0, so the
     `res00000` fallback only triggers for an empty stem.
   * `records = anim::locate(payload, &mesh)`; `texcoords = texcoords_of(payload, &records)`.
   * `path = out_dir.join(format!("{index:05}_{stem}.gltf")`)`;
     `gltf::write_gltf(&path, name, &mesh, &materials, texcoords.as_deref())` with context
     `format!("writing {}", path.display())`.
   * **Tail.** `suffix = format!("{index:05}_{stem}")`;
     `gltf::write_tail(&out_dir.join(gltf::tail_name(&format!("{suffix}.gltf"))), &payload[mesh.tail_offset()..])`
     with context `format!("writing the tail of {name}")`.
     (`tail_name` is `format!("{stem}.tail")`, so the file is
     `{index:05}_{stem}.gltf.tail`.)
   * **Records.** `document = anim::dump(name, payload, &mesh, &records)`;
     `records_path = out_dir.join(anim::records_name(&format!("{suffix}.gltf")))`;
     `std::fs::write(&records_path, document)` with context `format!("writing the records of {name}")`.
     (`records_name` is `format!("{stem}.anim.txt")` → `{index:05}_{stem}.gltf.anim.txt`.)
   * **Preview.** `preview = render::render(&mesh)`;
     `preview_path = out_dir.join(format!("{index:05}_{stem}.preview.png"))`;
     `gltf::encode_png(&preview.rgba, render::PREVIEW_PIXELS, render::PREVIEW_PIXELS)`
     then `.and_then(|png| std::fs::write(&preview_path, png).map_err(|e| anyhow::anyhow!("{e}")).map_err(Into::into))`
     with context `format!("writing the preview of {name}")`.
   * If `preview.covered == 0`: `eprintln!("warning: {}: model {index} '{name}': the preview covers no pixels", container.path.display())`
     **and** `report.failed.push((index, name.to_string(), "preview covers no pixels".into()))`
     — but the resource still counts as `written` (it was written successfully).
   * `bytes = std::fs::metadata(&path).map(|m| m.len() as usize).unwrap_or(0)` — the
     **glTF file size read back from disk**, not the string length.
     `written += 1`; `resources.push(Extracted3D { index, name, path, bytes })`.
3. `Ok(report)`.

## C6. `texcoords_of`

```rust
fn texcoords_of(payload: &[u8], records: &anim::Records) -> Option<Vec<[f32; 2]>>
```

* `records.pairs_in_unit_range != Some(records.pair_count)` ⇒ `None`
  (so **every** pair must be inside the range test).
* `v = records.pair_count`; for `k in 0..v`:
  `at = records.tail_offset + 12 + 8 * k`; `at + 8 > payload.len()` ⇒ `None`;
  read two **little-endian** f32s (`payload[at..at+4]`, `payload[at+4..at+8]`) and
  push `[a, b]`.
* `Some(out)`.

Note `texcoords_of` is called **after** `anim::locate`, and it reads the same
offsets `locate` used, so the two agree by construction.

## C7. `texture_names_in`

```rust
fn texture_names_in(payload: &[u8]) -> Vec<String>
```

A **sliding-window scan over the whole payload**:

```rust
let mut at = 0usize;
while at + 4 <= payload.len() {
    let count = u32::from_le_bytes([payload[at], payload[at+1], payload[at+2], payload[at+3]]) as usize;
    if !(1..=64).contains(&count) { at += 4; continue; }
    let table = at + 4;
    if table + count * 32 > payload.len() { at += 4; continue; }
    let mut ok = true;
    let mut found = Vec::new();
    for record in 0..count {
        let rec = &payload[table + record * 32 .. table + (record + 1) * 32];
        let end = rec.iter().position(|&b| b == 0).unwrap_or(rec.len());
        if end == 0 { ok = false; break; }
        let text = String::from_utf8_lossy(&rec[..end]).to_string();
        if !text.contains("TGA") { ok = false; break; }
        found.push(text);
    }
    if ok { names.extend(found); at = table + count * 32; } else { at += 4; }
}
```

So the window advances by **4 bytes** on rejection and past the whole table on
acceptance, and it restarts from the beginning for each payload. Every record
must be non-empty **and** contain the substring `"TGA"` (uppercase, literal).
Doc evidence, `src/dma3d.rs`: "The table is `[u32 count][count * 32-byte
NUL-padded names]` (measured on `createBtDraw.mod`, which holds 31 of them)."

## C8. `write_index` — `index.html`

```rust
pub fn write_index(base: &Path, texture_dir: &Path, model_dir: &Path) -> Result<usize>
```

1. `textures = list_dir(texture_dir, "png")`; `models = list_dir(model_dir, "gltf")`; `previews = list_dir(model_dir, "preview.png")`.
2. `relative_to(dir) = dir.strip_prefix(base).unwrap_or(dir).to_string_lossy().replace('\\', "/")`.
   So when both directories are under `base` (the normal case: `<base>/Textures`
   and `<base>/Models`) the prefixes are the **relative** names `Textures`,
   `Models`; a directory outside `base` is used verbatim.
3. Build the page (see C8.1) and `std::fs::write(base.join("index.html"), page)` with context `format!("writing {}", path.display())`.
4. Returns `files`, which is `models.len() + textures.len()` (the number of
   `<figure>` elements emitted).

### C8.1 The exact page text

`INDEX_HEAD` (a raw string literal, verbatim):

```html
<!DOCTYPE html>
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
```

then, in order:

```html
<h2>Objects <span class="count">{models.len()}</span></h2>
<p>Every object below is a rendered picture of the model itself, made from the model's own vertices. The file next to it is a <code>.gltf</code>, which is the model. <a href="#how">How to open one</a>.</p>
<div class="grid">
{one figure per model, in list_dir order}
</div>
<h2>Textures <span class="count">{textures.len()}</span></h2>
<p>The game stores each texture as a palette and a grid of one-byte indices. They are expanded to ordinary PNGs here.</p>
<div class="grid">
{one figure per texture, in list_dir order}
</div>
```

then `INDEX_TAIL` verbatim:

```html
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
```

The model figure, one per `(index, name)` from `list_dir(model_dir, "gltf")`
(`preview` is the file name of the matching `.preview.png`, or `""`):

```html
<figure><a href="{model_prefix}/{name}" title="Open {index:05}" ><img src="{model_prefix}/{preview}" alt="{index:05}" loading="lazy"></a><figcaption>{index:05}</figcaption></figure>
```

emitted with a trailing `\n` (the format string in the Rust ends with `</figure>\n`).
Note the **space before the `>`** of the `<a …>` tag and the absence of a space
before `</a>`.

The texture figure, one per `(index, name)` from `list_dir(texture_dir, "png")`:

```html
<figure><a href="{texture_prefix}/{name}" title="Open {index:05}" ><img src="{texture_prefix}/{name}" alt="{index:05}" loading="lazy"></a><figcaption>{index:05}</figcaption></figure>
```

(For a texture the link and the image are the same path.) The whole `format!`
string, quoted from the Rust, is:

```rust
page.push_str(&format!(
    "<figure>\
     <a href=\"{texture_prefix}/{name}\" title=\"Open {index:05}\" >\
     <img src=\"{texture_prefix}/{name}\" alt=\"{index:05}\" loading=\"lazy\">\
     </a><figcaption>{index:05}</figcaption></figure>\n"
));
```
The `\`-continued literal contributes no whitespace, so the emitted text has no
newlines inside a figure — exactly the single line shown above.

There are **no URLs other than the one in `INDEX_TAIL`**, no `<script`, and no
stylesheet or network fetch; `tests/dma3d.rs` asserts all three.

### C8.2 `list_dir`

```rust
fn list_dir(dir: &Path, extension: &str) -> Vec<(usize, String)>
```

* `std::fs::read_dir(dir)` fails ⇒ an empty `Vec` (no error).
* For each entry: `name = entry.file_name().to_string_lossy().to_string()`;
  keep only `name.ends_with(extension)`.
* `digits: String = name.chars().take_while(|c| c.is_ascii_digit()).collect()`;
  `index = digits.parse::<usize>().unwrap_or(usize::MAX)` (an unparsable or empty
  prefix, e.g. a name not starting with a digit, sorts last).
* `out.push((index, name))`, then `out.sort()` — ascending by `(index, name)`,
  so the game's resource order comes back out.

The dir entries are **not** recursively walked, and the extension match is a
suffix test: asking for `"png"` in the model directory also matches
`.preview.png`, which is why the previews are listed with `"preview.png"` and the
glTFs with `"gltf"` separately (a `.gltf.tail` does not end with `gltf`).

## C9. `output_stem`

```rust
fn output_stem(container: &Container, index: usize) -> String { sanitize(container.name(index), index) }
```

---

# PART D — `src/dma3d/texfile.rs`

## D1. `parse`

```rust
pub fn parse(payload: &[u8]) -> Result<Texture>
```

1. `k = palette_size(payload).context("no palette/index split fits this payload")?`
   — the context is prepended to whatever `palette_size` raised.
2. `palette_bytes = k * PALETTE_ENTRY_BYTES`; build `palette` as `k` entries of
   four bytes read at `entry * 4 + 0..4`.
3. `indices = payload[palette_bytes..].to_vec()`; `area = indices.len()`.
4. `if !area.is_power_of_two() { bail!("{area} index bytes is not a power of two, so no width and height fit") }`
   — this check appears **twice in the Rust** (a copy-paste); one test is enough.
5. `(width, height, axis) = split_area(indices.as_slice(), area)`.
6. `if width * height != area { bail!("{area} index bytes do not factor into {width}x{height}") }`
7. `undecidable = area.trailing_zeros() % 2 == 1 && { let (w, h) = wide_tall(area); !decide(indices.as_slice(), w, h).1 }`
   — only an odd exponent (a 1:2 area) can be undecidable, and only when
   `decide`'s second return is `false` (i.e. its tie branch).
8. `Ok(Texture { palette, indices, width, height, axis, undecidable })`.

Note `parse` calls `decide` a second time here, after `split_area` already called
it; the two calls are on identical inputs, so the result is the same. The port may
compute it once — but only if it proves `decide` is pure (it is).

## D2. `palette_size`

```rust
pub fn palette_size(payload: &[u8]) -> Result<usize>
```

```rust
for k in 1..=payload.len() / PALETTE_ENTRY_BYTES {
    let start = k * PALETTE_ENTRY_BYTES;
    if start >= payload.len() { bail!("payload is all palette: {k} entries with no index bytes"); }
    let largest = payload[start..].iter().copied().max().unwrap_or(0);
    if usize::from(largest) < k { return Ok(k); }
}
bail!("no palette size makes the trailing bytes valid indices")
```

So `K` is the **smallest** `k` with `max(payload[4k..]) < k`, i.e. `1 + the largest
index`. A payload whose length is not a multiple of four still terminates the loop
properly because `k <= len/4` bounds `start <= len - 4`.

Doc evidence, `docs/DMA3D.md` §4.1: "`K` is the **smallest** value for which every
byte after `4*K` is a valid index — that is, `1 + the largest index the pixel data
uses`", confirmed by "`payload_len - 4*K` lands on a power of two in **all 705**
files" and "**no palette entry is dead.**".

## D3. `split_area`, `wide_tall`

```rust
fn split_area(indices: &[u8], area: usize) -> (usize, usize, Axis)
fn wide_tall(area: usize) -> (usize, usize)
```

```rust
let root = (area as f64).sqrt().round() as usize;
if root * root == area && WIDTH_CANDIDATES.contains(&root) { return (root, root, Axis::Square); }
let (w, h) = wide_tall(area);
if decide(indices, w, h).0 { (w, h, Axis::Wide) } else { (h, w, Axis::Tall) }
```

```rust
debug_assert!(area.is_power_of_two());
let bits = area.trailing_zeros() as usize;
(1usize << (bits / 2 + 1), 1usize << (bits / 2))
```

So for `area = 2^(2m+1)` the pair is `(2^(m+1), 2^m)` — always exactly 1:2, wide
first. For `area = 2^(2m)` (a perfect square) the pair is `(2^(m+1), 2^m)` only
when the root is not in `WIDTH_CANDIDATES`; that cannot happen for the squares
this engine produces (8..256 are all candidates), but the code path exists and
the port must keep it.

`roots` the port must reproduce exactly: `WIDTH_CANDIDATES = [8, 16, 32, 64, 128,
256]`. A square whose root is outside that set (e.g. area 4 → root 2, or area
2^18 → root 512) falls through to the 1:2 branch.

## D4. `decide`

```rust
fn decide(indices: &[u8], wide: usize, tall: usize) -> (bool, bool)
```

Returns `(true when the wide reading is right, true when the data could tell
them apart)`.

```rust
let wide_shape = shape(indices, wide, tall);
let tall_shape = shape(indices, tall, wide);
let wide_delta = vertical_delta(indices, wide, tall);
let tall_delta = vertical_delta(indices, tall, wide);

if wide_shape.components == 1 && tall_shape.components != 1 { return (true, true); }
if tall_shape.components == 1 && wide_shape.components != 1 { return (false, true); }
if wide_shape.cut == 0 && tall_shape.cut != 0 { return (true, true); }
if tall_shape.cut == 0 && wide_shape.cut != 0 { return (false, true); }

let order = wide_delta.partial_cmp(&tall_delta).unwrap_or(std::cmp::Ordering::Equal);
match order {
    std::cmp::Ordering::Less => (true, true),
    std::cmp::Ordering::Greater => (false, true),
    std::cmp::Ordering::Equal => (false, false),
}
```

Order of the vetoes matters: components before cut, and each veto fires only when
it is **unique** to one reading. `partial_cmp` on `f64` returns `None` only for
NaN, which is treated as `Equal` here.

Doc evidence, `src/dma3d/texfile.rs`: "The rule is **the mean index difference
across a vertical step, with two vetoes on top of it**… 1. **one connected
component.** 2. **zero pixels cut on the frame border.**", corroborated by
`docs/DMA3D.md` §4.2 and pinned by `tests/dma3d_numbers.rs::the_measured_orientations_are_reproduced`
(21 named textures with expected width/height).

## D5. `vertical_delta`

```rust
fn vertical_delta(indices: &[u8], w: usize, h: usize) -> f64
```

`h < 2` ⇒ `0.0`. Otherwise:

```rust
let mut total = 0u64; let mut pairs = 0usize;
for row in 0..h - 1 {
    let base = row * w;
    for col in 0..w {
        total += u64::from(indices[base + col].abs_diff(indices[base + w + col]));
        pairs += 1;
    }
}
total as f64 / pairs as f64
```

`abs_diff` is the unsigned absolute difference. The division is `f64`; the port
must carry the same accumulation in integers (or prove the float rounding is
irrelevant — it is, for these magnitudes, but integer accumulation is safest).

## D6. `background` and `shape`

```rust
fn background(indices: &[u8], palette_len: usize) -> u8
```

`palette_len == 1` ⇒ `0`. Otherwise `counts = vec![0usize; palette_len]`, count
every index that fits (`counts.get_mut` skips out-of-range), and return the index
of the **strictly largest** count — `if *count > counts[usize::from(best)]`, so
**the first maximum wins** (ties keep the lower index). `best` starts at 0.

```rust
fn shape(indices: &[u8], w: usize, h: usize) -> Shape
```

```rust
let palette_len = indices.iter().copied().max().map(|m| usize::from(m) + 1).unwrap_or(1);
let bg = usize::from(background(indices, palette_len));
let mut seen = vec![false; w * h];
let mut components = 0usize; let mut cut = 0usize;
let mut stack = Vec::new();
for row in 0..h {
    for col in 0..w {
        let at = row * w + col;
        if seen[at] || usize::from(indices[at]) == bg { continue; }
        if row == 0 || row == h - 1 || col == 0 || col == w - 1 { cut += 1; }
        components += 1;
        seen[at] = true;
        stack.push(at);
        while let Some(at) = stack.pop() {
            let row = at / w; let col = at % w;
            for (dr, dc) in [(-1i32, 0), (1, 0), (0, -1), (0, 1)] {
                let nr = row as i32 + dr; let nc = col as i32 + dc;
                if nr < 0 || nc < 0 || nr >= h as i32 || nc >= w as i32 { continue; }
                let next = nr as usize * w + nc as usize;
                if seen[next] || usize::from(indices[next]) == bg { continue; }
                seen[next] = true;
                if nr == 0 || nr == h as i32 - 1 || nc == 0 || nc == w as i32 - 1 { cut += 1; }
                stack.push(next);
            }
        }
    }
}
Shape { components, cut }
```

A 4-connected flood fill from each unvisited non-background pixel; the fill is
depth-first through a LIFO stack, but the counts do not depend on the visit order.
`cut` counts **pixels** (not components) on the frame border, each counted once,
at the moment it is first discovered (the seed or a newly pushed neighbour).
`palette_len` here is derived from the **maximum index in the data**, not from
`Texture::palette`, and an empty `indices` gives 1.

## D7. `Texture::to_rgba` / `Texture::entry`

```rust
pub fn to_rgba(&self) -> Vec<u8>
```
`Vec::with_capacity(self.indices.len() * 4)`; for each index `i`, push
`self.palette.get(usize::from(i)).copied().unwrap_or([0, 0, 0, 0])` — four bytes,
straight RGBA in file order. An out-of-range index yields transparent black.

```rust
pub fn entry(&self, index: u8) -> [u8; 4]
```
Same lookup, one entry.

## D8. The in-file tests (`#[cfg(test)] mod tests`)

Copied assertions (they run without the shipped assets):

* `a_payload_of_one_repeated_byte_is_not_an_image`:
  `assert!(palette_size(&[0x00; 64]).is_ok()); assert!(parse(&[0x00; 64]).is_err());`
* `one_palette_entry_and_a_grid_of_zero_indices`: payload = `[0,0,0,255]` + 64 zero bytes;
  `palette == vec![[0,0,0,255]]`, `width == 8`, `height == 8`, `indices.len() == 64`,
  `axis == Axis::Square`, `to_rgba().len() == 256`, `rgba[..4] == [0,0,0,255]`.
* `the_palette_size_is_one_past_the_largest_index`: 40 palette entries
  `[i, 0x80, 0x80, 0xff]` for `i in 0..40`, then 2048 bytes of `7`;
  `palette_size == 40`, `palette.len() == 40`, `indices.len() == 2048`, `width * height == 2048`.
* `a_transparent_magenta_first_entry_survives`: payload `[0xff,0x00,0xff,0x00]` +
  2048 zeros; `palette[0] == [255,0,255,0]`, `indices.len() == 2048`,
  `to_rgba()[..4] == [255,0,255,0]`.
* (asset-gated, skips) `every_shipped_texture_decodes`: every texture parses;
  `area.is_power_of_two()`; `8 <= width <= 256 && 8 <= height <= 256`;
  `max(indices) < palette.len()`; `to_rgba().len() == area * 4`; `square > 600`.
* (asset-gated) `the_payload_length_is_the_palette_plus_the_indices`:
  `palette.len() * 4 + indices.len() == payload.len()` for all 705.

The asset-gated tests read `$DMA3D_ASSETS` / `/tmp/dma3d/pc` / `/tmp/en/dma3d_en`
and look for `DMA3D.MMX`; when it is absent they print
`skipping: the 3D game's assets are not present` and return.

---

# PART E — `src/dma3d/modfile.rs`

## E1. `parse_mesh`

```rust
pub fn parse_mesh(payload: &[u8]) -> Result<ModelMesh>
```

The magic gate first:

```rust
if !payload.starts_with(b"mod II") { bail!("not a `.mod` payload (no `mod II` magic)"); }
```

Then a scan for the structural signature:

```rust
for at in 6..payload.len().min(6000) {
    let Some(c) = count_at(payload, at) else { continue; };            // 1..=400_000
    let indices_end = at + 4 + 2 * c as usize;
    if indices_end + 4 > payload.len() { continue; }
    let v = u32_at(payload, indices_end);
    if !(1..=100_000).contains(&v) { continue; }
    let v = v as usize;
    let vertices_start = indices_end + 4;
    let normals_count_offset = vertices_start + VERTEX_RECORD_BYTES * v;   // + 16*v
    if normals_count_offset + 4 > payload.len() { continue; }
    if u32_at(payload, normals_count_offset) as usize != v { continue; }
    let normals_start = normals_count_offset + 4;
    if normals_start + NORMAL_RECORD_BYTES * v > payload.len() { continue; }
```

Then the two halves of the signature:

* **Indices**: read `c` little-endian u16s at `at + 4 + 2*k`.
  Every index must be `< v`; the set of distinct indices must have size `>= 2`
  (`collect::<HashSet<_>>().len() < 2` ⇒ `continue`).
* **Vertices**: for `k in 0..v`, `off = vertices_start + 16*k`; `coord = [f32_at(off), f32_at(off+4), f32_at(off+8)]`.
  Every coordinate must satisfy `plausible(x)`; on failure `positions.clear()` and
  `break` (the loop then fails the `positions.len() != v` test below).
  `marked` counts records whose fourth float is within `MARKER_EPSILON` of `VERTEX_MARKER`.
  After the loop: `if positions.len() != v || marked * 10 < v * 9 { continue; }`
  — i.e. **at least 90 %** of the records must carry `w == 1.0`.
* **Normals**: for `k in 0..v`, `off = normals_start + 16*k`; same three floats,
  plus `length = (x²+y²+z²).sqrt()` must satisfy `plausible` for every coordinate
  **and** `(length - 1.0).abs() <= UNIT_LENGTH_EPSILON`; `unit` counts records whose
  fourth float is within `MARKER_EPSILON` of `0.0` (checked with `f32_at(off+12).abs()`).
  After the loop: `if normals.len() != v || unit * 10 < v * 9 { continue; }`.

Then the tail header, **recorded, never required**:

```rust
let tail_at = normals_start + NORMAL_RECORD_BYTES * v;
let tail_header = if tail_at + 12 <= payload.len() {
    Some((u32_at(payload, tail_at), u32_at(payload, tail_at + 4), u32_at(payload, tail_at + 8)))
} else { None };
```

and `return Ok(ModelMesh { index_count_offset: at, indices, positions, normals, tail_header })`.

If no `at` in `6..min(len, 6000)` matched: `bail!("no mesh chain in the payload")`.

The scan starts at byte **6** (past the `mod II` magic) and stops at
`min(payload.len(), 6000)` — "6000 is well past every measured header (163-263)
and keeps a corrupt file from being scanned end to end."

Doc evidence, `docs/DMA3D.md` §3.1: "the chain is located **by its structural
signature**: `C` opens `2*C` bytes whose successor `u32` opens `V` records of
`w == 1.0`, then a second `u32` equal to `V` opens unit-length records of
`w == 0.0`. Requiring both halves is what stops a stray `w == 1.0` run from being
read as a vertex array."

### The offsets, in the reader's order

| Quantity | Expression |
|---|---|
| index count word | at `at` (4 bytes, LE) |
| index list | `at + 4 .. at + 4 + 2*C` |
| vertex count word `V` | `indices_end` (4 bytes, LE) |
| vertices | `vertices_start .. vertices_start + 16*V` |
| normal count word | `vertices_start + 16*V` |
| normals | `normals_count_offset + 4 .. + 16*V` |
| tail | `tail_at = normals_start + 16*V` |

## E2. `count_at`, `plausible`, the little-endian readers

```rust
fn count_at(payload: &[u8], off: usize) -> Option<u32> {
    if off + 4 > payload.len() { return None; }
    let c = u32_at(payload, off);
    if !(1..=400_000).contains(&c) { return None; }
    Some(c)
}
fn plausible(f: f32) -> bool { f.is_finite() && f.abs() < 1.0e7 }
fn f32_at(bytes: &[u8], off: usize) -> f32 { f32::from_le_bytes([bytes[off], bytes[off+1], bytes[off+2], bytes[off+3]]) }
fn u32_at(bytes: &[u8], off: usize) -> u32 { u32::from_le_bytes([bytes[off], bytes[off+1], bytes[off+2], bytes[off+3]]) }
fn u16_at(bytes: &[u8], off: usize) -> u16 { u16::from_le_bytes([bytes[off], bytes[off+1]]) }
```

**All `.mod` payload words are little-endian**, unlike the container and the
`.flm` stream.

## E3. `ModelMesh` accessors

Already given in §3.5. The identity the tests rely on:
`tail_offset() == normal_array_offset() + 4 + 16 * normals.len()`, and with the
reader's formula `normal_array_offset() == at + 4 + 2*C + 4 + 16*V`.

## E4. The in-file tests

* `a_payload_without_the_magic_is_not_a_model`: `parse_mesh(b"not a model at all").is_err()`.
* `the_ui_records_carry_no_mesh`: `parse_mesh(&[0u8; 276]).is_err()`.
* `a_synthetic_chain_parses` — the full synthetic payload is described in the Rust:
  `b"mod II"` + `0u16` + `3u32` (C=3) + indices `[1, 2, 1]` as u16 LE + `3u32` (V=3)
  + three 16-byte vertex records `(x,y,z,1.0)` for `[[0,1,2],[3,4,5],[6,7,8]]`
  + `3u32` + three 16-byte normal records `(nx,ny,nz,0.0)` for `[[1,0,0],[0,1,0],[0,0,1]]`
  + `0u32`, `1u32`, `3u32`. Assertions:
  `vertex_count() == 3`; `indices == vec![1, 2, 1]`;
  `positions == vec![[0.0,1.0,2.0],[3.0,4.0,5.0],[6.0,7.0,8.0]]`;
  `normals == vec![[1.0,0.0,0.0],[0.0,1.0,0.0],[0.0,0.0,1.0]]`;
  `tail_header == Some((0, 1, 3))`; `tail_offset() == 122`;
  `&p[tail_offset()..] == [0,0,0,0,1,0,0,0,3,0,0,0]`; `normal_array_offset() == 70`.
* (asset-gated) `the_mesh_parses_out_of_the_shipped_props`: every payload named
  `createProp*` and not containing `UI` parses; `4 <= vertex_count <= 4096`;
  every index `< vertex_count`; `indices.contains(&0)`; no index equals
  `vertex_count`; `normals.len() == vertex_count`; if `tail_header` is `Some`,
  `header.2 == vertex_count`; every normal is unit length within `1e-3`;
  `parsed >= 30`.
* (asset-gated) `the_ui_records_parse_as_no_mesh`: every `createPropUI*` payload
  fails to parse, and `ui > 0`.

---

# PART F — `src/dma3d/anim.rs`

## F1. `locate`

```rust
pub fn locate(payload: &[u8], mesh: &ModelMesh) -> Records
```

1. `tail_offset = mesh.tail_offset()`; `header = None`; `pair_count = 0`; `pairs_in_unit_range = None`.
2. If `tail_offset + 12 <= payload.len()`, read the three **little-endian** u32s at
   `tail_offset`, `+4`, `+8` as `head`. If `head.2 as usize == mesh.vertex_count()`
   — and **only** then:
   * `header = Some(head)`; `pair_count = v` (the vertex count);
   * `end = tail_offset + 12 + 8*v`; if `end <= payload.len()`, count `inside` =
     the number of `k in 0..v` whose pair `(a, b)` at `tail_offset + 12 + 8*k`
     satisfies `(-0.001..=1.001).contains(&a) && (-0.001..=1.001).contains(&b)`,
     and set `pairs_in_unit_range = Some(inside)`. (If `end` overflows the payload,
     it stays `None`.)
3. `end_of_pairs = tail_offset + 12 + 8*pair_count`;
   `remainder = payload[end_of_pairs..].to_vec()` when that fits, else `Vec::new()`.
4. `Records { tail_offset, header, pairs_in_unit_range, pair_count, remainder, tracks: track_names(payload) }`.

Note the range is a **closed** `-0.001..=1.001`, i.e. slightly looser than `0..1` —
and `texcoords_of` requires *every* pair inside it, not `Some(count)` alone.

## F2. `track_names`

```rust
pub fn track_names(payload: &[u8]) -> Vec<TrackName>
```

Identical in shape to `texture_names_in` (C7) with three differences:

* the acceptance test is `!text.contains("TGA") && !text.starts_with("key")` ⇒ reject
  (so a record must either contain `"TGA"` **or** start with `"key"` — lowercase);
* the collected value is `TrackName { name: text, at: table + record * 32 }`
  (the record's offset in the payload);
* the same 4-byte advance / whole-table jump.

## F3. `dump`

```rust
pub fn dump(name: &str, payload: &[u8], mesh: &ModelMesh, records: &Records) -> String
```

Built with `writeln!`/`write!` into a `String`, every error ignored (`let _ =`).
Exact output:

```
# <name>
# payload <payload.len()> bytes; mesh <mesh.indices.len()> indices / <mesh.vertex_count()> vertices; mesh chain at <mesh.index_count_offset:#x>
# the mesh decoder stops at <records.tail_offset:#x>; everything below is what it left

[header]
  u32 <a>, u32 <b>, u32 <v>   the [0, 1, V] shape, the third word = the vertex count
```
or, when `records.header` is `None`:

```
[header]
  absent. 50 of the 144 mesh payloads open with something else, so the shape is not required
```

Then the pairs section, when `pairs_in_unit_range` is `Some(inside)`:

```
[pairs]  <pair_count> float pairs, <inside> of them wholly inside 0..1
  all inside 0..1 reads as texture placement; none inside 0..1 means the pairs are something else. Which payloads are which is not measured
  <k:4>: (<a:.5>, <b:.5>)        <- for k in 0..min(pair_count, 8), breaking if at+8 > payload.len()
  ... <pair_count - 8> more, not printed        <- only when pair_count > 8
```

or, when it is `None`:

```
[pairs]  not read: the header is not the [0, 1, V] shape
```

Then the records section:

```
[records]  <remainder.len()> bytes at <remainder_offset():#x>, undecoded
  The record layout is not measured: not the record size, not the keyframes per record, and not which node each track drives. These are the game's own bytes, in the game's own order, rendered four ways so a candidate layout can be checked against them.
  <remainder_offset() + start:#06x>  <hex bytes joined by " ">     <- for start in (0..min(len,1024)).step_by(16)
  ... <len - 1024> bytes not printed                              <- only when len > 1024

  as u32 (little-endian), the first <words> of them:
  <u32> <u32> ...        <- first 24 chunks of exactly 4 bytes, on ONE line, each preceded by two spaces
                            (write! without a newline, then a single writeln!)
  as f32 (little-endian):
  <f32:.5> <f32:.5> ...  <- same shape, 5 decimals

  as u16 (little-endian):
  <u16> <u16> ...        <- first 32 chunks of exactly 2 bytes
```

Note `words = bytes.len() / 4` (integer division, so a ragged tail is silently
short), and the three "views" are each a single line of space-separated values
each prefixed by two spaces, terminated by one `writeln!`.

Finally:

```
[track names]
  none. Only six payloads name theirs, and none of those carries a mesh
```
or one line per track:

```
  <name left-aligned in 24> at <at:#x>
```

The document has **no trailing newline beyond the last `writeln!`** of the tracks
section.

## F4. `records_name`

```rust
pub fn records_name(stem: &str) -> String { format!("{stem}.anim.txt") }
```
Called with `format!("{index:05}_{stem}.gltf")`, so the file is
`{index:05}_{name}.gltf.anim.txt`.

---

# PART G — `src/dma3d/gltf.rs`

## G1. `base64` and `base64_decode`

```rust
const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64(data: &[u8]) -> String
```
For each `chunk` in `data.chunks(3)`: `b = [chunk[0], chunk.get(1).unwrap_or(0), chunk.get(2).unwrap_or(0)]`,
`n = (b[0] << 16) | (b[1] << 8) | b[2]`, and push four characters:
`BASE64[(n >> 18) & 63]`, `BASE64[(n >> 12) & 63]`, then `BASE64[(n >> 6) & 63]`
if `chunk.len() > 1` else `'='`, then `BASE64[n & 63]` if `chunk.len() > 2` else
`'='`. Standard RFC 4648 with `+` and `/` and `=` padding.

```rust
fn base64_decode(text: &str) -> Result<Vec<u8>>
```
* Skip every `b'='` and every `b.is_ascii_whitespace()` byte; anything else must
  be a `BASE64` character, else
  `bail!("not a base64 character: {:?}", c as char)` — note the **debug** formatting
  of the char, so a non-ASCII byte prints as `'é'`-style Rust debug (`'\u{e9}'`).
* Group the remaining sextets in fours; `n = group.iter().fold(0u32, |acc, &s| (acc << 6) | u32::from(s))`.
* Shifts by group length:

  | `group.len()` | shifts |
  |---|---|
  | 4 | `[16, 8, 0]` |
  | 3 | `[10, 2]` |
  | 2 | `[4]` |
  | else | `bail!("a base64 group of {} sextets", group.len())` |

  and push `((n >> shift) & 0xff) as u8` for each shift.

The doc comment states the bug this fixed: "three sextets carry 18 bits (two
bytes at shifts 10 and 2) and two sextets carry 12 bits (one byte at shift 4).
Reading a short group with the full group's shifts silently corrupts the tail of
every buffer."

## G2. `json` — the minimal reader

Full grammar, all error strings verbatim:

```
parse(text):
  at = 0; value = parse_value(bytes, &mut at)?; skip(bytes, &mut at);
  if at != bytes.len() { bail!("trailing content at byte {at}"); }
  Ok(value)

skip(b, at):        while *at < b.len() && b[*at].is_ascii_whitespace() { *at += 1 }
expect(b, at, c):   skip; if *at >= b.len() || b[*at] != c { bail!("expected {:?} at byte {}", c as char, at) }; *at += 1

parse_value(b, at):
  skip;
  if *at >= b.len() { bail!("unexpected end of input") }
  match b[*at] {
    b'{' => parse_object, b'[' => parse_array, b'"' => String(parse_string),
    b't' => literal(b, at, "true", Value::Bool(true)),
    b'f' => literal(b, at, "false", Value::Bool(false)),
    b'n' => literal(b, at, "null", Value::Null),
    _    => parse_number,
  }

literal(b, at, word, value):
  if b[*at..].starts_with(word.as_bytes()) { *at += word.len(); Ok(value) }
  else { bail!("expected the literal {word} at byte {at}") }

parse_string(b, at):
  expect(b, at, b'"')
  loop {
    if *at >= b.len() { bail!("unterminated string") }
    match b[*at] {
      b'"' => { *at += 1; return Ok(out) }
      b'\\' => { *at += 1;
                 if *at >= b.len() { bail!("unterminated escape") }
                 match b[*at] {
                   b'"' => push('"'), b'\\' => push('\\'), b'/' => push('/'),
                   b'b' => push('\u{8}'), b'f' => push('\u{c}'), b'n' => push('\n'),
                   b'r' => push('\r'), b't' => push('\t'),
                   b'u' => { if *at + 4 >= b.len() { bail!("truncated \\u escape") }
                             let hex = str::from_utf8(&b[*at+1 .. *at+5]).context("\\u escape is not hex")?;
                             let code = u32::from_str_radix(hex, 16).context("\\u escape")?;
                             out.push(char::from_u32(code).unwrap_or('\u{fffd}')); *at += 4 }
                   other => bail!("unknown escape {:?}", other as char),
                 }
                 *at += 1 }
      _ => { let start = *at; let width = utf8_width(b[*at]); *at += width;
             let end = (*at).min(b.len());
             out.push_str(&String::from_utf8_lossy(&b[start..end])) }
    }
  }

utf8_width(first): 0x00..=0x7f => 1, 0xc0..=0xdf => 2, 0xe0..=0xef => 3, _ => 4

parse_number(b, at):
  let start = *at;
  if b[*at] == b'-' || b[*at] == b'+' { *at += 1 }
  while b[*at] is ascii_digit or one of . e E - + { *at += 1 }
  let text = str::from_utf8(&b[start..*at]).context("number is not UTF-8")?;
  let n: f64 = text.parse().with_context(|| format!("bad number {text:?}"))?;
  Value::Number(n)

parse_array(b, at):
  expect(b, at, b'['); skip;
  if b[*at] == b']' { *at += 1; return Array(vec![]) }
  loop { items.push(parse_value(b, at)?); skip;
         if *at >= b.len() { bail!("unterminated array") }
         match b[*at] { b',' => *at += 1, b']' => { *at += 1; return Array(items) },
                        _ => bail!("expected ',' or ']' at byte {}", at) } }

parse_object(b, at):
  expect(b, at, b'{'); skip;
  if b[*at] == b'}' { *at += 1; return Object(vec![]) }
  loop { let key = parse_string(b, at)?; expect(b, at, b':'); let value = parse_value(b, at)?;
         entries.push((key, value)); skip;
         if *at >= b.len() { bail!("unterminated object") }
         match b[*at] { b',' => *at += 1, b'}' => { *at += 1; return Object(entries) },
                        _ => bail!("expected ',' or '}}' at byte {}", at) } }
```

`Value::get(key)` returns the **first** entry whose key matches. `as_u64`
accepts only `Number(n)` with `n >= 0.0` and truncates; `as_usize` maps it to
`usize`; `as_str`/`as_array` are the obvious.

Duplicated keys are kept in order and `get` finds the first — which matters
because the writer never emits duplicates.

## G3. `write_tail` / `tail_name`

```rust
pub fn write_tail(path: &Path, tail: &[u8]) -> Result<()>
pub fn tail_name(stem: &str) -> String
```
`std::fs::write(path, tail)` with context `format!("writing {}", path.display())`;
`tail_name` is `format!("{stem}.tail")`. Written with the payload slice
`&payload[mesh.tail_offset()..]`, so `payload = glTF's geometry input + tail`.

## G4. `read_gltf`

```rust
pub fn read_gltf(path: &Path) -> Result<ReadBack>
```

1. `text = std::fs::read_to_string(path)` with context `format!("reading {}", path.display())`.
2. `document = json::parse(&text)?`.
3. `version`: `document.get("asset")?.get("version")?.as_str()` with context `format!("{}: no asset.version", path.display())`.
4. `generator`: the same chain, `Option`.
5. `material_count`: `document.get("materials").and_then(as_array).map(len).unwrap_or(0)`.
6. `accessors`, `bufferViews`, `buffers`: each `get(...)` + `as_array()` with
   context `format!("{}: no accessors", …)`, `"{}: no bufferViews"`, `"{}: no buffers"`.
7. `uri`: `buffers.first()?.get("uri")?.as_str()` with context `"{}: no buffer uri"`;
   `data = uri.strip_prefix("data:application/octet-stream;base64,")` with context
   `"{}: buffer uri is not a base64 data uri"`; `binary = base64_decode(data)?`.
8. `declared = buffers.first()?.get("byteLength")?.as_u64()` with context `"{}: no buffer byteLength"`;
   `binary.len() < declared` ⇒ `bail!("{}: buffer holds {} bytes but declares {}", path.display(), binary.len(), declared)`.
9. The first mesh primitive: `document.get("meshes")?.as_array()?.first()?.get("primitives")?.as_array()?.first()?`
   with context `"{}: no mesh primitive"`.
10. `attributes = primitive.get("attributes")` with context `"{}: no attributes"`;
    `position_index = attributes.get("POSITION")?.as_usize()` context `"{}: no POSITION"`;
    `normal_index = attributes.get("NORMAL")?.as_usize()` context `"{}: no NORMAL"`;
    `index_index = primitive.get("indices")?.as_usize()` context `"{}: no indices"`.
11. The `scalars(index)` closure:
    * `accessor = accessors.get(index)` context `format!("accessor {index} does not exist")`;
    * `view = buffer_views.get(accessor.get("bufferView")?.as_usize())` with the inner
      context `format!("{}: accessor {index} has no bufferView", path.display())` and the outer
      `format!("bufferView for accessor {index} does not exist")`;
    * `offset = view.get("byteOffset").and_then(as_u64).unwrap_or(0) as usize`;
    * `count = accessor.get("count")?.as_usize()` context `"{}: accessor {index} has no count"`;
    * `component = accessor.get("componentType")?.as_u64()` context `"{}: accessor {index} has no componentType"`;
    * `width = match component { 5123 => 2, 5126 => 4, other => bail!("component type {other} is not supported") }`;
    * `parts = match accessor.get("type")?.as_str() { Some("SCALAR") => 1, Some("VEC2") => 2, Some("VEC3") => 3, other => bail!("accessor type {other:?} is not supported") }`;
    * read `count * parts` scalars at `offset + k*width`, LE; `5123` ⇒ `u16` widened
      to `f32`, otherwise `f32`; `out.push(raw as f32)`.
12. `index_count = accessors[index_index].get("count")?.as_usize()` context `"{}: index accessor has no count"`;
    `index_scalars = scalars(index_index)?`; `indices = index_scalars.iter().take(index_count).map(|f| *f as u16).collect()`.
13. `positions_flat = scalars(position_index)?`; `normals_flat = scalars(normal_index)?`;
    if either `len % 3 != 0` ⇒ `bail!("{}: accessor length is not a multiple of 3", path.display())`;
    chunk into triples.
14. Optional `TEXCOORD_0`: when present, `tex = accessors.get(tex_index)` context
    `"{}: TEXCOORD_0 names accessor {tex_index} which does not exist"`,
    `count = tex.get("count")?.as_usize()` context `"{}: TEXCOORD_0 has no count"`,
    `flat = scalars(tex_index)?`; `flat.len() != count * 2` ⇒
    `bail!("{}: TEXCOORD_0 holds {} scalars for {count} elements", path.display(), flat.len())`;
    chunk into pairs. Absent ⇒ an empty `Vec`.
15. `Ok(ReadBack { version, generator, indices, positions, normals, texcoords, material_count })`.

## G5. `json_string`

```rust
fn json_string(s: &str) -> String
```
`"` + §2.3's `script.rs` escaper + `"`. Used for every `name` field the writer
emits.

## G6. `Material` and `from_texture`

```rust
pub struct Material { pub name: String, pub png: Vec<u8> }
impl Material { pub fn from_texture(name: &str, rgba: &[u8], width: usize, height: usize) -> Result<Self> }
```
`from_texture` = `Ok(Self { name: name.to_string(), png: encode_png(rgba, width, height)? })`.
(`dma3d.rs` builds `Material` directly from PNG bytes read off disk instead; the
port needs both constructors.)

## G7. `Accessor` and its JSON

```rust
struct Accessor { offset: usize, length: usize, component_type: u32, kind: &'static str, count: usize, min_max: Option<[f32; 6]> }
impl Accessor { fn json(&self, buffer_view: usize) -> String }
```

Field order in `json`:

1. `  "bufferView": <buffer_view>`
2. `  "componentType": <component_type>`
3. `  "count": <count>`
4. `  "type": "<kind>"`
5. only when `min_max` is `Some`: `  "min": [minx, miny, minz],\n  "max": [maxx, maxy, maxz]`

joined with `",\n  "` and wrapped as `format!(" {{\n{}\n }}", …)`. So the emitted
object is:

```
 {
  "bufferView": 1,
  "componentType": 5126,
  "count": 4,
  "type": "VEC3",
  "min": [0, 0, 0],
  "max": [1, 1, 1]
 }
```

(a leading space before `{`, two-space indent, and the `min`/`max` line pair
sharing one joined field, which is why `"max"` is indented only two spaces in
contrast to three). `min_max` is `Some` **only for the POSITION accessor**.

Float printing is Rust's `{}` on `f32` — integral values print with no decimal
point. A `-0.0` prints `-0`, `inf`/`NaN` cannot occur because only finite values
feed min/max.

## G8. `write_gltf` — the binary buffer

```rust
pub fn write_gltf(path: &Path, name: &str, mesh: &ModelMesh,
                  materials: &[Material], texcoords: Option<&[[f32; 2]]>) -> Result<()>
```

`pad4(buf)` pushes `0x00` until `buf.len() % 4 == 0`.

Order of the buffer, **each section padded to 4 before it starts**:

| Section | Contents | Notes |
|---|---|---|
| (pad) | zeros | `indices_start = buffer.len()` after padding |
| indices | `mesh.indices` as **little-endian u16**, unchanged (zero-based as stored) | `indices_len` |
| (pad) | zeros | `positions_start` |
| positions | every `mesh.positions` `[x,y,z]` as three LE f32 | `positions_len` |
| (pad) | zeros | `normals_start` |
| normals | every `mesh.normals` `[nx,ny,nz]` as three LE f32 | `normals_len` |
| (pad) | zeros | `texcoords_start` |
| texcoords | each `[u, v]` as two LE f32, written **only if** `uv.len() == mesh.positions.len()` | `texcoords_len`, `write_texcoords = true` |
| (pad) | zeros | final |

`min`/`max` for POSITION are computed **over finite values only**:

```rust
let mut min = [f32::INFINITY; 3];
let mut max = [f32::NEG_INFINITY; 3];
for v in &mesh.positions {
    for (axis, value) in v.iter().enumerate() {
        if value.is_finite() {
            if *value < min[axis] { min[axis] = *value; }
            if *value > max[axis] { max[axis] = *value; }
        }
    }
}
```
An empty or all-non-finite mesh leaves `min`/`max` at the infinities, which then
print as `inf`/`-inf` in the JSON — the port must reproduce that literal text.

Accessors, always in this order:

| # | section | component_type | kind | count | min_max |
|---|---|---|---|---|---|
| 0 | indices | 5123 | `SCALAR` | `mesh.indices.len()` | `None` |
| 1 | positions | 5126 | `VEC3` | `mesh.positions.len()` | `Some([min…max])` |
| 2 | normals | 5126 | `VEC3` | `mesh.normals.len()` | `None` |
| 3 | texcoords | 5126 | `VEC2` | `mesh.positions.len()` | only when `write_texcoords` |

`bufferViews`: one per accessor, in the same order, each
`{ "buffer": 0, "byteOffset": <offset>, "byteLength": <length> }`.

Materials — for `(i, m)` in `materials.iter().enumerate()` (payload order):

* `images_json.push(" {\n  \"name\": <json_string(name)>, \n  \"uri\": \"data:image/png;base64,<base64(png)>\"\n }")`
* `textures_json.push(" {\n  \"source\": {i},\n  \"sampler\": 0\n }")`
* `materials_json.push(" {\n  \"name\": <json_string(name)>, \n  \"pbrMetallicRoughness\": {\n   \"baseColorTexture\": {\n    \"index\": {i}\n   },\n   \"metallicFactor\": 0.0,\n   \"roughnessFactor\": 1.0\n  }\n }")`

Note `0.0` and `1.0` are printed by Rust's `{}` on `f64` as `0` and `1` — the
emitted text reads `"metallicFactor": 0,` and `"roughnessFactor": 1,`.

The document sections, pushed in this exact order:

1. `"asset": {\n "version": "2.0",\n "generator": "mmf-extract"\n}`
2. `"scene": 0`
3. `"scenes": [\n {\n  "name": "scene",\n  "nodes": [0]\n }\n]`
4. `"nodes": [\n {\n  "name": <json_string(name)>,\n  "mesh": 0\n }\n]`
5. `"meshes": [\n {\n  "name": <json_string(name)>,\n  "primitives": [\n   {\n    "attributes": {\n     "POSITION": 1,\n     "NORMAL": 2[,\n     "TEXCOORD_0": 3]\n    },\n    "indices": 0[,\n    "material": 0]\n   }\n  ]\n }\n]`
   — the `TEXCOORD_0` fragment is `",\n     \"TEXCOORD_0\": 3"` and the material
   fragment `",\n    \"material\": 0"`.
6. **only when `materials` is non-empty**, in this order:
   `"materials": [\n <joined with ",\n ">\n]`,
   `"textures": [\n <joined with ",\n ">\n]`,
   `"images": [\n <joined with ",\n ">\n]`,
   `"samplers": [\n {\n  "magFilter": 9729,\n  "minFilter": 9987,\n  "wrapS": 10497,\n  "wrapT": 10497\n }\n]`
7. `"accessors": [\n <joined with ",\n">\n]`
8. `"bufferViews": [\n <joined with ",\n">\n]`
9. `"buffers": [\n {\n  "byteLength": <buffer.len()>,\n  "uri": "data:application/octet-stream;base64,<base64(buffer)>"\n }\n]`

Then:

```rust
let document = format!("{{\n{}\n}}\n", sections.join(",\n"));
std::fs::write(path, document).with_context(|| format!("writing {}", path.display()))?;
```

So the file is `{\n` + the sections joined by `",\n"` + `\n}\n`, and **ends with a
newline**.

### G8.1 The exact emitted glTF document

`write_gltf(path, "test", mesh, &[], Some(&uv))` from the in-file test
`a_written_document_reads_back` — mesh `indices [0,1,2,1,2,3]`,
`positions [[0,0,0],[1,0,0],[0,1,0],[0,0,1]]`,
`normals [[0,1,0],[1,0,0],[0,0,1],[0,0,1]]`, `tail_header Some((0,1,4))`,
**no** materials, **no** texcoords (the `None` write). The complete file, byte
for byte (79 lines, LF endings, trailing newline):

```json
{
"asset": {
 "version": "2.0",
 "generator": "mmf-extract"
},
"scene": 0,
"scenes": [
 {
  "name": "scene",
  "nodes": [0]
 }
],
"nodes": [
 {
  "name": "test",
  "mesh": 0
 }
],
"meshes": [
 {
  "name": "test",
  "primitives": [
   {
    "attributes": {
     "POSITION": 1,
     "NORMAL": 2
    },
    "indices": 0
   }
  ]
 }
],
"accessors": [
 {
  "bufferView": 0,
  "componentType": 5123,
  "count": 6,
  "type": "SCALAR"
 },
 {
  "bufferView": 1,
  "componentType": 5126,
  "count": 4,
  "type": "VEC3",
  "min": [0, 0, 0],
  "max": [1, 1, 1]
 },
 {
  "bufferView": 2,
  "componentType": 5126,
  "count": 4,
  "type": "VEC3"
 }
],
"bufferViews": [
 {
  "buffer": 0,
  "byteOffset": 0,
  "byteLength": 12
 },
 {
  "buffer": 0,
  "byteOffset": 12,
  "byteLength": 48
 },
 {
  "buffer": 0,
  "byteOffset": 60,
  "byteLength": 48
 }
],
"buffers": [
 {
  "byteLength": 108,
  "uri": "data:application/octet-stream;base64,AAABAAIAAQACAAMAAAAAAAAAAAAAAAAAAACAPwAAAAAAAAAAAAAAAAAAgD8AAAAAAAAAAAAAAAAAAIA/AAAAAAAAgD8AAAAAAACAPwAAAAAAAAAAAAAAAAAAAAAAAIA/AAAAAAAAAAAAAIA/"
 }
]
}
```

Every detail of that text is load-bearest:

* after a bare `{` line, each section key starts at **column 0**;
* the section separator is `",\n"` — a comma on the previous line's last line;
* nested objects indent by one space per level (` `, `  `, `   `, `    `, `     `);
* the accessors and bufferViews arrays are joined by `","` + `\n` while their
  elements already carry a leading space;
* `"min": [0, 0, 0]` — three values, comma+space separated, Rust `{}` on `f32`;
* `"max"` is indented two spaces, not three (§G7);
* the base64 URI is the whole 108-byte buffer with **no** line breaks.

With texcoords present (`uv` of four pairs all `0.0`/`1.0`) the document differs
in exactly three places: the `attributes` object gains
`     "TEXCOORD_0": 3,` (after `"NORMAL": 2`), the accessors array gains

```
 {
  "bufferView": 3,
  "componentType": 5126,
  "count": 4,
  "type": "VEC2"
 },
```

and the buffer grows by `4 * 2 * 4 = 32` bytes with `byteOffset` 108 and
`byteLength` 32 (positions stay at 12/48 and normals at 60/48).

## G9. The in-file tests

* `base64_round_trips_every_length`: for `n in [0,1,2,3,4,5,6,7,726,728,3060,6848]`,
  `data = (0..n).map(|i| (i*7+3) as u8)`, `base64_decode(base64(&data)) == data`.
* `a_written_document_reads_back`: write without texcoords ⇒ `plain.version == "2.0"`,
  `plain.indices == mesh.indices`, `plain.positions == mesh.positions`,
  `plain.normals == mesh.normals`, `plain.texcoords.is_empty()`; write with
  `uv = [[0,0],[1,0],[0,1],[1,1]]` ⇒ the same three equalities and
  `back.texcoords == uv`.

---

# PART H — `src/dma3d/render.rs`

## H1. `render`

```rust
pub fn render(mesh: &ModelMesh) -> Preview
```

All arithmetic is `f32`; the canvas is `PREVIEW_PIXELS × PREVIEW_PIXELS = 256×256`.

1. `rgba = vec![0u8; 256*256*4]` (transparent black background),
   `depth = vec![-f32::INFINITY; 256*256]`, `covered = 0`.
2. **Bounds** from `mesh.positions` — `min`/`max` per axis, **without** a finiteness
   guard here (unlike `write_gltf`); NaN comparisons leave the initial infinities
   in place.
3. ```rust
   let centre = [(min[0]+max[0]) * 0.5, (min[1]+max[1]) * 0.5, (min[2]+max[2]) * 0.5];
   let extent = (max[0]-min[0]).max(max[1]-min[1]).max(max[2]-min[2]).max(1.0e-6);
   let scale = (width.min(height) as f32) * 0.42 / extent;      // 256 * 0.42 / extent
   let half  = width.min(height) as f32 * 0.5;                  // 128.0
   let light = normalise(LIGHT);                                // LIGHT = [0.45, 0.75, 0.49]
   let eye   = normalise([1.0, 0.8, 1.0]);
   let right = normalise(cross([0.0, 1.0, 0.0], eye));
   let up    = cross(eye, right);                              // NOT renormalised
   ```
4. `project(p)`: `d = p - centre`;
   `sx = dot(d, right) * scale + half`; `sy = half - dot(d, up) * scale`;
   `sz = dot(d, eye)`.
5. `let triangles = mesh.indices.len() / 3;` — a ragged index tail is **dropped**
   (`docs/DMA3D.md`: "A model whose index count is not a multiple of three has a
   ragged tail; the extra indices are dropped").
6. For each triangle `t`, `corners[k] = mesh.positions.get(mesh.indices[t*3+k]).copied().unwrap_or([0.0; 3])`
   for `k in 0..3` — an out-of-range index yields the origin.
7. `u = corners[1] - corners[0]`, `v = corners[2] - corners[0]`,
   `n = cross(u, v)`; `n_len = sqrt(n·n)`; if `!n_len.is_finite() || n_len < 1.0e-12` ⇒
   `continue` (degenerate face). Then `n /= n_len`.
8. `facing = n · light`; `lambert = facing.abs().clamp(0.0, 1.0)`;
   `shade = 0.28 + 0.72 * lambert`;
   ```rust
   let colour = [
       (200.0 * shade).clamp(0.0, 255.0) as u8,
       (206.0 * shade).clamp(0.0, 255.0) as u8,
       (216.0 * shade).clamp(0.0, 255.0) as u8,
   ];
   ```
   (`as u8` on an `f32` in Rust **truncates toward zero** and saturates; the
   clamp bounds it to 0..255.)
9. Project the three corners; **skip** the triangle when all of
   `[ax,bx,cx]` are `< 0.0` or `>= width`, or all of `[ay,by,cy]` are `< 0.0` or
   `>= height`.
10. Bounding box of the pixel loop:
    ```rust
    let min_x = ax.min(bx).min(cx).max(0.0) as usize;
    let max_x = (ax.max(bx).max(cx).min(width as f32 - 1.0)) as usize;
    let min_y = ay.min(by).min(cy).max(0.0) as usize;
    let max_y = (ay.max(by).max(cy).min(height as f32 - 1.0)) as usize;
    ```
    Note `width as f32 - 1.0` is `255.0`, so a vertex exactly at 255.0 is included.
11. `let area = (bx - ax) * (cy - ay) - (by - ay) * (cx - ax);`
    `if area.abs() < 1.0e-9 { continue; }` (edge-on); `let inv = 1.0 / area;`
12. For `py in min_y..=max_y`, `px in min_x..=max_x`:
    ```rust
    let sx = px as f32 + 0.5;  let sy = py as f32 + 0.5;
    let w0 = ((bx - ax) * (sy - ay) - (by - ay) * (sx - ax)) * inv;
    let w1 = ((cx - bx) * (sy - by) - (cy - by) * (sx - bx)) * inv;
    let w2 = ((ax - cx) * (sy - cy) - (ay - cy) * (sx - cx)) * inv;
    if w0 < -1.0e-6 || w1 < -1.0e-6 || w2 < -1.0e-6 { continue; }
    let z = w0 * az + w1 * bz + w2 * cz;
    let at = py * width + px;
    if z <= depth[at] { continue; }
    depth[at] = z;
    let base = at * 4;
    rgba[base] = colour[0]; rgba[base+1] = colour[1]; rgba[base+2] = colour[2]; rgba[base+3] = 255;
    ```
    The depth test is **strictly greater wins**, and the initial depth is
    `-inf`, so the first triangle to touch a pixel always paints it.
13. `covered = rgba.chunks(4).filter(|a| a[3] != 0).count()`.
14. `Preview { rgba, covered }`.

## H2. The vector helpers

```rust
const LIGHT: [f32; 3] = [0.45, 0.75, 0.49];
fn cross(a, b) -> [f32; 3] { [a[1]*b[2]-a[2]*b[1], a[2]*b[0]-a[0]*b[2], a[0]*b[1]-a[1]*b[0]] }
fn dot(a, b) -> f32 { a[0]*b[0] + a[1]*b[1] + a[2]*b[2] }
fn normalise(v: [f32; 3]) -> [f32; 3] {
    let len = dot(v, v).sqrt();
    if len < 1.0e-12 { return v; }
    [v[0]/len, v[1]/len, v[2]/len]
}
```

---

# PART I — Tests that pin the behaviour (assertions copied)

## I1. `tests/scripts.rs`

**`original_script_payloads_are_byte_identical`** — asset-gated (skips when
`/RawAssets/HD.MMS` or `$DMA_ASSETS/HD.MMS` is absent):

```rust
let n = container.resource_count as usize;
let meta = container.script.as_ref().expect("script metadata");
assert_eq!(n, 4813);
assert_eq!(meta.types.len(), 89);

let written = script::extract_container(&container, &tmp, true).expect("extract");
assert_eq!(written, n, "every script object must be written");

for i in 0..n {
    let safe = sanitize(container.name(i), i);
    let extracted = std::fs::read(tmp.join(format!("{i:05}_{safe}.bin"))).…;
    let start = container.offsets[i] as usize;
    let end = container.offsets[i + 1] as usize;
    assert_eq!(extracted, container.payload(i), "resource {i} ('{}'): extracted bytes differ from the payload");
    assert_eq!(extracted, &on_disk[start..end], "resource {i} ('{}'): extracted bytes differ from [{start},{end}) on disk");
    assert_eq!(container.plain_len(i), container.payload_len(i), "resource {i}: payload is not stored plainly");
}
```

**`script_metadata_accounts_for_every_payload_byte`**:

```rust
assert_eq!(meta.version, 11);
assert_eq!(meta.type_count as usize, meta.types.len());
assert_eq!(meta.element_counts.len(), n);
assert_eq!(meta.object_types.len(), n);
assert_eq!(meta.object_names.len(), n);
assert_eq!(meta.metadata_end, container.offsets[0] as usize);

for i in 0..n {
    let type_index = meta.object_types[i] as usize;
    let type_name = &meta.types[type_index];
    let record_size = meta.type_aux[type_index] as usize;
    let count = meta.element_counts[i] as usize;
    let len = container.payload_len(i);
    let fixed = count * record_size;

    assert!(record_size > 0, "resource {i}: zero record size");
    assert!(fixed <= len, "resource {i} ('{}', {type_name}): {count} x {record_size} = {fixed} exceeds {len}");
    payload_bytes += len;
    record_bytes += fixed;

    match type_name.as_str() {
        "Animation" | "creditsTextEntry" | "StdBrush" | "AnimBrush" | "TextList" | "StringTable" => {
            tail_bytes += len - fixed;
        }
        _ => {
            assert_eq!(fixed, len, "resource {i} ('{}', {type_name}) has {} trailing bytes");
        }
    }
}

assert_eq!(payload_bytes, 3_329_791);
assert_eq!(record_bytes, 3_282_998);
assert_eq!(tail_bytes, 46_793);
assert_eq!(record_bytes + tail_bytes, payload_bytes);
```

**`modern_script_output_is_structured_and_complete`**:

```rust
let written = script::extract_container(&container, &tmp, false).expect("extract");
assert_eq!(written, n);

let json = read_to_string(tmp.join("container.json"));
assert!(json.contains("\"resource_count\": 4813"));
assert!(json.contains("\"opaque_bytes\": 0"));
assert!(json.contains("\"type\": \"StringTable\""));
assert!(json.contains("\"record_bytes\": 3282998"));

let types = read_to_string(tmp.join("types.txt"));
assert_eq!(types.lines().filter(|l| !l.starts_with('#') && !l.trim().is_empty()).count(), 89);

let strings = read_to_string(tmp.join("strings.txt"));
assert!(strings.contains("Too many blueprints"));
assert!(strings.contains("Disney Interactive"));
assert!(strings.contains("FineSpray 1"));
assert!(strings.contains("TextList[4810] \"creationsPath\" [2] offset=0x000a \"Creations\""));
assert!(strings.contains("AnimBrush[1393] \"WhippedCream\" [4] offset=0x003c \"Whipped Cream5\""));
assert!(strings.contains("# 632 payload strings decoded"));

let object = read_to_string(tmp.join("objects").join("00000_localizationStrs.txt"));
assert!(object.contains("StringTable entries (130 x 10 bytes"));
assert!(object.contains("Too many blueprints"));
assert!(object.contains("All 3717 payload bytes are represented above."));

let text_list = read_to_string(tmp.join("objects").join("04810_creationsPath.txt"));
assert!(text_list.contains("## String offsets (3 x 4 bytes"));
assert!(text_list.contains("offset=0x000a -> \"Creations\""));
assert!(text_list.contains("All 32 payload bytes are represented above."));

let anim_brush = read_to_string(tmp.join("objects").join("01393_WhippedCream.txt"));
assert!(anim_brush.contains("ref=1393 name_offset=0x003c flags=0x0 -> \"Whipped Cream5\""));
assert!(anim_brush.contains("All 115 payload bytes are represented above."));

assert_eq!(read_dir(tmp.join("objects")).count(), n);
```

These pin: the `{index:05}_{name}.txt` filename template, the object-count line
`# All <len> payload bytes…`, the exact `strings.txt` lines for TextList and
AnimBrush, the `types.txt` 89 data rows, and `# 632 payload strings decoded`.

## I2. `tests/film_roundtrip.rs` (the third game's `DMA.MMF`)

**`original_film_is_the_verbatim_inner_flm`**:

```rust
let count = film::extract_container(&container, &out_dir, true).expect("extract original");
assert_eq!(count, 1, "one film resource");
let extracted_path = out_dir.join("00000_Intro.flm");
let payload = container.payload(0);
assert_eq!(payload.len(), 6_856_628);
assert_eq!(&payload[..2], &[0xF7, 0x89]);
assert_eq!(extracted, payload);
assert_eq!(extracted, &on_disk[start..end]);            // [0x470, 0x68A424)
assert_eq!(container.sizes[0], 26_870_306);
assert_ne!(container.sizes[0] as usize, payload.len());
assert_ne!(container.plain_len(0), payload.len());
let film = film::decode_film(&container, 0).expect("decode film");
assert_eq!(film.chunks.len(), 412);
assert_eq!(film.chunks[0].kind, film::TYPE_FULL_FRAME);
assert_eq!(film.chunks.last().unwrap().kind, film::TYPE_END);
assert_eq!(film.chunks.last().unwrap().offset + 8, payload.len());
assert!(!out_dir.read_dir().unwrap().any(|e| e.unwrap().path().extension().is_some_and(|x| x == "png")),
        "original mode must not write PNGs");
```

**`modern_film_writes_lossless_pngs_and_timeline`**:

```rust
let count = film::extract_container(&container, &out_dir, false).expect("extract modern");
assert_eq!(count, 1);
let film = film::decode_film(&container, 0).expect("decode film");
assert_eq!(film.width, 546);   assert_eq!(film.height, 410);
assert_eq!(film.stride, 548);
assert_eq!(film.bounds, [0, 0, 410, 546]);       // {top, left, bottom, right}
assert_eq!(film.palette_count, 1);
assert_eq!(&film.palette[..4], &[255, 0, 0, 0], "transparent black");
assert_eq!(&film.record[..10], &[0x19, 0x02, 0, 0, 0, 0, 0x01, 0x9a, 0x02, 0x22]);

assert_eq!(film.frames.len(), 139);
assert_eq!(film.unique_frames, 133);
assert_eq!(film.frames[0].start_tick, 0);
assert_eq!(film.frames[1].start_tick, 83);
assert_eq!(film.frames[138].start_tick, 11_454);
assert_eq!(film.duration_ticks(), 11_537);
assert!(film.frames.iter().all(|f| f.duration_tick == 83));
assert_eq!(duplicates, vec![6, 51, 52, 53, 137, 138]);

assert_eq!(sha256_hex(&film.frames[0].indices),   "153192a1d8a2f4ea0ba3285ba29531a687e5749b4dfd238e4891d63dff64c8c8");
assert_eq!(sha256_hex(&film.frames[138].indices), "661f73dda583c23eef4ecd3398bb32987534d1fe0487247d9a23c6002127f202");

assert_eq!(film.audio.len(), 139);
assert_eq!(film.audio.iter().filter(|a| a.sync).count(), 12);
assert_eq!(film.audio.iter().filter(|a| !a.sync).count(), 127);
assert_eq!(film.chunk_kinds[&film::TYPE_AUDIO], 139);
assert_eq!(film.chunk_kinds[&film::TYPE_TIMESTAMP], 138);
assert_eq!(film.chunk_kinds[&film::TYPE_DELTA], 132);
assert_eq!(film.chunk_kinds[&film::TYPE_FULL_FRAME], 1);

assert_eq!(png_count, 139, "one PNG per frame");
let manifest = read_to_string(film_dir.join("manifest.json"));
for needle in ["\"frame_count\": 139", "\"unique_frame_count\": 133",
               "\"duration_ms\": 11537", "\"chunk_count\": 412", "\"chunk_count\": 139",
               "\"sync_blocks\": 12", "\"duplicate\": true"] {
    assert!(manifest.contains(needle), "manifest lacks {needle}");
}
assert_eq!(manifest.matches("\"duplicate\": true").count(), 6);
// … then every PNG is decoded and compared with the palette-expanded frame.
```

## I3. `tests/film_3d.rs` (`DMA3D.MMF`, three films)

```rust
let count = film::extract_container(&container, &out_dir, true).expect("extract original");
assert_eq!(count, 3, "three film resources");
// each file is <index:05>_<name> (the name already ends in .flm), equal to the
// container payload and to the on-disk slice; payload[..2] == [0xF7, 0x89];
// zero PNGs written in original mode.

let measured: &[(&str, [u16; 4], usize, usize, usize)] = &[
    ("introAxDisney.flm",       [0, 0, 600, 800], 800, 600, 800),
    ("introAxImageBuilder.flm", [60, 107, 486, 696], 589, 426, 592),
    ("introAxSplash.flm",       [60, 80, 540, 720], 640, 480, 640),
];
// bounds == the rect; width/height/stride as listed;
// decoded.chunk_kinds[&film::TYPE_FULL_FRAME] == [45usize, 12, 1][index];

let frame_counts = [61usize, 62, 901];
let unique_counts = [16usize, 49, 748];
// decoded.frames.len() == frame_counts[i]; decoded.unique_frames == unique_counts[i];
// every frame.indices.len() == stride * height;
// frames[0].full_chunk.is_some();
// frames with timestamp_chunk && full_chunk == key_frame_count(&decoded) - 1;
// at least one frame has timestamp_chunk && delta_chunk;
// one PNG per frame in <index:05>_<name without .flm>/;
// every PNG decodes to the palette-expanded bytes.
```

and the golden SHA-256 of the raw index buffers:

```text
film 0 frame 0:   83cfc6457821a9e57f2b89ba8252b1f21e69a207303770dfb57ccc549effa021
film 0 frame 60:  83cfc6457821a9e57f2b89ba8252b1f21e69a207303770dfb57ccc549effa021
film 1 frame 0:   53f054252658c5289b5e36a20925df0b707f3543d88300dd55eed474ee03dba7
film 1 frame 61:  679fb0847b1873fc9f21a3ceff76ca7601c72676c92c0229232108c36f78d7ab
film 2 frame 0:   5caa0b76b27100a7b2d75e82899316ba2a7a5278d73debdc62e49154c185802c
film 2 frame 900: 88fa6ec417fbc557be03b66f5a0cf65e2e09638d4a3deed1ede3fa9ce75942fb
```

## I4. `tests/dma3d.rs`

**`the_3d_containers_open_with_their_own_kinds`**:

```rust
assert_eq!(models.kind, ContainerKind::Models);
assert_eq!(models.version, 3);
assert_eq!(models.resource_count, 460);
assert_eq!(models.offsets.len(), 461);
assert_eq!(*models.offsets.last().unwrap() as usize, models.data.len());
assert!(models.offsets.windows(2).all(|w| w[0] <= w[1]));
assert_eq!(models.big_endian, true);
assert_eq!(models.name(0), "createAmbientLight.mod");
assert_eq!(textures.resource_count, 705);
assert_eq!(textures.offsets.len(), 706);
assert_eq!(textures.name(0), "CreationSlotTex.TGA");
```

**`the_metadata_regions_are_measured`** — this is the test that pins
`models_meta_len`/`textures_meta_len`:

```rust
let n = u16::from_be_bytes([data[0x22], data[0x23]]) as usize;         // big-endian!
let first = u32::from_be_bytes([data[0x24], …, data[0x27]]) as usize;
let last = u32::from_be_bytes([data[0x24 + 4*n], …]) as usize;
assert_eq!(last, data.len(), "{name}: the offset table must end at EOF");
let names_end = 0x24 + 4 * (n + 1) + 2;
let region = first - (names_end + 32 * n);
assert_eq!(region, region_of(n), "{name}: metadata region is {region} bytes, expected {want}");
```
with `region_of = models_meta_len` (want 1940 for N=460) and
`region_of = textures_meta_len` (want 4232 for N=705).

**`every_3d_payload_round_trips_byte_exactly`**:

* `--original`: `report.failed.is_empty()`, `report.written == resource_count`,
  and every `entry.path` read equals `container.payload(entry.index)`.
* modern textures: `failed.is_empty()`, `written == 705`, and for every entry the
  PNG's width/height equal `measured.width`/`measured.height`, the byte count
  equals `expected.len()`, and the decoded `rgba` equals `measured.to_rgba()`.
* modern models: `failed.is_empty()`, `written == 460`; for each entry
  `modfile::parse_mesh` fails ⇒ the file bytes equal the payload; otherwise
  `gltf::read_gltf(&entry.path)` succeeds with `version == "2.0"`,
  `indices.len() == mesh.indices.len()`, `positions.len() == mesh.positions.len()`,
  `normals.len() == mesh.normals.len()`, and **every** index, position and normal
  equal to the decoder's. Texcoords: if `back.texcoords.is_empty()` then
  `records.pairs_in_unit_range != Some(records.pair_count)`, else
  `back.texcoords.len() == mesh.vertex_count()` and every component is inside
  `-1.0e-6..=1.000_001`.
  `assert_eq!(meshes, 144, "the mesh-bearing payload count changed");`
  `assert_eq!(meshes + passthrough, container.resource_count as usize);`
* tail: for every mesh payload except those with
  `payload.len() == mesh.tail_offset()`, the file
  `gltf::tail_name(<entry file name>)` equals `&payload[mesh.tail_offset()..]`,
  `tail.len() < payload.len()`, `with_tail == 144`, and
  `assert_eq!(tail_bytes, 590_532, "the undecoded tail size changed from the measured value")`.

**`the_modern_output_is_a_browsable_whole`** (drives `cli::run` with `--types models`):

```rust
assert!(summary.failed.is_empty());
assert_eq!(summary.models, 460 + 705);
let index = tmp.join("index.html");
assert!(index.is_file(), "no index.html was written");
assert!(page.contains("<!DOCTYPE html>"));
assert!(!page.contains("http://") && !page.contains("https://") || page.contains("gltf-viewer"));
assert!(!page.contains("<script"));
let references = page.split('"').filter(|s| s.starts_with("Models/") || s.starts_with("Textures/"));
let mut distinct = references.clone(); distinct.sort(); distinct.dedup();
assert_eq!(distinct.len(), 705 + 144 + 144, "the index does not list every asset");
assert_eq!(distinct.iter().filter(|p| p.ends_with(".png")).count(), 705 + 144);
assert_eq!(distinct.iter().filter(|p| p.ends_with(".gltf")).count(), 144);
for reference in &references { assert!(tmp.join(reference).is_file(), "the index refers to {reference}, which is not on disk"); }
let texture_count = distinct.iter().filter(|p| p.starts_with("Textures/")).count();
assert_eq!(reference_count - before, distinct.len() + texture_count);
assert_eq!(texture_count, 705);
assert_eq!(distinct.len() - texture_count, 288);
// previews: exactly 144 .preview.png and 144 .gltf files, every preview 256x256
// and covering at least one non-transparent pixel.
```

(The relative prefixes `Models/` and `Textures/` come from
`relative_to` stripping `base`, which is why the test's `tmp` is the base and the
output directories are `Models/DMA3D/` and `Textures/DMA3D/`.)

## I5. `tests/dma3d_numbers.rs`

* `the_two_containers_hold_the_measured_resource_counts`: 460 and 705.
* `the_payloads_that_carry_a_mesh_are_counted`:
  `ui == 42` (`createPropUI*`), `ui_named == 64` (contains `"UI"`),
  `meshed == 144`, `meshed_ui == 0`.
* `every_texture_decodes_and_no_palette_entry_is_dead`: for all 705,
  `1 + max(indices) == palette.len()`, `area.is_power_of_two()`, and
  `square + wide + tall == 705` with `wide + tall == 27`.
* `the_undecided_tail_of_the_mesh_payloads_is_sized`:
  `meshed == 144`, `tail == 590_532`.
* `the_tail_header_is_a_measurement_not_a_rule`:
  `"[0, 1, V]" == 94`, `"[0, 0, 0]" == 49`, `"other" == 1`, sum == 144,
  where the key is `Some((0, 1, v))` with `v == vertex_count` ⇒ `"[0, 1, V]"`,
  `Some((0, 0, 0))` ⇒ `"[0, 0, 0]"`, `Some(_)` ⇒ `"other"`, `None` ⇒ `"too short"`.
* `the_measured_orientations_are_reproduced` — 21 named textures with expected
  `(width, height)`:
  `CreationTex.TGA 64x32`, `DuplicateTex.TGA 64x32`, `NewTex.TGA 64x32`,
  `beewings.1/2/3.TGA 32x16`, `createBtUndo.1/2/3.TGA 32x64`,
  `createModMickeyPartArm{,L,R}.TGA 64x128`, `createModMickeyPartHead{,S}.TGA 64x128`,
  `createModMickeyPartLeg{L,R}.TGA 64x128`, `createModMickeyPartTorso.TGA 64x128`,
  `createModMickeyParts.TGA 64x128`, `createModMickeySlider.TGA 64x128`,
  `introAxSplash.TGA 256x128`, `shrBkLeft1.TGA 128x256`; each also
  `!texture.undecidable`.
* `every_rectangle_is_one_to_two`: `rectangles == 27` and for every
  non-`Square` texture `max(w,h) == 2*min(w,h)`.
* `the_undecidable_texture_is_flagged`: `createBtFileIo.TGA` has
  `palette.len() == 1` and `undecidable == true`, and it is the **only** flagged
  texture in all 705.

---

# PART J — Rust-level details the Lean port must reproduce

1. **`f32` Display.** `write_gltf`'s `min`/`max` use `{}` on `f32`, which prints
   `0` for `0.0`, `1` for `1.0`, `-0` for `-0.0`, and `inf`/`-inf`/`NaN` for those.
   A Lean port using `toString` on `Float` will print `0.000000` — the port must
   reproduce Rust's shortest-round-trip formatting for these values.
2. **Integer formatting with width and padding.** `{index:05}` is zero-padded;
   `{v:>3}` is space-padded on the left; `{a:04x}` zero-pads hex; `{a:#06x}`
   means `0x` + zero-padded-to-4 hex.
3. **anyhow chain formatting** (`{e}` versus `{e:#}`) — §2.2. It changes stderr
   text, which several tests read indirectly and which users diff.
4. **`Vec::with_capacity` and `Vec::len` after `break`.** In `parse_mesh`, a failed
   coordinate clears `positions` and `continue`s the candidate scan — the port
   must discard the partial vector, not reset a length counter.
5. **`checked_mul`/`checked_add` returning `None` ⇒ a different error** than the
   range check that follows. Keep both branches.
6. **`count.checked_mul(10)` in `string_table_entries` is redundant with
   `count` from a `u32`** but must stay, because it is the only guard for
   `count` values the container could theoretically supply.
7. **`usize` arithmetic overflow panics are compiled in** (debug builds). The
   port's integer type must be wide enough for
   `2 * count * record_size` on `HD.MMS` (max 4813*48 = 231,024 — trivial), and
   for `sum(duration_tick)` (11,537 — trivial). The only large quantities are
   byte sums, which are asserted against the exact values in §I.
8. **`HashSet<&[u8]>` for `unique_frames`** compares byte slices; the Lean port
   needs value equality on `ByteArray` (and must be careful that `HashSet`
   insertion order never matters — it does not, only the count is used).
9. **`BTreeMap` ordering** appears in three places that are externally visible:
   `chunk_kinds` (numeric u16 order → `manifest.json`'s `chunk_types`),
   `layout_counts` (string order → `container.json`'s `layouts`), and
   `type_counts` is a plain `Vec` (type-index order → `types.txt` and the `types`
   array).
10. **The `.flm` chunk walk is big-endian even for a little-endian container**;
    the `.mod` payload is little-endian even in a big-endian container. Both are
    explicit `true`/`false` in the Rust and must not be derived from
    `container.big_endian`.
11. **`Module` separation.** `dma3d.rs` is the only place that knows the output
    filenames; `modfile`/`texfile`/`anim`/`gltf`/`render` are pure functions of
    bytes. Keep the Lean modules split the same way so the tests can call the
    pure ones directly.

---

# PART K — Where a doc disagrees with the code (the port must follow the code)

1. **`docs/DMA3D.md` §2.1 vs `textures_meta_len`.** The doc now records the
   `Textures` region as `32*N + 4*N + 2*N + 2` = 26,792 bytes (the names table
   plus the metadata region), while the function is `6*N + 2` = 4,232, which is
   what `tests/dma3d.rs::the_metadata_regions_are_measured` asserts
   (`region = offsets[0] - (names_end + 32*N)`). **Port `6*N + 2`.** The doc's
   note "The `Textures` region was measured **wrong** at first, as `6*N + 2`"
   refers to an earlier state of the document; the function and the test still
   measure the region between the names table and the first payload, excluding
   the 32-byte-per-resource names.
2. **`docs/DMA3D.md` §3.2/§4.2 counts vs the parser.** The doc says the
   `[0, 1, V]` header appears in 94 of 144 payloads and mentions `[0, 0, 0]` in
   49 and one other; `tests/dma3d_numbers.rs` pins exactly 94 / 49 / 1 with the
   classifier `Some((0,1,v)) && v == vertex_count`. The parser never requires the
   header. Both agree — keep them.
3. **`src/dma3d/gltf.rs` doc comment vs the code.** The module doc says
   "indices … one-based indices read from the payload minus 1" and "so the writer
   subtracts one". The code **does not subtract one** — the comment above the
   buffer loop says: "The payload's indices are already zero-based … so glTF reads
   them straight through", and `tests/dma3d.rs` asserts `back.indices == mesh.indices`
   for all 144 meshes. **Port the code: write the indices unchanged.**
4. **`docs/DMA3D.md` §4.2 mentions `axis_of`** ("decided by [`axis_of`]"); the
   function is named `decide` in `src/dma3d/texfile.rs`. Same function, stale
   name in the doc.
5. **`src/script.rs` `classify` accepts `StringTable` on tail length alone**
   while `docs/SCRIPTS.md` §4.3 describes the entries; a payload can therefore be
   classified `string-table` and still print `# could not decode StringTable`.
   That is intentional and `HD.MMS` contains none — the port must keep it.
6. **`film.rs`'s `MAX_RECORD_LEN` doc says 26 is "the longest per-film record
   seen"** and the metadata solve tries 16..=26 in steps of 2, so a 22-byte table
   (the second game) is found before 26 and used. The port must iterate the same
   candidate list and **take the first** that satisfies
   `palettes_end == payload_start` — not the largest.
7. **`parse_film_metadata` never reads `container.big_endian`**; every read is
   big-endian by construction (`read_u16_checked`, explicit `from_be_bytes`).
   `docs/FILMS.md` agrees (§2 "4*N u16 big-endian").
8. **`dma3d.rs`'s `extract_gltf` reports a "preview covers no pixels" failure
   while still counting the resource as written.** `tests/dma3d.rs` therefore
   asserts both `report.failed.is_empty()` **and** `written == 460` — with the
   shipped assets no preview is blank, so both hold. The port must not change
   either side of that.

---

# PART L — Implementation checklist for the Lean port

1. Constants and enums — §3 (all of them, with the doc evidence quoted).
2. `Container` accessors and `ScriptMeta` — §1.
3. `script.rs`: `extract_container` → `write_originals` / `write_modern` (§A1–A3),
   the classifiers §A4–A5, the four decoders §A6, and the five writers
   §A7–A10 with their exact templates and §A10.7's hexdump.
4. `film.rs`: `extract_container` (§B1), `decode_film` (§B3) with the event
   assembly, `parse_film_metadata`'s eight tables and length solve (§B4),
   `parse_chunks` (§B5), the frame/delta decoders (§B7–B10), and
   `manifest_json`'s exact key order (§B12).
5. `dma3d.rs`: the two region lengths (§C1), the three extract paths (§C2–C5),
   `texcoords_of` (§C6), `texture_names_in` (§C7), `write_index` + `list_dir`
   (§C8) with the verbatim `INDEX_HEAD`/`INDEX_TAIL`.
6. `texfile.rs`: `palette_size`, `split_area`, `wide_tall` (§D2–D3), `decide`
   with its veto order (§D4), `vertical_delta`, `background`, `shape` (§D5–D6).
7. `modfile.rs`: the signature scan and its four rejections (§E1).
8. `anim.rs`: `locate` and `dump` with the four views (§F1, F3).
9. `gltf.rs`: `base64`/`base64_decode` (§G1), the JSON reader (§G2),
   `read_gltf` (§G4), `write_gltf`'s buffer and section order (§G8) — compare
   against §G8.1's byte-exact document.
10. `render.rs`: the rasteriser (§H1) — compare the preview byte-for-byte with
    the Rust output on the synthetic mesh before trusting it.
11. No subprocess is needed anywhere in this port; the PNG contract is §2.4.

---

# Appendix M — complete emitted examples

## M1. `container.json` — the real `HD.MMS` head and tail, byte for byte

Abbreviated with `…` in exactly two places (the middle 86 `types` rows and the
middle 4811 `objects` rows); every other byte is what the writer emits. LF line
endings, and the file ends with a newline.

```json
{
  "format": "MMFW Scripts",
  "source": "/RawAssets/HD.MMS",
  "endian": "little",
  "container_version": 3,
  "resource_count": 4813,
  "script_metadata": {
    "version": 11,
    "unknown_fields": [65535, 65535, 2490],
    "type_count": 89,
    "offset": 19292,
    "end": 147460,
    "bytes": 128168
  },
  "coverage": {
    "payload_bytes": 3329791,
    "record_bytes": 3282998,
    "tail_bytes": 46793,
    "opaque_bytes": 0,
    "layouts": {
      "anim-brush": 12,
      "animation": 2783,
      "brush-names": 46,
      "credits-text": 155,
      "records": 1813,
      "string-table": 1,
      "text-list": 3
    }
  },
  "types": [
    {"index": 0, "name": "Animation", "record_size": 10, "object_count": 2783},
    {"index": 1, "name": "DMAPicture", "record_size": 10, "object_count": 7},
    {"index": 42, "name": "StringTable", "record_size": 10, "object_count": 1},
    …
    {"index": 88, "name": "DMAAnimatedButton", "record_size": 8, "object_count": 1}
  ],
  "objects": [
    {"index": 0, "name": "localizationStrs", "type_index": 42, "type": "StringTable", "record_size": 10, "elements": 130, "payload_offset": 147460, "payload_size": 3717, "layout": "string-table", "file": "objects/00000_localizationStrs.txt"},
    {"index": 1384, "name": "FineSpray", "type_index": 53, "type": "StdBrush", "record_size": 48, "elements": 5, "payload_offset": 293616, "payload_size": 315, "layout": "brush-names", "file": "objects/01384_FineSpray.txt"},
    …
    {"index": 4810, "name": "creationsPath", "type_index": 9, "type": "TextList", "record_size": 4, "elements": 3, "payload_offset": 1474, "payload_size": 32, "layout": "text-list", "file": "objects/04810_creationsPath.txt"}
  ]
}
```

Points to check against the code:

* two-space indent at the top two levels, four inside `objects` and `types`;
* `layouts` keys in `BTreeMap` (string) order — `anim-brush` sorts before
  `animation` because `-` (0x2d) is below `a` (0x61);
* one `types` row per class with `{"index": …, "name": …, "record_size": …,
  "object_count": …}` and a trailing comma on every row but the last;
* `objects` rows carry the **raw** (unescaped in this example, all names here are
  plain ASCII) object name, the type index and type name, the record size, the
  element count, `payload_offset` = `container.offsets[index]` (decimal), and the
  `file` path with the *sanitized* name — which differs from the raw name
  whenever the name holds a byte outside `[A-Za-z0-9._-]` (those become `_`).
* measured in this example: the header fields, the `coverage` block, every
  `types` row shown, and the first `objects` row (`payload_offset` 147460 is
  `0x24004`, the first payload; `payload_size` 3717 is 130×10 + 2417). The two
  later `objects` rows have measured `record_size`/`elements`/`payload_size`
  (`docs/SCRIPTS.md` §4.1: `FineSpray` has 5 records of 48 bytes plus five names;
  `creationsPath` is 32 bytes) but **placeholder** `payload_offset` values — the
  real ones come from the container's own offset table, and what the rows are
  here to show is the field order.

## M2. `strings.txt` — the head and the three pinned lines

```text
# Strings decoded from /RawAssets/HD.MMS
# Escapes: \" \\ \0 \n \r \t, other bytes as \xNN

[type names]
type[  0] "Animation"
type[  1] "DMAPicture"
…
type[ 88] "DMAAnimatedButton"

[object names]
object[0000] "localizationStrs"
object[0001] "ISbigBALa"
…
object[4812] "appDataPath"

[payload strings]
StringTable[0000] "localizationStrs" id=0 offset=0x0000 "Too many blueprints!  Render or remove some existing ones first."
creditsTextEntry[0140] "firstBlank" ""
StdBrush[1384] "FineSpray" [0] "FineSpray 1"
StdBrush[1384] "FineSpray" [1] "FineSpray 2"
TextList[4810] "creationsPath" [0] offset=0x0000 ".."
TextList[4810] "creationsPath" [1] offset=0x0003 "Common"
TextList[4810] "creationsPath" [2] offset=0x000a "Creations"
AnimBrush[1393] "WhippedCream" [0] offset=0x0000 "Whipped Cream1"
…

# 632 payload strings decoded
```

`type[  0]` is `{index:3}` (right-aligned in 3); `object[0000]` is `{index:04}`
(zero-padded to 4). The three lines the test pins verbatim are the
`TextList[4810]`/`AnimBrush[1393]` forms and the final count. The `[payload
strings]` block is emitted in object order, one pass, and the StringTable branch
skips offsets `0xfffd..=0xffff` entirely.

## M3. An object dump header, for a `Records`-layout object

`HD.MMS` object 1 `ISbigBALa` (type `Animation`, 197 elements × 10 bytes,
payload 1,970 bytes) — the header plus the first and last record lines:

```text
# Object 1: ISbigBALa
# Type: Animation (index 0)
# Elements: 197 x 10 bytes = 1970
# Payload: 1970 bytes at container offset 0x<real offset, {:x}>
# Layout: animation
## Fixed records (197 x 10 bytes)
# [index] raw bytes | decoded fields
  [   0] 00 00 00 00 00 00 00 00 00 00 | op 0 args 0 0 0 0
  [   1] 01 00 01 00 00 00 00 00 00 00 | op 1 args 1 0 0 0
  [   2] 02 00 00 00 00 00 00 00 00 00 | op 2 args 0 0 0 0
  [   3] 03 00 00 00 00 00 00 00 00 00 | op 3 args 0 0 0 0
  [   4] 05 00 01 00 53 00 00 00 00 00 | op 5 args 1 83 0 0
  [   5] 06 00 00 00 01 00 14 00 f5 ff | op 6 args 0 1 20 65525
  …
  [ 196] 0e 00 00 00 00 00 00 00 00 00 | op 14 args 0 0 0 0
## Tail: object name (1 NUL-terminated string(s), 8 bytes)
  [0] +0x0000 len   8 "ISbigBALa"
# All 1978 payload bytes are represented above.
```

(Records 6..196 elided; the tail length, the `+0x` prefix and `len   8`'s
right-alignment in 3 are exact. The last line's number is the payload length —
1,970 fixed + 8 tail.)

## M4. `manifest.json` — a complete two-frame synthetic film, byte for byte

A film of width 2, height 1, stride 4 (frame 4 bytes), one palette, two frames
(the first from a key-frame chunk, the second from a delta chunk with two
rects), one non-sync audio chunk. Every line is exactly what `manifest_json`
emits, including the object-key order inside `source` and the audio chunk lines.

```json
{
  "container": "/RawAssets/DMA.MMF",
  "film_index": 0,
  "name": "Intro.flm",
  "width": 2,
  "height": 1,
  "stride": 4,
  "frame_size_bytes": 4,
  "pixel_format": "8bpp palette indices; RGBA = palette[transparency, red, green, blue], alpha = 255 - transparency",
  "record_hex": "190200000000019a0222",
  "record_bounds": [0, 0, 1, 2],
  "record_note": "bytes +2..+10 are u16 BE {top, left, bottom, right}; the rest is preserved verbatim",
  "palette_count": 1,
  "palette_hex": "ff000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
  "frame_count": 2,
  "unique_frame_count": 2,
  "timebase": "ticks treated as milliseconds (83-tick spacing = 12.05 fps)",
  "duration_ms": 166,
  "chunk_count": 4,
  "chunk_types": {"2": 1, "6": 1, "10": 1, "11": 1},
  "offset_note": "all chunk offsets are relative to the start of the inner .flm payload",
  "frames": [
    {
      "index": 0,
      "start_ms": 0,
      "duration_ms": 83,
      "duplicate": false,
      "png": "frame_000.png",
      "source": {"kind": "full", "chunk_offset": 1234},
      "delta_rects": []
    },
    {
      "index": 1,
      "start_ms": 83,
      "duration_ms": 83,
      "duplicate": false,
      "png": "frame_001.png",
      "source": {"kind": "delta", "timestamp_chunk_offset": 1500, "delta_chunk_offset": 2000},
      "delta_rects": [[0,0,10,10], [0,0,20,20]]
    }
  ],
  "audio": {
    "chunk_count": 1,
    "data_blocks": 1,
    "sync_blocks": 0,
    "note": "12-byte header [data_len, zero, block_index], then PCM bytes; sync blocks use block_index 4294967295. Audio is preserved verbatim by --original.",
    "chunks": [
      {"chunk_offset": 100, "chunk_len": 7332, "data_offset": 120, "data_len": 7320, "index": 0, "sync": false}
    ]
  }
}
```

The `palette_hex` line above is the full 1,024-byte palette rendered as 2048
lowercase hex characters: entry 0 is `ff000000` (transparency 255, R 0, G 0, B 0)
and entries 1..255 are `00000000`. The Rust writes `hex(&film.palette)` with no
separators and no line breaks, so the port must emit the same single long line.

## M5. `.gltf.anim.txt` — a complete records document, byte for byte

`anim::dump("createPropBucket.mod", payload, mesh, records)` with
`mesh.index_count_offset = 0xa3`, 498 indices, 151 vertices, `tail_offset =
0x1773`, `tail_header = Some((0, 1, 151))`, 151 float pairs of which 119 are
inside `0..1`, and a 17-byte remainder. The pair values are the measured ones
(`docs/DMA3D.md` §3.2 quotes the same numbers for `createPropBucket.mod`); the
remainder bytes are a synthetic illustration whose four views are computed
below, so every line shows exactly what the writer emits:

```text
# createPropBucket.mod
# payload 7240 bytes; mesh 498 indices / 151 vertices; mesh chain at 0xa3
# the mesh decoder stops at 0x1773; everything below is what it left

[header]
  u32 0, u32 1, u32 151   the [0, 1, V] shape, the third word = the vertex count

[pairs]  151 float pairs, 119 of them wholly inside 0..1
  all inside 0..1 reads as texture placement; none inside 0..1 means the pairs are something else. Which payloads are which is not measured
     0: (0.35300, 0.02480)
     1: (0.35320, 0.02500)
     2: (0.35340, 0.02520)
     3: (0.35360, 0.02540)
     4: (0.35380, 0.02560)
     5: (0.35400, 0.02580)
     6: (0.35420, 0.02600)
     7: (0.35440, 0.02620)
  ... 143 more, not printed

[records]  17 bytes at 0x1c37, undecoded
  The record layout is not measured: not the record size, not the keyframes per record, and not which node each track drives. These are the game's own bytes, in the game's own order, rendered four ways so a candidate layout can be checked against them.
  0x001c37  00 00 80 3f 00 00 00 40 00 00 40 40 00 00 80 40
  0x001c47  10

  as u32 (little-endian), the first 4 of them:
  1065353216 1073741824 1082130432 1084227584

  as f32 (little-endian):
  1.00000 2.00000 3.00000 4.00000

  as u16 (little-endian):
  0 32768 0 16384 0 16448 0 16512

[track names]
  none. Only six payloads name theirs, and none of those carries a mesh
```

The arithmetic that produces those numbers, so the port can check itself:

```text
mesh.tail_offset()          = 0x1773                     (= 6003)
header                      = 12 bytes                   -> 0x1773 .. 0x177f
151 float pairs             = 8 * 151 = 1208 bytes       -> 0x177f .. 0x1c37
remainder_offset()          = 0x1773 + 12 + 1208 = 0x1c37 (= 7223)
remainder.len()             = 7240 - 7223 = 17
words                       = 17 / 4 = 4                 -> 4 u32 values, 4 f32 values
u16 groups                  = 17 / 2 = 8                 -> 8 u16 values
```

Points to check against the code:

* the pair lines are `{k:4}` right-aligned (four wide), so `   0:`, `   7:`;
* `[records]`'s address is `{:#06x}` — `0x` followed by the offset in hex,
  zero-padded so the **number** part is at least 4 digits: `0x1c37` and
  `0x1c47` above (and `0xa3` would print as `0x00a3`, which is why the
  `# payload … mesh chain at {:#x}` header line reads `0xa3` — that one uses the
  unpadded `{:#x}`, and the two formats must not be swapped);
* the three "views" each print only whole 2- or 4-byte groups
  (`chunks_exact`), 24 u32, 24 f32 and 32 u16 values respectively, and the
  u32 line's text says "the first <words> of them" where
  `words = remainder.len() / 4`;
* a payload that carries a track name table prints one
  `  <name padded to 24> at <offset:#x>` line per name instead of the `none.`
  sentence — `createPropGuitar.mod` ends with `key01`…`key06`.
