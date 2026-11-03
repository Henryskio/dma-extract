# Porting specification — `src/sound.rs` and `src/v1/*` → Lean 4

This document is a **byte-exact porting specification**. It is written so that the
Lean modules can be implemented **without reading the Rust**. Every public
function is given with its exact name, signature and return value; every field the
code touches is given with its exact byte offset and endianness; every constant
is named with its value and the doc evidence for it; every error message is
verbatim; every subprocess invocation is given as exact argv + stdio wiring;
every output file is given with its exact name template and exact bytes.

Nothing here is a summary of the algorithm — the arithmetic and the bit
twiddling are quoted from the Rust where they are subtle.

**Sources of truth, in order** (when they disagree, the higher one wins):

1. The Rust code in `src/sound.rs`, `src/v1/*.rs`, `src/container.rs`.
2. The tests in `tests/` (they pin the code, including where a doc is stale).
3. `docs/SOUNDS.md` and `docs/GAME1.md`.
4. The module-level doc comments in the Rust files.

Section 16 lists every place where a doc comment / `docs/*.md` claim disagrees
with the code, and says which the port must follow.

---

## 1. Scope and module map

| Rust file | Lean module (suggested) | Public API to port |
|---|---|---|
| `src/sound.rs` | `MmfExtract/Sound.lean` | `DPCM_TABLE`, `DPCM_SILENCE`, `Encoding`, `SoundFormat`, `DecodedSound`, `ExtractedSound`, `ExtractReport`, `format_of`, `decode`, `decode_dpcm`, `extract_container` |
| `src/v1/mod.rs` | `MmfExtract/V1.lean` | `V1Metadata` (re-export), `uncompressed_len`, `palette_of`, `palette_bank`, `palette_count`, `palette_bytes` |
| `src/v1/lzss.rs` | `MmfExtract/V1/Lzss.lean` | `decompress` |
| `src/v1/metadata.rs` | `MmfExtract/V1/Metadata.lean` | `V1Metadata`, `parse` |
| `src/v1/picture.rs` | `MmfExtract/V1/Picture.lean` | `payload`, `decode`, `pixel_format` |
| `src/v1/blobs.rs` | `MmfExtract/V1/Blobs.lean` | `STAMP`, `OBJECT_BYTES`, `RECORD_BYTES`, `OBJECT_COUNT_OFFSET`, `OBJECT_BYTES_OFFSET`, `OBJECTS_OFFSET`, `RECORD_BYTES_FIELD`, `VERSION_107`, `STAGE_OFFSET`, `OBJECT_KIND_MIN`, `OBJECT_KIND_MAX`, `Record`, `Object`, `Blob`, `read_u32` (private), `extract_container` |
| `src/v1/scripts.rs` | `MmfExtract/V1/Scripts.lean` | `V1_SCRIPT_VERSION`, `V1Scripts`, `parse`, `extract_container` |

Dependencies the port may assume are already implemented or specified elsewhere
(`PORT-PICTURES.md` etc.):

* `Container` parser (all fields, `payload`, `payload_len`, `plain_len`, `name`,
  `flags_of`, `attr_of`, `dimension`, `kind_of`) — specified in §2 below.
* `MmfExtract.Bytes` helpers: `byteAt`, `u16At`, `u32At`, `takeU16`, `takeU32`,
  `takeU16BE`, `slice` (already exist in `lean/src/MmfExtract/Bytes.lean`).
* `picture::{decode_bytes, pixel_format, PALETTE_BYTES}` — the port must call
  them with the signatures given in §8.2. They need not be reimplemented for
  this port.
* `cli::sanitize` — reproduced verbatim in §3.7 (sound's filename stem depends on
  it).

---

## 2. Struct inventory

### 2.1 `Container` fields, with the byte each one is read from

All multi-byte integers are read in the container's own byte order: **big-endian**
when the two bytes at `0x10` are `"MM"`, **little-endian** when they are `"II"`.
`read_u16(d, off, be)` = `u16::from_be_bytes` if `be` else `from_le_bytes`; the
same for `read_u32`. `big_endian` is `true` for `"MM"`, `false` for `"II"`; any
other marker is an error (`"{}: unknown endian marker {:?}"`).

| Rust field | Type | Where the bytes come from (sounds) |
|---|---|---|
| `path` | `PathBuf` | the file path |
| `data` | `Vec<u8>` | the whole file |
| `kind` | `ContainerKind` | `String::from_utf8_lossy(&data[5..16]).trim_end_matches('\0')` → `"Sounds"` ⇒ `Sounds` |
| `big_endian` | `bool` | `data[0x10..0x12] == b"MM"` |
| `version` | `u16` | **u16 at `0x12`**, container order |
| `file_id` | `[u8; 6]` | `data[0x14..0x1A]` (raw copy) |
| `timestamp` | `f64` | `data[0x1A..0x22]`, container order |
| `resource_count` | `u16` | **v1: u16 at `0x1E`; v2/v3: u16 at `0x22`** |
| `offsets` | `Vec<u32>` | `N+1` u32s at `0x20` (v1) or `0x24` (v2/v3); resource `i` spans `[off[i], off[i+1])`; last entry == file length |
| `names` | `Vec<String>` | v2/v3 sounds: 32-byte NUL-padded fields from `names_base`; v1 sounds: same; v1 scripts: `N` empty strings |
| `attr` | `Vec<u8>` | see §2.2 / §2.3 |
| `flags` | `Vec<u8>` | **empty for version-1 containers of every kind** |
| `dimensions` | `Vec<Dimension>` | v2/v3 sounds: read but all-zero; v1 sounds: empty |
| `sizes` | `Vec<u32>` | v2/v3: the decoded-size table; **v1: `vec![0; N]`** |
| `extra_u32` | `Vec<u32>` | `4*N` u32s right after the names table |
| `kinds` | `Vec<u8>` | **empty for sounds and films** |
| `names_end` | `usize` | end of the names table (v1 scripts: `payload_start`) |
| `palettes` | `Vec<Vec<u8>>` | **empty for every sound container, both versions** |
| `uncompressed_sizes` | `Vec<u32>` | **empty for sounds** (pictures only) |
| `script` | `Option<ScriptMeta>` | `None` for sounds |

`Dimension { width: u16, height: u16 }`.

Accessors (all `get(i).copied().unwrap_or(default)` — **never panic on a short
array**):

```rust
pub fn payload(&self, i: usize) -> &[u8] {          // panics if i+1 >= offsets.len()
    let s = self.offsets[i] as usize;
    let e = self.offsets[i + 1] as usize;
    &self.data[s..e]
}
pub fn payload_len(&self, i: usize) -> usize { (self.offsets[i + 1] - self.offsets[i]) as usize }
pub fn name(&self, i: usize) -> &str { self.names.get(i).map(|s| s.as_str()).unwrap_or("") }
pub fn dimension(&self, i: usize) -> Dimension { self.dimensions.get(i).copied().unwrap_or_default() }
pub fn flags_of(&self, i: usize) -> u8 { self.flags.get(i).copied().unwrap_or(0) }
pub fn attr_of(&self, i: usize) -> u8 { self.attr.get(i).copied().unwrap_or(0) }
pub fn plain_len(&self, i: usize) -> usize {
    let recorded = self.sizes.get(i).copied().unwrap_or(0);
    if recorded == 0 { self.payload_len(i) } else { recorded as usize }
}
pub fn kind_of(&self, i: usize) -> u8 { self.kinds.get(i).copied().unwrap_or(0) }
```

Names are read as: take the 32-byte field, cut at the first `0x00` (or the whole
field if there is none), `String::from_utf8_lossy`.

### 2.2 A version-2/3 **Sounds** container byte-for-byte (the shared format)

`N = resource_count`, `names_base = 0x24 + 4*(N+1) + 2`, `names_end = names_base + 32*N`,
`payload_start = offsets[0]`. Every read below is in the container's byte order
and every offset is relative to the start of the file:

| Field | Bytes | Endianness | Doc evidence |
|---|---|---|---|
| `extra_u32[i]` (Mac timestamps) | u32 at `names_end + 4*i`, `4*N` total | container order | SOUNDS.md §2 "u32 array: Macintosh timestamps (seconds since 1904-01-01)" |
| (2-byte word) | u16 at `names_end + 4*N`, always `0x0008` | container order | SOUNDS.md §2 "u16 word, `0x0008` in both shipped files; purpose unidentified" |
| `attr[i]` (format byte `C`) | u8 at `names_end + 4*N + 2 + i` | — | SOUNDS.md §2 "audio-format byte array (`C`)" at `4*N + 2` |
| `flags[i]` (encoding byte `E`) | u8 at `names_end + 5*N + 2 + i` | — | SOUNDS.md §2 "flags/encoding byte array (`E`)" at `5*N + 2` |
| `dimensions[i]` | u16 height at `names_end + 6*N + 2 + 4*i`, u16 width at `+2` | container order | all zero for sounds |
| `sizes[i]` (decoded byte count) | u32 at `names_end + 10*N + 2 + 4*i` | container order | SOUNDS.md §2 "decoded size in bytes" at `10*N+2` |
| first payload | `names_end + 14*N + 2` == `offsets[0]` | — | SOUNDS.md §2: metadata is `14*N + 2` bytes |

Measured, from SOUNDS.md §2: `HD.MMA` N=329 → 4608 bytes of metadata, first
payload `0x406e`; `app.MMA` N=354 → 4958 bytes, first payload `0x4550`.

### 2.3 A version-1 **Sounds** container byte-for-byte (the first game)

`N = u16 at 0x1E`, `names_base = 0x20 + 4*(N+1) + 2`, `names_end = names_base + 32*N`,
`payload_start = offsets[0]`, offsets table at `0x20 + 4*i` for `i in 0..=N`.

| Field | Bytes | Endianness | Doc evidence |
|---|---|---|---|
| `resource_count` | u16 at `0x1E` | container order | GAME1.md §2.1: `HD.SND` 66, `mpApp.SND` 152, `pack.SND` 234 |
| `offsets[i]` | u32 at `0x20 + 4*i` | container order | GAME1.md §2: "`u32@(0x20 + 4*N)` is the EOF marker" |
| `extra_u32[i]` | u32 at `names_end + 4*i` | container order | GAME1.md §3: "`4*N` array decodes as **Mac timestamps**… The `N` byte array is the format byte (`attr`)" |
| `attr[i]` (format byte) | u8 at `names_end + 4*N + i` | — | GAME1.md §3: **Sounds are `5*N + 2` bytes** — the `4*N` array, one `N` byte array and 2 bytes |
| 2 bytes padding | `names_end + 5*N .. +5*N+2` | — | must land exactly on `offsets[0]` |
| `flags` | **empty** | — | GAME1.md §3 lists only `attr` for sounds; `flags_of` therefore returns `0` |
| `sizes` | **all zeros** | — | `container.rs`: "Version-1 containers have no decoded-size table"; so `plain_len(i) == payload_len(i)` |

**Consequence the port must reproduce exactly:** because `flags` is empty,
`flags_of(i) & 3 == 0` for *every* first-game sound, so every version-1 sound
takes the **stored-PCM** branch of `decode` and never the DPCM branch. The
`E & 3` codec selector exists only in versions 2/3.

### 2.4 `Encoding`, `SoundFormat`, `DecodedSound`, `ExtractedSound`, `ExtractReport`

```rust
pub enum Encoding { Pcm8, Pcm16, Dpcm16 }        // PartialEq, Eq, Copy
impl Encoding { pub fn bits_per_sample(self) -> u16 { Pcm8 => 8, Pcm16 | Dpcm16 => 16 } }

pub struct SoundFormat { pub sample_rate: u32, pub channels: u16, pub bits_per_sample: u16 }  // PartialEq, Eq, Copy

pub struct DecodedSound {
    pub format: SoundFormat,
    pub encoding: Encoding,
    /// Interleaved little-endian PCM. 8-bit samples are unsigned; 16-bit samples
    /// are signed. This is exactly the byte stream written by `--original` and
    /// the sample data placed in the modern WAV file.
    pub pcm: Vec<u8>,
}

pub struct ExtractedSound {            // Clone, Debug
    pub index: usize, pub name: String, pub encoding: Encoding,
    pub format: SoundFormat, pub path: PathBuf, pub bytes: usize,
}

pub struct ExtractReport {             // Default, Debug
    pub written: usize,
    pub failed: Vec<(usize, String, String)>,   // (index, name, error display string)
    pub resources: Vec<ExtractedSound>,
}
```

### 2.5 V1 structs

```rust
#[derive(Debug, Default)]
pub struct V1Metadata {
    pub attr: Vec<u8>,               // sounds only; empty for the other kinds
    pub flags: Vec<u8>,              // pictures only
    pub kinds: Vec<u8>,              // pictures only
    pub dimensions: Vec<Dimension>,  // pictures only, {height, width} pairs
    pub uncompressed_sizes: Vec<u32>,// pictures only; 0 = stored as-is
    pub palettes: Vec<Vec<u8>>,      // pictures only; count * 1024-byte palettes
}

#[derive(Debug, Default)]
pub struct V1Scripts {
    pub version: u16,        // always 4 in shipped files
    pub header_end: usize,   // table_end + 2
    pub payload_start: usize,// offsets[0]
    pub names: Vec<String>,  // per object, from the payload's trailing string; "" if none
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Record { pub a: u16, pub b: u16, pub event: u32 }   // all big-endian on the wire

#[derive(Debug, Clone)]
pub struct Object { pub kind: u32, pub parameter: u32, pub record_count: u32, pub records: Vec<Record> }

#[derive(Debug, Clone)]
pub struct Blob { pub object_count: u32, pub object_bytes: u32, pub record_bytes: u32, pub objects: Vec<Object> }
```

---

## 3. Named constants (every one, with its source evidence)

### 3.1 `src/sound.rs`

| Name | Value | Quoted evidence |
|---|---|---|
| `DPCM_TABLE` | 256 `i16` values, see §3.2 | module doc: "driven by a 256-entry `i16` table at virtual address `0x532778` / file offset `0x132778`"; SOUNDS.md §4: "The 256-entry `i16` lookup table lives at virtual address `0x532778` / file offset `0x132778` in `DMA4.exe`" |
| `DPCM_SILENCE` | `0xF8` (`pub const`) | SOUNDS.md §4: "`0xf8`: zero, silence"; module doc "The code that represents a zero step (digital silence)" |
| `MEASURE_SAMPLES` | `20 * 22_050` = **441000** | doc: "How much payload to measure: 20 seconds' worth of samples." |
| `RAIL_LEVEL` | `0.99` (`f64`) | doc: "A sample within this of full scale counts as sitting on the rail." GAME1.md §5.2: "The correct reading puts 0.00-0.05 % of samples on the rails" |
| `RAIL_MARGIN` | `0.005` (`f64`) | doc: "How much more of the payload must sit on the rails at the declared depth than at the other one before the depth is overridden." GAME1.md §5.2: mis-readings sit at 16-51 % / 27-37 % vs 0.00-0.05 % correct |
| `FLAC_COMPRESSION` | `"-8"` | doc: "8, the highest… The level is only spelled `-N`… a bare `8` is read as a second input file"; SOUNDS.md §6.2 |
| `FLAC_TOOL` | `"flac"` | doc: "The external encoder, documented as a dependency."; SOUNDS.md §6.1: "`apt install flac` / `brew install flac`" |

Derived values used inline (all from SOUNDS.md §2 / GAME1.md §5.1):

* sample rates: `attr & 3 == 0 → 11_025`, `== 1 → 22_050`, `2` or `3 → 44_100`.
* channels: `attr & 8 != 0 → 2` else `1`.
* declared bits: `attr & 4 != 0 → 16` else `8`.
* differential silence exclusion: `(code & 0x7f) > 0x2f` and `code != 0xf8`.

### 3.2 `DPCM_TABLE` — all 256 entries, verbatim (index = code)

Copy this table into the Lean port character for character. Entries 0..47 and
128..175 are absolute step sizes; the remaining non-silence codes are added to
the previous sample of the same channel.

```
idx  0.. 15:     1,     6,    19,    41,     73,    120,    180,    258,    353,    468,    603,    761,    942,   1149,   1381,   1641
idx 16.. 31:  1929,  2247,  2596,  2977,   3391,   3839,   4323,   4844,   5401,   5998,   6633,   7310,   8028,   8788,   9593,  10441
idx 32.. 47: 11335, 12276, 13264, 14300,  15385,  16520,  17707,  18945,  20236,  21581,  22981,  24435,  25946,  27515,  29141,  30826
idx 48.. 63:  2048,  2304,  2560,  2816,   3072,   3328,   3584,   3840,   1024,   1152,   1280,   1408,   1536,   1664,   1792,   1920
idx 64.. 79:   512,   576,   640,   704,    768,    832,    896,    960,    256,    288,    320,    352,    384,    416,    448,    480
idx 80.. 95:   128,   144,   160,   176,    192,    208,    224,    240,     64,     72,     80,     88,     96,    104,    112,    120
idx 96..111:    32,    36,    40,    44,     48,     52,     56,     60,     16,     18,     20,     22,     24,     26,     28,     30
idx112..127:     8,     9,    10,    11,     12,     13,     14,     15,      0,      1,      2,      3,      4,      5,      6,      7
idx128..143:    -1,    -6,   -19,   -41,    -73,   -120,   -180,   -258,   -353,   -468,   -603,   -761,   -942,  -1149,  -1381,  -1641
idx144..159: -1929, -2247, -2596, -2977,  -3391,  -3839,  -4323,  -4844,  -5401,  -5998,  -6633,  -7310,  -8028,  -8788,  -9593, -10441
idx160..175:-11335,-12276,-13264,-14300, -15385, -16520, -17707, -18945, -20236, -21581, -22981, -24435, -25946, -27515, -29141, -30826
idx176..191: -2048, -2304, -2560, -2816,  -3072,  -3328,  -3584,  -3840,  -1024,  -1152,  -1280,  -1408,  -1536,  -1664,  -1792,  -1920
idx192..207:  -512,  -576,  -640,  -704,   -768,   -832,   -896,   -960,   -256,   -288,   -320,   -352,   -384,   -416,   -448,   -480
idx208..223:  -128,  -144,  -160,  -176,   -192,   -208,   -224,   -240,    -64,    -72,    -80,    -88,    -96,   -104,   -112,   -120
idx224..239:   -32,   -36,   -40,   -44,    -48,    -52,    -56,    -60,    -16,    -18,    -20,    -22,    -24,    -26,    -28,    -30
idx240..255:    -8,    -9,   -10,   -11,    -12,    -13,    -14,    -15,      0,     -1,     -2,     -3,     -4,     -5,     -6,     -7
```

(The same table is printed in `docs/SOUNDS.md` §4 and must match it byte for
byte; index 248 = `0xf8` = `0`.)

### 3.3 `src/v1/lzss.rs`

| Name | Value | Quoted evidence |
|---|---|---|
| `V1_WINDOW` | **1024** | doc: "Window size of the version-1 LZSS, and the initial write cursor. `0x42e104` loads `eax` with `0x3be` (958) and every later update masks it with `0x3ff`"; GAME1.md §4.3: "**The window.** 1024 bytes. `0x42e0f7` counts to `0x3be` (958) zeroing slots" |
| `V1_WINDOW_START` | **958** (`0x3be`) | same quote: "starts the write cursor at 958 — it zeroes exactly the slots the cursor has not reached yet" |

Token layout (GAME1.md §4.3, quoted):

```text
u8 control    one bit per token, least significant bit first, eight tokens per
              control byte. bit 1 = a literal, bit 0 = a match.
literal:      the next input byte, verbatim.
match:        the next two input bytes, big-endian:
                 distance = word & 0x3FF        (10 bits)
                 length   = (byte0 >> 2) + 3    (3..=66, from the other 6 bits)
```

and: "Note *which* bits hold which field: `length = (byte0 >> 2) + 3` puts the
length in the word's **upper** six bits, so the encoded word is
`((length - 3) << 10) | distance`. Reading it as `((length - 3) << 2) | distance`
collides the length into the distance and silently invents matches."

Also: "A match's `distance` is a **window slot index**, not a count of bytes
back. With the cursor starting at 958, `n` bytes back is `(958 + p - n) mod 1024`
in the encoded word." and "The decoder stops as soon as it has produced the
declared number of bytes; any remaining input is padding."

### 3.4 `src/v1/metadata.rs`

| Name | Value | Quoted evidence |
|---|---|---|
| `PALETTE_BYTES` (imported from `picture`) | **1024** | `picture.rs`: "Number of bytes in a 256-entry palette"; GAME1.md §4.1: "A u16 count is followed by `count` 1024-byte palettes" |
| picture marker | **7** (`0x0007`) | GAME1.md §3: "`2 bytes marker, always 0x0007`"; code comment: "Every shipped picture container reads 7 here, and `0x4183`'s reader requires it" |

### 3.5 `src/v1/blobs.rs`

| Name | Value | Quoted evidence |
|---|---|---|
| `STAMP` | `b"\x00\x0cgoofy 2.6.3\x00"` (14 bytes) | doc: "The 14-byte stamp that opens every payload: `u16 0x000c` then the string"; GAME1.md §6.1: "`u16 0x000c` the stamp's length prefix; `"goofy 2.6.3\0"` (12 bytes)" |
| `OBJECT_BYTES` | **12** | doc: "Measured as `object_bytes == count * 12` in all 305 payloads, and the iterator `FUN_004b5ec0` reads exactly that many" |
| `RECORD_BYTES` | **8** | doc: "the 8 bytes `FUN_004b5f34` advances its cursor by per record" |
| `OBJECT_COUNT_OFFSET` | **14** | GAME1.md §6.1: "`14 u32 object_count`" |
| `OBJECT_BYTES_OFFSET` | **18** | GAME1.md §6.1: "`18 u32 object_bytes == object_count * 12`" |
| `OBJECTS_OFFSET` | **22** | doc: "Where the object headers start, just past the stamp and the two `u32`s" |
| `RECORD_BYTES_FIELD` | **4** | doc: "the header parser advances its cursor past it **without reading it** (`add dword [ebx + 0x20027], 4` at `0x4b5df4`)" |
| `VERSION_107` | **0x107** (263) | doc: "the record reader adds… when the stream's version field reads `0x107`"; GAME1.md §6.2: "`0x4b5c6b` … **only on a match** does it set the stream's version field (`object + 0xa`) to `0x107`" |
| `STAGE_OFFSET` | **-800** | doc: "`0xfce0` as a signed 16-bit value: the additive inverse of 800"; GAME1.md §6.2: "It then adds `0xfce0` to both `u16`s, which is `-800`" |
| `OBJECT_KIND_MIN` | **6** | doc: "The smallest object kind the switch at `0x4a7ee0` handles" |
| `OBJECT_KIND_MAX` | **0x32** (50) | doc: "The largest object kind the switch at `0x4a7ee0` handles"; GAME1.md §6.2: "handles `6` to `0x32` (6..=50)" |

### 3.6 `src/v1/scripts.rs`

| Name | Value | Quoted evidence |
|---|---|---|
| `V1_SCRIPT_VERSION` | **4** | doc: "Measured as 4 in both shipped containers (`HD.mms`, `pack.mms`). A container that reads otherwise is rejected"; GAME1.md §7: "`2 bytes u16, measured as 4 in both containers`" |

### 3.7 `cli::sanitize` (used by `sound::output_stem`, reproduce exactly)

```rust
pub fn sanitize(name: &str, index: usize) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' { c } else { '_' })
        .collect();
    let cleaned = cleaned.trim_matches('.').to_string();
    if cleaned.is_empty() { format!("res{index:05}") } else { cleaned }
}
```

Keep every `char` (Rust `char`, i.e. a Unicode scalar value) that is ASCII
alphanumeric or one of `-`, `_`, `.`; replace every other char with a single
`_`; then trim leading and trailing `.`; if the result is empty use
`res%05d` (5-wide zero-padded index). Note: multi-byte UTF-8 chars each become
one `_`.

---

## 4. `src/sound.rs`, function by function

### 4.1 `format_of`

```rust
pub fn format_of(container: &Container, index: usize) -> SoundFormat
```

Reads **only** `container.attr_of(index)` (the format byte `C`). No payload
access, no error return.

```
attr := attr_of index
sample_rate    := if attr &&& 3 = 0 then 11025 else if attr &&& 3 = 1 then 22050 else 44100
channels       := if attr &&& 8 != 0 then 2 else 1
bits_per_sample:= if attr &&& 4 != 0 then 16 else 8
return { sample_rate, channels, bits_per_sample }
```

Evidence, SOUNDS.md §2 "The format byte `C`":

| Bits | Meaning |
|---|---|
| 0–1 | sample rate: `0` = 11025, `1` = 22050, `2` or `3` = 44100 Hz |
| 2 | 16-bit samples |
| 3 | stereo (two channels); clear = mono |
| 4–7 | auxiliary; not needed to reconstruct the PCM |

Observed `C` values (SOUNDS.md §2) the port must handle: `0x01, 0x05, 0x06, 0x11,
0x15, 0x1d, 0x1e`.

### 4.2 `swap16` (private)

```rust
fn swap16(stored: &[u8]) -> Vec<u8>
```

Byte-swaps every pair and **preserves an odd trailing byte**. Output length is
always `stored.len()`.

```rust
let mut out = Vec::with_capacity(stored.len());
let mut chunks = stored.chunks_exact(2);
for pair in &mut chunks { out.push(pair[1]); out.push(pair[0]); }
out.extend_from_slice(chunks.remainder());
out
```

Lean: for `i : Nat`, `i+1 < len` ⇒ `out[2i] = stored[2i+1]`, `out[2i+1] = stored[2i]`;
an odd final byte is copied unchanged.

Rationale to preserve in a comment: GAME1.md §5.1 "the versions-2/3 sound
containers hold only stored 8-bit PCM and 16-bit DPCM output, neither of which
has a byte order to get wrong, so `src/sound.rs` copied stored 16-bit PCM
through on the stated assumption that it was 'already little-endian'. That
assumption is false for the first game's 228 stored-16-bit resources."
Measurement: "a quiet 16-bit signal puts almost all of its energy in its low
byte, so whichever byte position is nearly always `00`/`ff` is the high one."

### 4.3 `rail_fraction` (private)

```rust
fn rail_fraction(payload: &[u8], bits: u16) -> Option<f64>
```

Returns `None` for a payload too short to measure. **The 16-bit path
byte-swaps on the way in** — "16-bit payloads are byte-swapped on the way in,
the same way the output path swaps them, so the reading is measured in the order
the samples are actually meant to be read."

Quoted Rust (offset arithmetic — do not simplify):

```rust
let bytes_per_sample = if bits >= 16 { 2 } else { 1 };
let cap = MEASURE_SAMPLES * bytes_per_sample;                 // 441000 or 882000
let start = (payload.len().saturating_sub(cap)) / 2;
let window = &payload[start..start + payload.len().min(cap)];
if window.len() < bytes_per_sample * 2 { return None; }
```

So: `cap = 441000 * bytes_per_sample`; `start = (len − cap).max(0) / 2`; the
window is `payload[start .. start + min(len, cap)]`. When `len < cap` the window
is the whole payload.

16-bit branch (`bits >= 16`):

```rust
if window.len() % 2 != 0 { return None; }
for pair in window.chunks_exact(2) {
    let v = i16::from_le_bytes([pair[1], pair[0]]) as f64 / 32768.0;
    if v.abs() >= RAIL_LEVEL { rails += 1; }
    samples += 1;
}
```

i.e. each sample is `i16` from the **swapped** pair (`[pair[1], pair[0]]`), i.e.
the big-endian value of the payload, divided by `32768.0`; a rail is
`|v| >= 0.99`, i.e. `|sample| >= 32441`.

8-bit branch:

```rust
for &b in window {
    let v = (f64::from(b) - 128.0) / 128.0;
    if v.abs() >= RAIL_LEVEL { rails += 1; }
}
(rails, window.len())       // samples == window.len()
```

Rails are exactly the bytes `0`, `1` and `255` (`|b−128|/128 >= 0.99`
⟺ `|b−128| >= 126.72` ⟺ `b <= 1` or `b >= 255`).

Then:

```rust
if samples == 0 { return None; }
Some(rails as f64 / samples as f64)
```

Note the `f64` division is on `usize` counters cast to `f64`; a Lean port may use
`Float` of the ratio of two `Nat`s (or rational division) — the comparison
against `0.005` has a two-order-of-magnitude margin, so either is safe, but the
*integers* must match exactly.

### 4.4 `implied_bits` (private)

```rust
fn implied_bits(payload: &[u8], declared: u16) -> Option<u16>
```

```
other := if declared >= 16 then 8 else 16
if other = 16 && payload.len % 2 != 0 then return None      // "an odd payload cannot be 16-bit"
declared_rail := rail_fraction(payload, declared)?          -- None propagates
other_rail    := rail_fraction(payload, other)?
if declared_rail - other_rail > 0.005 then Some(other) else None
```

Doc: "Returns `Some(other)` only when the declared reading puts far more of the
payload on the rails than the other one. Returns `None` when the payload agrees
with the byte, when it is too short to say, or when the other depth would not
even fit."

### 4.5 `decode`

```rust
pub fn decode(container: &Container, index: usize) -> Result<DecodedSound>
```

Step by step, in this exact order:

1. `stored := container.payload(index)` — the raw stored payload slice.
2. `flags := container.flags_of(index)`; `attr := container.attr_of(index)`.
3. `format := format_of(container, index)`.
4. **Encoding selection**:

   ```
   if flags & 3 == 0:
       encoding := if attr & 4 != 0 then Pcm16 else Pcm8
   else:
       if attr & 4 == 0:
           bail!("resource {index} uses the differential codec but its format byte 0x{attr:02x} does not set the 16-bit flag")
       encoding := Dpcm16
   ```
   SOUNDS.md §2 "The flags byte `E`": `E & 3 == 0` ⇒ stored PCM
   (`E = 0x04` in this game), nonzero ⇒ differential (`E = 0x09`).
5. `if encoding == Dpcm16 then format.bits_per_sample = 16` — "The differential
   decoder expands one byte to one 16-bit sample. Its output is always 16-bit,
   regardless of what the raw format byte says."
6. `bits := format.bits_per_sample`; `overridden := false`.
7. **Payload-driven depth override**, only when `encoding` is `Pcm8` or `Pcm16`:

   a. Odd-payload guard, quoted:

   ```rust
   if bits >= 16 && stored.len() % 2 != 0 {
       eprintln!("note: {}: resource {index} '{}': the format byte says 16-bit but the \
                  payload is {} bytes and cannot hold a whole number of 16-bit samples; \
                  using the payload",
                 container.path.display(), container.name(index), stored.len());
       bits = 8;
       overridden = true;
   }
   ```

   The final string is exactly:
   `note: <path>: resource <index> '<name>': the format byte says 16-bit but the payload is <len> bytes and cannot hold a whole number of 16-bit samples; using the payload`

   Code comment: "This is what `mpApp.SND` resource 119 needs: its format byte
   says 16-bit and it is 7,817 bytes, which no 16-bit reading can account for."

   b. Rail test:

   ```rust
   if let Some(implied) = implied_bits(stored, bits) {
       eprintln!("note: {}: resource {index} '{}': the format byte says {bits}-bit \
                  but the payload reads as {implied}-bit; using the payload",
                 container.path.display(), container.name(index));
       bits = implied;
       overridden = true;
   }
   ```

   Final string exactly:
   `note: <path>: resource <index> '<name>': the format byte says <bits>-bit but the payload reads as <implied>-bit; using the payload`

   Note the second call uses the *possibly already overridden* `bits`.
8. `if overridden then format.bits_per_sample = bits`.
9. `expected := container.plain_len(index)` (v2/v3: the decoded-size table entry,
   or the stored length when it is 0; v1: always the stored length).
10. **Payload construction**:

    * `Pcm8 | Pcm16`:
      * `if expected != stored.len()` ⇒
        `bail!("resource {index} is stored PCM but the size table says {expected} bytes and the payload has {}", stored.len())`
      * `sample_bytes := bits_per_sample / 8` (1 or 2)
      * `if stored.len() % sample_bytes != 0` ⇒
        `bail!("resource {index} has {} bytes, not a whole number of {}-bit samples", stored.len(), bits_per_sample)`
      * `if bits >= 16 then swap16(stored) else stored.to_vec()`
    * `Dpcm16`:
      * `if expected != stored.len() * 2` ⇒
        `bail!("resource {index} is differential but the size table says {expected} bytes, while {} stored bytes expand to {}", stored.len(), stored.len()*2)`
      * `decode_dpcm(stored, format.channels)?`

11. **Frame check**:

    ```rust
    let frame_bytes = usize::from(format.channels) * usize::from(format.bits_per_sample / 8);
    if frame_bytes == 0 || pcm.len() % frame_bytes != 0 {
        bail!("resource {index}: {} decoded bytes do not form whole {}x{}-bit frames",
              pcm.len(), format.channels, format.bits_per_sample);
    }
    ```

12. Return `DecodedSound { format, encoding, pcm }`.

`--original` note that the port must reproduce: **`--original` writes
`decoded.pcm`, not `container.payload`** (see §16, item 1).

### 4.6 `decode_dpcm`

```rust
pub fn decode_dpcm(input: &[u8], channels: u16) -> Result<Vec<u8>>
```

Direct transliteration of `FUN_004c6b80` in `DMA4.exe`. Quoted:

```rust
if channels == 0 { bail!("differential PCM has zero channels"); }
let channels = usize::from(channels);
let mut previous = vec![0i16; channels];
let mut out = Vec::with_capacity(input.len() * 2);
let mut channel = 0usize;

for &code in input {
    let mut sample = DPCM_TABLE[usize::from(code)];
    if (code & 0x7f) > 0x2f && code != DPCM_SILENCE {
        sample = sample.wrapping_add(previous[channel]);
    }
    previous[channel] = sample;
    out.extend_from_slice(&sample.to_le_bytes());
    channel = (channel + 1) % channels;
}
Ok(out)
```

* `previous` is zeroed at the start of **every** call ("Predictors are
  independent per channel and are reset to zero at the start of the stream").
* The addition **wraps at 16 bits** (`wrapping_add`, i.e. two's-complement
  `Int16` arithmetic).
* Output is `2 * input.len()` bytes, signed 16-bit **little-endian**, channels
  interleaved round-robin starting at channel 0.
* SOUNDS.md §4 pseudocode (identical):

  ```text
  prev[0..channels) = 0
  channel = 0
  for each stored byte b:
      s = table[b]
      if (b & 0x7f) > 0x2f and b != 0xf8:
          s = (s + prev[channel]) as i16      // wrap at 16 bits
      prev[channel] = s
      output s as little-endian i16
      channel = (channel + 1) % channels
  ```

### 4.7 `output_stem` (private) — filename stem

```rust
fn output_stem(container: &Container, index: usize) -> String {
    let safe = sanitize(container.name(index), index);
    let lower = safe.to_ascii_lowercase();
    for extension in [".aif", ".aiff", ".wav"] {
        if lower.ends_with(extension) {
            let stem = &safe[..safe.len() - extension.len()];
            if !stem.is_empty() { return stem.to_string(); }
        }
    }
    safe
}
```

ASCII-lowercase comparison; if the sanitized name (case preserved) ends with
`.aif`, `.aiff` or `.wav`, strip that many bytes; if the remainder is empty, fall
through and return the unshortened `safe`. Evidence, SOUNDS.md §6.2: "Output
names are `NNNNN_<name>.flac` / `.pcm`, with a trailing `.aif`, `.aiff`, or
`.wav` removed from the resource name."

### 4.8 `warn_failure` (private)

```rust
fn warn_failure(container: &Container, index: usize, name: &str, error: &anyhow::Error) {
    eprintln!("warning: {}: sound resource {index} '{name}': {error:#}",
              container.path.display());
}
```

Exact text: `warning: <path>: sound resource <index> '<name>': <error chain>`.
`{error:#}` is anyhow's "alternate" Display: the outermost context, then `: `
then the cause, recursively (e.g. `writing X: <io error>`). This differs from
`error.to_string()`, which yields only the outermost context — see §4.9 step 5.

### 4.9 `extract_container`

```rust
pub fn extract_container(
    container: &Container,
    out_dir: &Path,
    original: bool,
) -> Result<ExtractReport>
```

1. `std::fs::create_dir_all(out_dir)?` — **no context message**; the raw
   `io::Error` propagates.
2. `let mut report = ExtractReport::default();`
3. `let mut flac: Option<bool> = None;` — the probe is deferred: "The encoder is
   only needed by the modern path, so the probe is deferred until the first
   modern resource rather than run on every container."
4. `for index in 0..container.resource_count as usize:`
   1. `let name = container.name(index);`
   2. `let decoded = match decode(container, index) { Ok(d) => d, Err(error) => { warn_failure(container, index, name, &error); report.failed.push((index, name.to_string(), error.to_string())); continue; } };`
      → per-resource failure, non-fatal. `error.to_string()` is the **outermost
      context only** (for `bail!` with no context, the message itself).
   3. `let stem = output_stem(container, index);`
   4. `if !original && !*flac.get_or_insert_with(flac_available) { bail!(...) }`
      — the missing-encoder error is a **fatal `bail!` out of the whole
      extraction**, not a per-resource warning:
      ```
      the 'flac' tool is required to write the modern output and was not found on PATH; install it (apt install flac / brew install flac), or use --original for the raw PCM
      ```
      Note this check happens **after** `decode` and after `output_stem`: a
      container whose first resource fails to decode never probes `flac`.
   5. `let extension = if original { "pcm" } else { "flac" };`
   6. `let path = out_dir.join(format!("{index:05}_{stem}.{extension}"));`
   7. `let written = if original { std::fs::write(&path, &decoded.pcm).with_context(|| format!("writing {}", path.display())) } else { write_flac(&path, &decoded) };`
   8. `if let Err(error) = written { warn_failure(container, index, name, &error); report.failed.push((index, name.to_string(), error.to_string())); continue; }`
   9. `report.written += 1;`
   10. `report.resources.push(ExtractedSound { index, name: name.to_string(), encoding: decoded.encoding, format: decoded.format, path, bytes: decoded.pcm.len() });`
5. `Ok(report)`

Measured behaviour to reproduce: `HD.MMA` + `app.MMA` = 683 resources, zero
failures (SOUNDS.md §6.2); `HD.SND` + `mpApp.SND` + `pack.SND` = 452 resources,
451 written and 1 failure (`mpApp.SND` resource 121 `SifBtS10.aif`, 7817 bytes,
odd, format byte `0x25`), GAME1.md §5.

---

## 5. `src/v1/mod.rs`, function by function

```rust
pub fn uncompressed_len(container: &Container, i: usize) -> usize
    = container.uncompressed_sizes.get(i).copied().unwrap_or(0) as usize
```

```rust
pub fn palette_of(container: &Container, _i: usize) -> Option<&[u8]>
    = container.palettes.first().map(|p| p.as_slice())     // entry 0, the index is ignored
```

```rust
pub fn palette_bank(container: &Container) -> &[Vec<u8]>   = &container.palettes
pub fn palette_count(container: &Container) -> usize      = container.palettes.len()
pub const fn palette_bytes() -> usize                     = PALETTE_BYTES /* 1024 */
```

* `uncompressed_len` doc: "Zero means the payload is stored exactly as it appears
  in the container. Any other value is the number of bytes
  [`lzss::decompress`] expands the payload to, and is how a first-game container
  says 'this payload is compressed'."
* `palette_of` doc, including the judgement call: "`mpApp.PIC` ships two palettes
  and nothing in the container says which entry a picture indexes into — there is
  no per-resource selector field — so that choice rests on a vision check rather
  than a byte: rendering `dmalog074`/`dmalog079` and `nameLess_230`/
  `nameLess_231` through both entries and asking `qwen/qwen3.8-27b` which is the
  intended rendering returns 'FIRST' (entry 0) for both pictures tested." GAME1.md
  §9 classifies this as **judgement, not measured**.
* Palettes are 1024 bytes each: "256 entries of transparency, red, green, blue"
  (GAME1.md §4.1).

---

## 6. `src/v1/lzss.rs` — `decompress`

```rust
pub fn decompress(src: &[u8], uncompressed: usize) -> Result<Vec<u8>>
```

Quoted in full (offset and window arithmetic — copy exactly):

```rust
let mut window = [0u8; V1_WINDOW];        // 1024 zero bytes
let mut wp = V1_WINDOW_START;             // 958
let mut out: Vec<u8> = Vec::with_capacity(uncompressed);
let mut pos = 0usize;
let mut control: u8 = 0;
let mut bits_left: u8 = 0;

while out.len() < uncompressed {
    // Refill the control byte every 8 tokens.
    if bits_left == 0 {
        let Some(&c) = src.get(pos) else {
            bail!("v1 lzss: control byte exhausted at input offset {pos}");
        };
        pos += 1;
        control = c;
        bits_left = 8;
    }
    let is_literal = control & 1 == 1;
    control >>= 1;
    bits_left -= 1;

    if is_literal {
        let Some(&b) = src.get(pos) else {
            bail!("v1 lzss: literal byte exhausted at input offset {pos}");
        };
        pos += 1;
        out.push(b);
        if out.len() >= uncompressed { break; }
        window[wp & 0x3FF] = b;
        wp = wp.wrapping_add(1) & 0x3FF;
    } else {
        let (Some(&b0), Some(&b1)) = (src.get(pos), src.get(pos + 1)) else {
            bail!("v1 lzss: match pair exhausted at input offset {pos}");
        };
        pos += 2;
        let word = ((b0 as usize) << 8) | b1 as usize;
        let distance = word & 0x3FF;
        let length = (b0 as usize >> 2) + 3;
        for i in 0..length {
            let b = window[(distance + i) & 0x3FF];
            out.push(b);
            if out.len() >= uncompressed { break; }
            window[wp & 0x3FF] = b;
            wp = wp.wrapping_add(1) & 0x3FF;
        }
    }
}
Ok(out)
```

Semantics to preserve exactly:

* Output length is **exactly** `uncompressed` (or fewer if the input is
  exhausted — but that is an error, see below). Trailing input is padding.
* The window starts all-zero and the write cursor starts at **958**, and every
  window index is masked with `0x3FF`. The doc explains why zeroing the whole
  ring is equivalent: "the original zeroes the first 958 of them at entry
  (`0x42e0f7`, counting to `0x3be`) … Zeroing the whole buffer is equivalent and
  is what this port does, because the game's window lives at the fixed address
  `0x517540` and would otherwise carry stale bytes between calls."
* `distance` is a **window slot**, not a reach-back: `window[(distance + i) & 0x3FF]`.
* Overlapping runs are legal (the match reads the window it is still writing).
* A literal/match that would push the output past `uncompressed` is truncated
  mid-token and the loop breaks **before** writing to the window.
* Only under-run errors exist; a too-long input is silently accepted.

Errors (all verbatim):

| Condition | Message |
|---|---|
| control byte missing | `v1 lzss: control byte exhausted at input offset {pos}` |
| literal byte missing | `v1 lzss: literal byte exhausted at input offset {pos}` |
| match pair missing (either byte) | `v1 lzss: match pair exhausted at input offset {pos}` |

(`{pos}` is the byte offset of the byte that was needed, *before* it would have
been consumed; for the match pair it is the offset of `b0`.)

Measured: `pack.PIC` 2,993 + `mpApp.PIC` 110 streams with `size > len`, 125
`pack.PIC` streams with `size == len`, 9,733 stored-plain resources, and all
12,961 pictures decode (GAME1.md §4.3, §8; `tests/game1.rs`).

---

## 7. `src/v1/metadata.rs` — `parse`

```rust
pub fn parse(
    data: &[u8],
    kind: ContainerKind,
    n: usize,
    base: usize,
    payload_start: usize,
    big_endian: bool,
    path: &std::path::Path,
) -> Result<V1Metadata>
```

`base` is the offset just past the `4*N` u32 array the caller has already
consumed; `payload_start` is `offsets[0]`. Read helpers:

```rust
let read_u16 = |off| if big_endian { u16::from_be_bytes([data[off], data[off+1]]) }
               else { u16::from_le_bytes([data[off], data[off+1]]) };
let read_u32 = |off| if big_endian { u32::from_be_bytes([data[off],data[off+1],data[off+2],data[off+3]]) }
               else { u32::from_le_bytes([...]) };
```

Control flow:

1. `let need = base + 4 * n; if need > data.len() { bail!("{}: version 1 metadata does not fit (need {}, have {})", path.display(), need, data.len()) }`
2. `let mut cursor = base + 4 * n;` (the caller already read that array)
3. `match kind`:

   * **`Pictures`** (region is `15*N + 4` bytes plus the palette bank):
     1. `if cursor + 2 > data.len() || read_u16(cursor) != 7` ⇒
        `bail!("{}: version 1 picture metadata marker is not 7", path.display())`
        then `cursor += 2`.
     2. `flags[i] = data[cursor + i]` for `i in 0..n`; `cursor += n`.
     3. `kinds[i] = data[cursor + i]` for `i in 0..n`; `cursor += n`.
     4. dimensions — the pair is `{height, width}`, the **second** u16 is the
        width:
        ```rust
        let h = read_u16(cursor + 4 * i);
        let w = read_u16(cursor + 4 * i + 2);
        meta.dimensions.push(Dimension { width: w, height: h });
        ```
        then `cursor += 4 * n`.
     5. `uncompressed_sizes[i] = read_u32(cursor + 4*i)`; `cursor += 4*n`.
     6. unknown N-byte array (skipped): `if cursor + n > data.len() { bail!("{}: version 1 unknown byte array does not fit (need {}, have {})", path.display(), cursor + n, data.len()) }` then `cursor += n`.
     7. `palette_count = read_u16(cursor) as usize`; `cursor += 2`.
     8. for `k in 0..palette_count`: `at = cursor + k*1024`, `want = at + 1024`;
        `if want > data.len() { bail!("{}: palette bank overruns the file (need {}, have {})", path.display(), want, data.len()) }`;
        push `data[at..want]`. Then `cursor += palette_count * 1024`.
   * **`Sounds`** (region is `5*N + 2` bytes after the timestamp array):
     ```
     if cursor + n + 2 > data.len():
         bail!("{}: version 1 sound metadata does not fit (need {}, have {})", path, cursor+n+2, data.len())
     attr[i] = data[cursor + i]        for i in 0..n
     cursor += n + 2                   # the N byte array and 2 bytes of padding
     ```
     **No marker check, no flags, no dimensions, no sizes, no palettes.**
   * **`Blobs`**: `cursor += 2` (nothing beyond the `4*N` array plus 2 bytes).
   * anything else (**`Scripts`** included): nothing — "no version-1 metadata
     region is decoded".
4. `if cursor != payload_start { bail!("{}: version 1 metadata ends at {} but the first payload starts at {}", path.display(), cursor, payload_start) }`
5. `Ok(meta)`

Note that `container.rs` performs its own, identically worded pre-check for
every version-1 kind before calling `parse`:
`"{}: version 1 metadata does not fit (need {}, have {})"` with
`need = names_end + 4*n`. So a Lean port must be able to emit that string from
both layers.

The layout table this implements (GAME1.md §3, quoted):

```text
Pictures   4*N  u32 handle array
            2   marker, always 0x0007
            N   flags byte array
            N   kind byte array
          4*N  {u16 height, u16 width} dimension table
          4*N  u32 uncompressed-size table
            N   byte array whose accessor is unknown
            2   u16 palette count
     count*1024  palette bank
Sounds     4*N  u32 Mac timestamps
            N   format byte array
            2   padding
Blobs      4*N  u32 handle array
            2   padding
Scripts    (no version-1 metadata region)
```

---

## 8. `src/v1/picture.rs`

### 8.1 `payload`

```rust
pub fn payload(container: &Container, i: usize) -> Result<Vec<u8>>
```

```rust
let raw = container.payload(i);
let uncompressed = super::uncompressed_len(container, i);
if uncompressed == 0 { return Ok(raw.to_vec()); }
lzss::decompress(raw, uncompressed).with_context(|| {
    format!("expanding resource {i} ('{}') from {} to {uncompressed} bytes",
            container.name(i), raw.len())
})
```

The context message is exactly:
`expanding resource <i> ('<name>') from <stored_len> to <uncompressed> bytes`.
It wraps any `lzss` error, so the full chain printed with `{:#}` is
`expanding resource 3 ('x') from 10 to 20 bytes: v1 lzss: literal byte exhausted at input offset 4`.

### 8.2 `decode` and `pixel_format`

```rust
pub fn decode(container: &Container, i: usize) -> Result<Decoded>
pub fn pixel_format(container: &Container, i: usize) -> Option<picture::PixelFormat>
```

```rust
pub fn decode(container: &Container, i: usize) -> Result<Decoded> {
    let data = payload(container, i)?;
    let palette = super::palette_of(container, i);
    picture::decode_bytes(container, i, &data, palette)
}

pub fn pixel_format(container: &Container, i: usize) -> Option<picture::PixelFormat> {
    let data = payload(container, i).ok()?;
    let palette = super::palette_of(container, i);
    picture::pixel_format(container, i, &data, palette)
}
```

Signatures of the shared functions the Lean port must call (from
`src/picture.rs`):

```rust
pub const PALETTE_BYTES: usize = 1024;
pub fn decode_bytes(container: &Container, index: usize, data: &[u8],
                    palette: Option<&[u8]>) -> Result<Decoded>;
pub fn pixel_format(container: &Container, index: usize, data: &[u8],
                    palette: Option<&[u8]>) -> Option<PixelFormat>;
```

`v1::picture::decode` never looks at the container's own decompressed-size table
— the LZSS step replaces the shared LZ77 step. Evidence, GAME1.md §4.3: "The
expanded stream is an ordinary `crate::picture` payload: the same `2*(2h+1)`
count word, the same big-endian offset table, the same alternating
transparent/opaque run bytes and the same colour records."

---

## 9. `src/v1/blobs.rs`

### 9.1 Payload layout (all big-endian)

```text
  0        u16  0x000c                       the stamp's length prefix
  2        "goofy 2.6.3\0"      (12 bytes)   the writer's version stamp
 14        u32  object_count
 18        u32  object_bytes  == object_count * 12
 22        object_count x 12-byte objects:
              u32 kind        6..=0x32      selects the game-object class
              u32 parameter                  handed to that class by address
              u32 records                    how many records follow it
 22+ob     u32  record_bytes     == the bytes that remain
 26+ob     record_bytes of 8-byte records, in object order, each object's run
           closed by a record whose event is 0

record (8 bytes):  u16 a; u16 b;  u32 event
```

Measured totals (GAME1.md §6.1): **209,751 objects and 8,817,121 records** over
305 payloads / 73,061,910 bytes.

### 9.2 `Record`

```rust
impl Record {
    pub fn read(payload: &[u8], at: usize) -> Result<Record> {
        let end = at.checked_add(RECORD_BYTES).context("record offset overflow")?;
        if end > payload.len() {
            bail!("record at {at:#x} runs off the payload ({} bytes)", payload.len());
        }
        Ok(Record {
            a: u16::from_be_bytes([payload[at], payload[at + 1]]),
            b: u16::from_be_bytes([payload[at + 2], payload[at + 3]]),
            event: u32::from_be_bytes([payload[at+4], payload[at+5], payload[at+6], payload[at+7]]),
        })
    }

    pub fn stage_coords(&self) -> (i32, i32) {
        (self.a as i32 + STAGE_OFFSET, self.b as i32 + STAGE_OFFSET)   // -800 each
    }

    pub fn write(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.a.to_be_bytes());       // 2 bytes BE
        out.extend_from_slice(&self.b.to_be_bytes());       // 2 bytes BE
        out.extend_from_slice(&self.event.to_be_bytes());   // 4 bytes BE
    }
}
```

Note `stage_coords` widens the `u16` to `i32` first, so it is a plain `−800`
(e.g. `1680 → 880`, `720 → -80`); the game's own `add word [esi], 0xfce0` is
`u16` wrapping arithmetic and produces the same values, which is what
`tests/game1_blobs.rs` asserts both ways.

### 9.3 `Blob::decode`

```rust
pub fn decode(payload: &[u8]) -> Result<Blob>
```

Ordered checks — **every one is a hard failure**, in this order:

1. `if payload.len() < 14` ⇒
   `bail!("payload is {} bytes, too short for the {} byte stamp", payload.len(), 14)`
2. `if &payload[..14] != STAMP` ⇒
   `bail!("payload does not start with the goofy stamp (found {:02x?})", &payload[..14])`
   — `{:02x?}` on a `&[u8]` renders as `[00, 0c, 67, 6f, 6f, 66, 79, 20, 32, 2e, 36, 2e, 33, 00]`:
   square brackets, lowercase two-digit hex per byte, `, ` separators, no
   spaces inside the brackets.
3. `object_count = read_u32(payload, 14)?`; `object_bytes = read_u32(payload, 18)?`.
4. `if object_count == 0` ⇒ `bail!("the header's object count is zero")`.
5. `if object_bytes != object_count * 12u32` ⇒
   `bail!("the header's object_bytes is {object_bytes}, expected {} for {object_count} objects", object_count * 12)`
6. `let objects_end = (22).checked_add(object_bytes as usize).context("the object section runs past the payload")?;`
7. `if objects_end + 4 + 8 > payload.len()` ⇒
   `bail!("the object section ends at {objects_end:#x} but the payload is {} bytes", payload.len())`
   (`{:#x}` renders `0x166a`.)
8. `record_bytes = read_u32(payload, objects_end)?`;
   `records_start = objects_end + 4`; `remaining = payload.len() - records_start`;
   `if record_bytes as usize != remaining` ⇒
   `bail!("the header's record_bytes is {record_bytes} but {remaining} bytes remain")`
9. `if remaining % 8 != 0` ⇒
   `bail!("the record stream is {remaining} bytes, not a whole number of 8-byte records")`
10. Object headers, `for index in 0..object_count`:
    * `at = 22 + index*12`; `kind = read_u32(at)?`, `parameter = read_u32(at+4)?`,
      `count = read_u32(at+8)?`.
    * `if !(6..=50).contains(&kind)` ⇒
      `bail!("object {index} has kind {kind}, outside the 6..=50 the switch at 0x4a7ee0 handles")`
    * `total_records = total_records.checked_add(count as usize).context("the object record counts overflow")?`
11. `if total_records != remaining / 8` ⇒
    `bail!("the object record counts sum to {total_records} but the stream holds {} records", remaining / 8)`
12. Record stream, partitioned by the objects' own counts:
    ```rust
    let mut at = records_start;
    for object in objects.iter_mut() {
        for _ in 0..object.record_count {
            object.records.push(Record::read(payload, at)?);
            at += RECORD_BYTES;
        }
        let last = object.records.last().context("an object's record run is empty")?;
        if last.event != 0 {
            bail!("an object of kind {} ends on event {}, not the 0 terminator",
                  object.kind, last.event);
        }
    }
    ```
13. `if at != payload.len()` ⇒
    `bail!("the record stream ends at {at:#x}, not the payload's {} bytes", payload.len())`

`read_u32` helper (private):

```rust
fn read_u32(payload: &[u8], off: usize) -> Result<u32> {
    let end = off.checked_add(4).context("offset overflow")?;
    if end > payload.len() { bail!("u32 at {off:#x} runs off the payload"); }
    Ok(u32::from_be_bytes([payload[off], payload[off+1], payload[off+2], payload[off+3]]))
}
```

### 9.4 `Blob::encode` — the exact output bytes

```rust
out.extend_from_slice(STAMP);                                  // 14 bytes
out.extend_from_slice(&self.object_count.to_be_bytes());       // 4 bytes BE at 14
out.extend_from_slice(&self.object_bytes.to_be_bytes());       // 4 bytes BE at 18
for object in &self.objects {
    out.extend_from_slice(&object.kind.to_be_bytes());         // 4 bytes BE
    out.extend_from_slice(&object.parameter.to_be_bytes());    // 4 bytes BE
    out.extend_from_slice(&object.record_count.to_be_bytes()); // 4 bytes BE
}
out.extend_from_slice(&self.record_bytes.to_be_bytes());       // 4 bytes BE at 22+object_bytes
for object in &self.objects {
    for record in &object.records { record.write(&mut out); }  // 8 bytes BE each
}
```

Total length = `22 + object_bytes + 4 + Σ(records.len() * 8)`. This is asserted
to equal the original payload for all 305 payloads
(`tests/game1_blobs.rs::every_payload_round_trips_through_decode_and_encode`).

Also public: `Blob::version() -> u32` = `VERSION_107` (`0x107`), and
`Blob::records() -> impl Iterator<Item = &Record>` = a flat map over the objects.

### 9.5 `Blob::extract_container`

```rust
pub fn extract_container(container: &Container, out_dir: &Path, original: bool) -> Result<usize>
```

1. `if container.kind != Blobs` ⇒
   `bail!("{}: expected a Blobs container, found {:?}", container.path.display(), container.kind)`
2. `if container.version != 1` ⇒
   `bail!("{}: expected a version-1 container, found version {}", container.path.display(), container.version)`
   (`{:?}` on `ContainerKind` is the Rust `Debug` name: `Blobs`, `Sounds`, …)
3. `let n = container.resource_count as usize;`
4. `if original { return write_originals(container, out_dir); }` — **before** any
   modern-mode directory is created.
5. `create_dir_all(out_dir)` with context `format!("creating {}", out_dir.display())`.
6. `let payloads = out_dir.join("payloads"); create_dir_all(&payloads)` with the
   same context shape.
7. Aggregate counters `objects`, `records`, `events: BTreeMap<u32, usize>`,
   `kinds: BTreeMap<u32, usize>`, `written` (all `BTreeMap`s are key-sorted).
8. `for index in 0..n`:
   * `name = container.name(index)`; `payload = container.payload(index)`;
   * `blob = match Blob::decode(payload) { Ok(b) => b, Err(e) => { eprintln!("warning: {}: resource {index} '{name}': {e}", container.path.display()); continue; } }`
     — note: `{e}` here is plain `Display` of the `anyhow::Error`, i.e. the
     outermost message only, **not** `{:#}`.
   * `objects += blob.objects.len()`; for each object `*kinds.entry(kind).or_insert(0) += 1`
     and for each record `*events.entry(event).or_insert(0) += 1`;
     `records += blob.records().count()`.
   * `safe = safe_name(name, index)`; write
     `payloads.join(format!("{index:05}_{safe}.json"))` with the bytes of
     `blob_json(&blob, index, name)`, context
     `format!("writing {}", file.display())`.
   * `written += 1`.
9. Build the manifest (exact template, §9.7) and write
   `out_dir.join("manifest.txt")` with context
   `format!("writing {}", out_dir.join("manifest.txt").display())`.
10. `Ok(written)`.

### 9.6 `write_originals`, `safe_name`, `blob_json`, `json_escape`

```rust
fn write_originals(container: &Container, out_dir: &Path) -> Result<usize> {
    std::fs::create_dir_all(out_dir).with_context(|| format!("creating {}", out_dir.display()))?;
    let mut written = 0usize;
    for index in 0..container.resource_count as usize {
        let name = container.name(index);
        let safe = safe_name(name, index);
        let file = out_dir.join(format!("{index:05}_{safe}.blb"));
        std::fs::write(&file, container.payload(index))
            .with_context(|| format!("writing {}", file.display()))?;
        written += 1;
    }
    Ok(written)
}
```

* Original-mode files: `<out_dir>/NNNNN_<safe>.blb`, contents = the container's
  payload slice byte for byte (no subdirectory).
* Modern-mode files: `<out_dir>/payloads/NNNNN_<safe>.json` plus
  `<out_dir>/manifest.txt`.

`safe_name` (note: **no** `.trim_matches('.')`, unlike `cli::sanitize`):

```rust
fn safe_name(name: &str, index: usize) -> String {
    let cleaned: String = name.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' { c } else { '_' })
        .collect();
    if cleaned.is_empty() { format!("payload{index:05}") } else { cleaned }
}
```

`json_escape`:

```rust
fn json_escape(text: &str) -> String {
    for c in text.chars() {
        match c {
            '"'  => "\\\"",
            '\\' => "\\\\",
            '\n' => "\\n",
            '\r' => "\\r",
            '\t' => "\\t",
            c if (c as u32) < 0x20 => "\\u{:04x}",   // lowercase hex, 4 digits
            c => c,
        }
    }
}
```

`blob_json` — the exact bytes, line by line (every line ends `\n`):

```
{
  "format": "goofy 2.6.3 blob",
  "index": <index>,
  "name": "<json_escape(name)>",
  "stamp": "<json_escape(String::from_utf8_lossy(&STAMP[2..]))>",
  "version": <blob.version()>,
  "objects": [
    {
      "kind": <object.kind>,
      "parameter": <object.parameter>,
      "records": <object.record_count>,
      "path": [<record elements>]
    },
    ...      // one object block per object, comma after every block but the last
  ]
}
```

where each record element is `[a,b,event,sa,sb]` with `sa`,`sb` =
`stage_coords()`, elements joined by `,` with **no space**, and no trailing
comma after the last element of the last object. `STAMP[2..]` is
`b"goofy 2.6.3\x00"`, whose `from_utf8_lossy` string contains a NUL, so the
`stamp` value renders as `"goofy 2.6.3\u0000"` (the NUL is escaped by the
`< 0x20` arm).

### 9.7 `manifest.txt` — exact bytes

```
# First-game Blobs container: the goofy 2.6.3 payloads
<blank line>
container: <container.path.display()>
version: <container.version>
payloads: <n>, decoded: <written>
objects: <objects>
records: <records>
<blank line>
payload layout (docs/GAME1.md section 6):
  14-byte stamp, u32 object_count, u32 object_bytes == count * 12,
  object_count x 12-byte objects {u32 kind, u32 parameter, u32 records},
  u32 record_bytes, then the 8-byte records in object order.
<blank line>
object kinds (the switch at FUN_004a7ee0):
  kind {kind:>3}: {how_many:>6} objects        // for each kind, ascending key
<blank line>
record events:
  event {event:>3}: {how_many:>7} records      // for each event, ascending key
<blank line>
Event 0 closes each object's run; the record reader FUN_004b5f34
returns 0/1/2 for an object's first, middle and last record. The
coordinates are u16s that the reader adds -800 to (the stream's
version field is 0x107), and that FUN_004a7bc4 then scales through a
16.16 multiply and offsets by a stage origin, so they are not raw
pixels. Each JSON carries both the stored and the -800 values.
```

Formatting details: `kind {kind:>3}` and `{how_many:>6}` are right-aligned
decimal with a minimum width of 3 and 6 (no padding when the value is wider);
`event {event:>3}` and `{how_many:>7}` widths 3 and 7; the `BTreeMap` iteration
order is ascending numeric key.

---

## 10. `src/v1/scripts.rs`

### 10.1 Container layout

```text
0x00  16 bytes  "MMFW " + "Script", NUL padded
0x10   2 bytes  endian marker ("MM")
0x12   2 bytes  u16 version, 1
0x14   6 bytes  file id
0x1A   8 bytes  timestamp (f64); its low u16 is the resource count at 0x1E
0x1E   2 bytes  u16 resource count N
0x20  4*(N+1)  u32 offsets; resource i spans [off[i], off[i+1])
        2 bytes u16, measured as **4** in every shipped container
       …        payloads, concatenated, starting at offsets[0]
```

There is **no names table and no metadata region**. Two checks pin it down:
`offsets[0] == 0x20 + 4*(N+1) + 2` and the last offset equals the file length
(`HD.mms` 1218 / 75889, `pack.mms` 3990 / 574861).

### 10.2 `parse`

```rust
pub fn parse(path: &Path, data: &[u8], container: &Container, table_end: usize) -> Result<V1Scripts>
```

`table_end` is `0x20 + 4 * (N + 1)`.

1. `if container.kind != Scripts` ⇒ `bail!("{}: not a Script container", path.display())`
2. `let n = container.resource_count as usize; let payload_start = container.offsets.first().copied().unwrap_or(0) as usize;`
3. `let version = crate::container::read_u16(data, table_end, container.big_endian);`
   `if version != 4` ⇒
   `bail!("{}: version-1 script marker at {table_end:#x} is {version}, expected 4", path.display())`
4. `if payload_start != table_end + 2` ⇒
   `bail!("{}: version-1 script payloads start at {payload_start} but the marker ends at {}", path.display(), table_end + 2)`
5. For every resource: `let name = trailing_name(container.payload(index));`
   count the non-empty ones; push into `names` (length `n`, empty string when
   the payload carries no name).
6. `eprintln!("note: {}: {} of {} script objects carry a name in their payload's string tail", path.display(), named, n);`
   Exact text: `note: <path>: <named> of <n> script objects carry a name in their payload's string tail`
7. Return `V1Scripts { version, header_end: table_end + 2, payload_start, names }`.

`trailing_name` (private):

```rust
fn trailing_name(payload: &[u8]) -> String {
    let mut end = payload.len();
    while end > 0 && payload[end - 1] == 0 { end -= 1; }              // strip trailing NULs
    let mut start = end;
    while start > 0 && is_printable(payload[start - 1]) { start -= 1; } // last printable run
    if end - start < 3 { return String::new(); }
    String::from_utf8_lossy(&payload[start..end]).into_owned()
}
fn is_printable(byte: u8) -> bool { byte.is_ascii_graphic() || byte == b' ' }
```

`is_ascii_graphic()` is `0x21..=0x7e`. The run must be **at least 3 bytes**, and
it is the **last** printable run of the payload. Evidence, GAME1.md §7: names
are `char606`, `char1a`, `rdy53`, `sky333`; "126 of 295 `HD.mms` payloads and 664
of 988 `pack.mms` payloads carry a trailing name".

### 10.3 `extract_container`

```rust
pub fn extract_container(container: &Container, out_dir: &Path, original: bool) -> Result<usize>
```

1. `if container.kind != Scripts` ⇒
   `bail!("{}: expected a Scripts container, found {:?}", container.path.display(), container.kind)`
2. `if container.version != 1` ⇒
   `bail!("{}: expected a version-1 container, found version {}", container.path.display(), container.version)`
3. `create_dir_all(out_dir)` with context `format!("creating {}", out_dir.display())`.
4. `let scripts = parse(&container.path, &container.data, container, 0x20 + 4 * (container.resource_count as usize + 1))?;`
5. `if original { write_originals(container, &scripts, out_dir) } else { write_measurements(container, &scripts, out_dir) }`

`write_originals`:

```rust
for index in 0..container.resource_count as usize {
    let name = safe_name(&scripts.names[index], index);
    let file = out_dir.join(format!("{index:05}_{name}.bin"));
    match std::fs::write(&file, container.payload(index)) {
        Ok(()) => written += 1,
        Err(e) => { failed += 1;
            eprintln!("warning: could not write {} (object {index}): {e}", file.display()); }
    }
}
if failed > 0 {
    eprintln!("warning: {} of {} script objects could not be written", failed, container.resource_count);
}
Ok(written)
```

Exact warning texts:
`warning: could not write <path> (object <index>): <io error>` and
`warning: <failed> of <resource_count> script objects could not be written`.
Write failures are **counted, not fatal**. Files: `<out_dir>/NNNNN_<name>.bin`,
contents = the container's payload slice byte for byte.

`write_measurements`:

* creates `<out_dir>/objects/` (context `creating …`),
* per resource: `count` = the first 4 payload bytes as **big-endian** u32, or
  `0` when `payload.len() < 4`;
  `*counts.entry(count).or_insert(0) += 1` (a `BTreeMap`, ascending keys);
  `named` counts non-empty names;
  writes `<out_dir>/objects/NNNNN_<name>.txt` (context `writing …`) with exactly:

  ```
  object <index>
  payload bytes: <payload.len()>
  leading u32 (count): <count>
  name: <name>            (or the literal "(none in payload)" when the name is empty)
  ```

  plus, only when `payload.len() >= 4`, one more line:

  ```
  bytes after the count: <payload.len() - 4> (the per-class record length is unmeasured)
  ```

* then `manifest.txt`:

```
# First-game script container: what is measured
<blank line>
container: <container.path.display()>
version: <scripts.version>
objects: <n>
payloads start at: <scripts.payload_start> (offsets table size <scripts.payload_start - 2> + the version u16)
objects named in their payload: <named> of <n>
<blank line>
distribution of the leading u32 (count) across objects:
  count {count:>6}: {how_many:>4} objects     // for each count, ascending key
<blank line>
NOT DECODED: the fixed-size record between each payload's leading
count and its trailing name. Its length varies per object, as the
games-2/3 `type_aux` table does, and the per-class sizes are still
unmeasured. Use --original for the byte-exact payloads.
```

* returns `Ok(n)` — **every** object is counted as written, even though the
  individual writes use `?` on failure (a write failure aborts the whole
  container with an error, unlike `write_originals`).

`safe_name` (private, same shape as the blobs one but the fallback differs):

```rust
fn safe_name(name: &str, index: usize) -> String {
    let cleaned: String = name.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' { c } else { '_' })
        .collect();
    if cleaned.is_empty() { format!("object{index}") } else { cleaned }   // note: no zero padding
}
```

---

## 11. The `flac` subprocess — exact specification

### 11.1 Probe (`flac_available`, private)

```
argv:  ["flac", "--version"]
stdin: /dev/null      (Stdio::null)
stdout: /dev/null
stderr: /dev/null
env:   inherited unchanged (Command::env is never called; PATH is used to
       resolve "flac", execvp-style)
```

Returns `true` iff the process spawns **and** exits with status 0; any spawn
error yields `false` (never panics, never falls back to WAV). It is called at
most **once per extraction**, lazily, on the first modern resource
(`flac.get_or_insert_with(flac_available)`).

### 11.2 Encode (`write_flac`, private)

```rust
fn write_flac(path: &Path, decoded: &DecodedSound) -> Result<()>
```

```
argv:  ["flac", "--silent", "--force", "-8", "--output-name", <path>, "-"]
```

* `<path>` is the destination file path, exactly as passed (`out_dir/…`), passed
  as an `OsStr` — on Unix, the raw bytes of the path.
* The final `-` means "read the WAV from stdin"; `flac` auto-detects the format
  from the header, so no format flags are given.
* `--force` overwrites an existing file ("Overwrite an existing file rather than
  failing on a re-run").
* `-8` is the compression level, spelled only as the short option (see
  `FLAC_COMPRESSION` in §3.1: `--compression-level` is an ambiguous prefix in
  flac 1.4.3 and a bare `8` is read as a second input file).

stdio wiring:

| stream | wiring |
|---|---|
| stdin | `piped` — the WAV bytes (§12) are written with `std::io::copy` from a `Cursor<Vec<u8>>`, then the pipe is **closed** (`drop(stdin)`) before waiting |
| stdout | `null` |
| stderr | `piped` — captured by `wait_with_output` |

Sequence and failure handling, in order:

1. `.spawn().with_context(|| format!("running {FLAC_TOOL} to encode {}", path.display()))?`
   → context text: `running flac to encode <path>`
2. `child.stdin.take().context("flac's stdin was already taken")?`
   → exact message: `flac's stdin was already taken`
3. `let bytes = wav_bytes(decoded);`
   `let write = std::io::copy(&mut Cursor::new(bytes), &mut stdin);`
   `drop(stdin);`
   `let output = child.wait_with_output().with_context(|| format!("waiting for {FLAC_TOOL} on {}", path.display()))?;`
   → context text: `waiting for flac on <path>`
4. `if let Err(error) = write { bail!("feeding WAV bytes to {FLAC_TOOL} for {}: {error}", path.display()) }`
   → exact message: `feeding WAV bytes to flac for <path>: <io error>`
   (checked **after** the wait, so a broken pipe is only reported once the child
   has been reaped)
5. `if !output.status.success() { bail!("{FLAC_TOOL} failed on {}: {}", path.display(), String::from_utf8_lossy(&output.stderr).trim()) }`
   → exact message: `flac failed on <path>: <stderr, lossy UTF-8, trimmed>`
6. `Ok(())`

The missing-encoder error (raised by `extract_container`, not `write_flac`) is
fatal and verbatim:

```
the 'flac' tool is required to write the modern output and was not found on PATH; install it (apt install flac / brew install flac), or use --original for the raw PCM
```

The port must **never** fall back to WAV. Evidence, SOUNDS.md §6.1: "If it is
missing the extraction fails loudly, once, naming the install command — it never
falls back to WAV, because silently changing the output format is the one thing
this program must not do." And AGENTS.md: "`src/sound.rs` fails loudly and once,
naming the install command, when it is missing, rather than silently writing
WAV."

Verification argv (used by the tests, §14.3) — the same tool in reverse:

```
["flac", "--decode", "--silent", "--force-raw-format", "--endian=little",
 "--sign=<unsigned when bits==8 else signed>", "--force",
 "--output-name", <path.with_extension("raw.out")>, <path>]
```

---

## 12. `wav_bytes` — the exact intermediate WAV bytes

```rust
fn wav_bytes(decoded: &DecodedSound) -> Vec<u8>
```

The `hound` 3.5.1 `WavWriter` over an in-memory `Cursor<Vec<u8>>`, with

```rust
hound::WavSpec {
    channels: decoded.format.channels,
    sample_rate: decoded.format.sample_rate,
    bits_per_sample: decoded.format.bits_per_sample,
    sample_format: hound::SampleFormat::Int,
}
```

The Lean port must emit these bytes itself (no need to call hound). With
`channels <= 2` and `bits <= 16` — which is every case the decoders produce
(1 or 2 channels; 8 or 16 bits) — hound writes the classic `PCMWAVEFORMAT`
form, so the file is a **44-byte header followed by the data**:

| Offset | Length | Value |
|---|---|---|
| 0 | 4 | `"RIFF"` (`0x52 49 46 46`) |
| 4 | 4 | u32 LE `file_size = data_bytes + 36` |
| 8 | 4 | `"WAVE"` |
| 12 | 4 | `"fmt "` |
| 16 | 4 | u32 LE `16` (fmt chunk size) |
| 20 | 2 | u16 LE `1` (`WAVE_FORMAT_PCM`, because `SampleFormat::Int`) |
| 22 | 2 | u16 LE `channels` |
| 24 | 4 | u32 LE `sample_rate` |
| 28 | 4 | u32 LE `byte_rate = sample_rate * bytes_per_sample * channels` |
| 32 | 2 | u16 LE `block_align = byte_rate / sample_rate` (`= bytes_per_sample * channels`) |
| 34 | 2 | u16 LE `bits_per_sample` (8 or 16) |
| 36 | 4 | `"data"` |
| 40 | 4 | u32 LE `data_bytes = len(decoded.pcm)` |
| 44 | … | the samples |

where `bytes_per_sample = (bits_per_sample + 7) / 8` (1 for 8-bit, 2 for 16-bit).
All header integers are **little-endian** even though the container is
big-endian.

Sample bytes:

* **8-bit path** — `for &sample in &decoded.pcm { writer.write_sample((i16::from(sample) - 128) as i8) }`.
  hound's `i8` writer for `(bits=8, byte_width=1)` emits
  `u8_from_signed(x) = ((x as i16) + 128) as u8`. Composing with the decoder's
  `(i16::from(b) - 128) as i8` returns `b` for every `b in 0..=255`
  (e.g. `0x80 → -128 → 128`, `0xff → 127 → 255`). **The 8-bit data chunk is
  therefore byte-identical to `decoded.pcm`.** The comment in the Rust explains
  the intent: "hound's 8-bit interface is signed; adding 128 on write recovers
  the original unsigned byte exactly."
* **16-bit path** — `for bytes in decoded.pcm.chunks_exact(2) { writer.write_sample(i16::from_le_bytes([bytes[0], bytes[1]])) }`,
  and hound writes it back little-endian. **The 16-bit data chunk is therefore
  byte-identical to `decoded.pcm`** (an odd trailing byte would be dropped, but
  `decode` has already guaranteed a whole number of frames).

`writer.finalize()` then seeks back and patches bytes 4..8 and 40..44; the
returned `Vec` is the 44-byte header plus the data. This is the **only** place
the WAV is materialised: "an intermediate WAV is the one format its encoder
reads from stdin, and writing it once here keeps the 8-bit unsigned convention —
the only place WAV and FLAC differ — in a single place."

---

## 13. Output files

### 13.1 Sounds (`src/sound.rs`)

| Mode | Path | Bytes |
|---|---|---|
| modern | `<out_dir>/{index:05}_{output_stem}.flac` | the FLAC file produced by the `flac` CLI from the WAV bytes of §12 (deterministic given the same `flac` build) |
| original | `<out_dir>/{index:05}_{output_stem}.pcm` | `decoded.pcm` verbatim — **no header** |

`{index:05}` is a zero-padded 5-wide decimal index. `output_stem` is §4.7.
The `.pcm` file "carries no header, so the accompanying rate/channel information
is the `C` byte in the container metadata (and is preserved in the modern FLAC
path)" (SOUNDS.md §6.2).

Contents of `.pcm` per encoding — this is the subtle part:

| encoding | `.pcm` bytes |
|---|---|
| `Pcm8` | the container's payload slice, unchanged |
| `Pcm16` | the payload **byte-swapped** (`swap16`) — *not* the container's bytes |
| `Dpcm16` | `2 * stored.len()` bytes: the interleaved signed 16-bit LE sample stream |

The `ExtractReport` records `bytes = decoded.pcm.len()` for every written
resource.

### 13.2 First-game Blobs (`src/v1/blobs.rs`)

| Mode | Path | Bytes |
|---|---|---|
| original | `<out_dir>/{index:05}_{safe_name}.blb` | the container's payload slice, byte for byte |
| modern | `<out_dir>/payloads/{index:05}_{safe_name}.json` | the JSON of §9.6 |
| modern | `<out_dir>/manifest.txt` | the manifest of §9.7 |

### 13.3 First-game Scripts (`src/v1/scripts.rs`)

| Mode | Path | Bytes |
|---|---|---|
| original | `<out_dir>/{index:05}_{safe_name}.bin` | the container's payload slice, byte for byte |
| modern | `<out_dir>/objects/{index:05}_{safe_name}.txt` | the `.txt` of §10.3 |
| modern | `<out_dir>/manifest.txt` | the manifest of §10.3 |

### 13.4 First-game Pictures (for completeness — the `--original` path calls into `v1::picture`)

`cli::extract_pictures_original` writes `{index:05}_{sanitize(name, index)}.dat`
containing `picture_payload(container)(container, index)` = `v1::picture::payload`
= the container slice when the declared uncompressed size is 0, otherwise the
LZSS expansion. Not part of this port except that `v1::picture::payload` is used
to produce the bytes.

---

## 14. Tests that pin the behaviour down (assertions copied)

### 14.1 Unit tests inside `src/sound.rs`

```rust
#[test]
fn silence_code_produces_zero_samples() {
    let pcm = decode_dpcm(&[DPCM_SILENCE; 4], 1).unwrap();
    assert_eq!(pcm, vec![0; 8]);
}

#[test]
fn predictor_is_independent_per_channel() {
    // Code 0 is an absolute +1; code 0x30 adds the table's +2048 to the
    // previous sample of its channel.
    let mono = decode_dpcm(&[0, 0x30, 0x30, DPCM_SILENCE], 1).unwrap();
    assert_eq!(
        mono.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect::<Vec<_>>(),
        vec![1, 2049, 4097, 0]
    );

    let stereo = decode_dpcm(&[0, 0, 0x30, 0x30], 2).unwrap();
    assert_eq!(
        stereo.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect::<Vec<_>>(),
        vec![1, 1, 2049, 2049]
    );
}
```

(Note `decode_dpcm(&[0, 0, 0x30, 0x30], 2)` gives `1, 1` because code `0` is an
absolute +1 and each channel's predictor is separate.)

### 14.2 `tests/sounds.rs` — the shared (v2/3) sound format

`raw_assets_dir()` = `$DMA_ASSETS` or `/RawAssets` if either `is_dir()`, else
skip+pass. `SOUND_CONTAINERS = ["HD.MMA", "app.MMA"]`.

**`sound_metadata_layout_matches_the_loader`** — read straight from the file:

```rust
let n = u16::from_be_bytes([data[0x22], data[0x23]]) as usize;
let names_base = 0x24 + 4 * (n + 1) + 2;
let names_end = names_base + 32 * n;
let payload_start = u32::from_be_bytes([data[0x24], data[0x25], data[0x26], data[0x27]]) as usize;
assert_eq!(payload_start - names_end, 14 * n + 2, "{name}: sound metadata is not 14*N+2 bytes");
assert_eq!(&data[names_end + 4*n + 2 .. names_end + 5*n + 2],
           &Container::open(&path).unwrap().attr[..],  "{name}: attribute array is not at 4*N+2");
assert_eq!(&data[names_end + 5*n + 2 .. names_end + 6*n + 2],
           &Container::open(&path).unwrap().flags[..], "{name}: flags array is not at 5*N+2");
assert_eq!(container.kind, ContainerKind::Sounds);
assert_eq!(container.resource_count as usize, n);
assert!(container.dimensions.iter().all(|d| d.width == 0 && d.height == 0));
```

then per resource, from the container's own arrays:

```rust
match flags & 3 {
    0 => { assert_eq!(attr & 4, 0);                       // stored PCM declares 8-bit
           assert_eq!(decoded_size, stored); }             // size == stored length
    _ => { assert_ne!(attr & 4, 0);                        // differential declares 16-bit
           assert_eq!(decoded_size, stored * 2); }         // size == 2 * stored length
}
```

**`every_sound_decodes_and_its_modern_output_round_trips`**:

```rust
let report = sound::extract_container(&container, &out_dir, false).expect("extract sounds");
assert!(report.failed.is_empty(), "{name}: {:?}", report.failed);
assert_eq!(report.written, n, "{name}: not every sound was written");
assert_eq!(report.resources.len(), n);
for entry in &report.resources {
    let decoded = sound::decode(&container, entry.index).expect("decode");
    assert_eq!(entry.format, decoded.format);
    assert_eq!(entry.encoding, decoded.encoding);
    assert_eq!(entry.bytes, decoded.pcm.len());
    assert_eq!(entry.path.extension().and_then(|e| e.to_str()), Some("flac"));
    assert_flac_round_trips(name, entry.index, &entry.path, &decoded);
}
```

`assert_flac_round_trips` = `assert_streaminfo_matches` **plus** the raw decode:

```rust
let raw_path = path.with_extension("raw.out");
let sign = if decoded.format.bits_per_sample == 8 { "unsigned" } else { "signed" };
// flac --decode --silent --force-raw-format --endian=little --sign=<sign> --force
//       --output-name <raw_path> <path>
assert_eq!(recovered.len(), decoded.pcm.len());
assert_eq!(recovered, decoded.pcm);        // byte-identical round trip
```

`assert_streaminfo_matches` parses the FLAC header by hand:

```rust
assert_eq!(&bytes[..4], b"fLaC");
assert_eq!(bytes[4] >> 1, 0);                              // first block is STREAMINFO
let body_len = u32::from_be_bytes([0, bytes[5], bytes[6], bytes[7]]) as usize;
assert_eq!(body_len, 34);
let info = &bytes[8..42];
let packed = u64::from_be_bytes(info[10..18]);
let sample_rate    = (packed >> 44) & 0xf_ffff;            // 20 bits
let channels       = ((packed >> 41) & 0x7) + 1;           // 3 bits + 1
let bits_per_sample= ((packed >> 36) & 0x1f) + 1;          // 5 bits + 1
let total_frames   = packed & 0xf_ffff_ffff;               // 36 bits
assert_eq!(sample_rate, u64::from(decoded.format.sample_rate));
assert_eq!(channels, u64::from(decoded.format.channels));
assert_eq!(bits_per_sample, u64::from(decoded.format.bits_per_sample));
assert_eq!(total_frames * channels * bits_per_sample / 8, decoded.pcm.len() as u64);
```

**`original_output_is_the_decoded_payload_verbatim`**:

```rust
let report = sound::extract_container(&container, &out_dir, true).expect("extract sounds");
assert!(report.failed.is_empty());
assert_eq!(report.written, n);
for entry in &report.resources {
    let decoded = sound::decode(&container, entry.index).expect("decode");
    let written = std::fs::read(&entry.path).expect("read pcm");
    assert_eq!(written, decoded.pcm);                       // --original writes decoded.pcm
    assert_eq!(written.len(), container.sizes[entry.index] as usize);
    if decoded.encoding == sound::Encoding::Pcm8 {
        assert_eq!(written, container.payload(entry.index)); // only 8-bit is the raw payload
    }
}
```

**`known_files_have_the_expected_playback_format`** (on `app.MMA` / `HD.MMA`):

```rust
// Ball30sec.AIF — 28.8 s of stereo 22.05 kHz only with rate code 1 + the stereo bit
assert_eq!(decoded.format.sample_rate, 22_050);
assert_eq!(decoded.format.channels, 2);
assert_eq!(decoded.format.bits_per_sample, 16);
let frames = decoded.pcm.len() / 4;
let seconds = frames as f64 / 22_050.0;
assert!((28.7..=28.9).contains(&seconds));
// silence.aif — 44100 zero samples, i.e. one second of 22050 Hz stereo
assert!(decoded.pcm.iter().all(|&b| b == 0));
// ISthumbDRAG.aif in HD.MMA
assert_eq!(decoded.format.sample_rate, 22_050);
assert_eq!(decoded.format.channels, 1);
assert_eq!(decoded.format.bits_per_sample, 8);
assert_eq!(decoded.encoding, sound::Encoding::Pcm8);
```

### 14.3 `tests/sound_order.rs` — the first game's depth override and byte order

`game1_dir()` = `$GAME1_ASSETS` or `/PathAssets/Dati` or `/RawAssets/Dati`.
`GAME1_CONTAINERS = ["HD.SND", "mpApp.SND", "pack.SND"]`.

```rust
fn low_bits_zero_share(pcm: &[u8]) -> f64 {
    let samples = pcm.len() / 2;
    if samples == 0 { return 0.0; }
    let zero = pcm.chunks_exact(2).filter(|s| (s[0] as u16 & 0x01ff) == 0).count();
    zero as f64 / samples as f64
}
```

`WRONG_DEPTH` (each entry: container, name, the depth the payload actually is):

```rust
const WRONG_DEPTH: &[(&str, &str, u16)] = &[
    ("pack.SND", "MdyIn001.AIF", 16), ("pack.SND", "MdyIn002.AIF", 16),
    ("mpApp.SND", "IahUh066.AIF", 16), ("mpApp.SND", "IahUh069.AIFF", 16),
    ("pack.SND", "Mpl__S01.aif", 8),   ("pack.SND", "Mmk_S01.aif", 16),
    ("mpApp.SND", "Smp__S03.aif", 16),
    ("pack.SND", "CreditsMusic.aif", 16), ("mpApp.SND", "Credits music.aif", 16),
    ("mpApp.SND", "UVp__M01.aif", 16), ("mpApp.SND", "UVp__M02.aif", 16),
    ("pack.SND", "CtaTlS01.aif", 8),  ("pack.SND", "CtaTsS01.aif", 8),
    ("mpApp.SND", "CUdBtS01.aif", 8), ("mpApp.SND", "ZOOMRECT.AIF", 16),
    ("HD.SND", "CQtBtS01.aif", 8),
];
const RIGHT_DEPTH: &[(&str, &str, u16)] = &[
    ("pack.SND", "MdyIn003.AIF", 16), ("pack.SND", "mmkIn001.AIF", 16),
    ("pack.SND", "MddL3015.AIF", 16), ("pack.SND", "Mdd__S01.aif", 8),
    ("pack.SND", "MCHRaS01.aif", 8),  ("HD.SND", "CPtBtS01.aif", 8),
];
```

`the_payloads_depth_wins_over_the_format_byte`:

```rust
assert_eq!(format_of(container, i).bits_per_sample, if *want == 16 { 8 } else { 16 },
           "{name}: the format byte should read the wrong depth");
assert_eq!(decoded_bits(container, i), *want, "{name}: should decode at {want}-bit");
assert_eq!(checked, WRONG_DEPTH.len());
```

(`decoded_bits` = `sound::decode(...).format.bits_per_sample`.)

`a_correctly_declared_resource_keeps_its_depth`:

```rust
assert_eq!(format_of(container, i).bits_per_sample, *want);
assert_eq!(decoded_bits(container, i), *want);
```

`a_flipped_resource_decodes_off_the_rails` — over the first 200 000 bytes of
`decoded.pcm`, the rail share (`|v| >= 0.99`, `v = i16le/32768` for 16-bit and
`(b-128)/128` for 8-bit) must satisfy:

```rust
assert!(share < 0.02, "{name}: {share:.3} of the decoded samples sit on the rails");
```

`a_sixteen_bit_resource_decodes_to_the_wire_order` (`MddL3015.AIF`):

```rust
assert_eq!(decoded.encoding, sound::Encoding::Pcm16);
assert_eq!(decoded.format.bits_per_sample, 16);
let stored = container.payload(index);
assert_eq!(stored[0], 0xff); assert_eq!(stored[1], 0xe2);
assert_eq!(i16::from_be_bytes([stored[0], stored[1]]), -30);
assert_eq!(decoded.pcm[0], 0xe2); assert_eq!(decoded.pcm[1], 0xff);   // little-endian out
assert!(low_bits_zero_share(&decoded.pcm) < 0.5);
```

`no_stored_sixteen_bit_resource_is_byte_swapped` — all 228 stored-PCM16
resources across the three containers:

```rust
assert!(checked > 0, "no stored-16-bit resources were found");
assert!(share < 0.6, "... signature of byte-swapped 16-bit audio");
```

### 14.4 `tests/game1_payloads.rs` — the LZSS decompressor

An **independent** LZSS *compressor* is written in the test from the token
layout (`((length - 3) << 10) | slot`, `slot = (WINDOW + wp - distance) % WINDOW`,
`wp` starting at 958) and used to check the decompressor:

```rust
#[test] fn match_words_stay_inside_the_encoding() {
    for length in 3usize..=66 { for distance in [1usize, 2, 512, 1023] {
        let word = ((length - 3) << 10) | distance;
        assert!(word >> 16 == 0);
        assert_eq!(u16::from_be_bytes([b0, b1]) & 0x3FF, distance as u16);
        assert_eq!((b0 >> 2) as usize + 3, length);
    }}
}

#[test] fn lzss_round_trips() {
    // cases: [], [0], [0xAB], [0xAB;1|66|67|1024|1025|3000], 0..=255,
    // (i % 7), (i % 251), three "picture-like" payloads, and 4096 bytes of LCG noise
    let expanded = v1::lzss::decompress(&compressed, case.len()).expect("decompress");
    assert_eq!(expanded, case);
}

#[test] fn lzss_boundary_lengths_and_distances() {
    for length in [3usize, 4, 5, 32, 63, 64, 65, 66] { /* round-trips */ }
    for offset in [1usize, 100, 1023, 1024, 1025, 2048] { /* round-trips */ }
}

#[test] fn lzss_rejects_a_short_stream() {
    assert!(v1::lzss::decompress(&[0x01, 0x41], 3).is_err());
    assert!(v1::lzss::decompress(&[0xFF], 1).is_err());   // 8 literals, no bytes
}
```

and the synthetic version-1 picture container tests (which exercise
`v1::picture::payload` / `v1::picture::decode` and the metadata parser):

* `synthetic_container_parses_as_version_1`: `version == 1`, kind `Pictures`,
  `resource_count == 2`, `names == ["flat", "squashed"]`,
  `uncompressed_len(0) == 0`, `uncompressed_len(1) == 4*4*3`, `palettes.len() == 0`.
* `payload_expands_only_what_is_declared`: a declared-0 payload comes back as the
  container slice; a declared one does not, and has exactly the declared length.
* `original_mode_writes_the_expanded_payload`: plain `.dat` == container bytes;
  compressed `.dat` == `lzss::decompress(stored, declared)`, length == declared,
  and `stored.len() < written.len()`.
* `both_storage_forms_decode_to_the_same_pixels`: `v1::picture::decode` of the
  plain and the compressed resource give the same RGBA (`flat_rgba(6,4)`).
* `the_cli_decoder_dispatch_is_version_aware`: `cli::picture_decoder` picks
  `v1::picture::decode` for version 1, and `cli::picture_payload` picks
  `v1::picture::payload`.
* `the_shared_payload_path_is_unaffected`: `picture::payload` (the shared, LZ77
  path) hands back the stored stream for a version-1 container.

### 14.5 `tests/game1.rs` — the version-1 containers

* `every_payload_is_a_byte_exact_slice`: `payload(i) == &data[offsets[i]..offsets[i+1]]`
  for all 8 containers (`HD.PIC`, `mpApp.PIC`, `pack.PIC`, `Topolino.CPK`,
  `HD.SND`, `mpApp.SND`, `pack.SND`, `pack.BLB`).
* `header_fields_are_measured`: counts `1682, 384, 10893, 2, 66, 152, 234, 305`;
  `be32(&data, 0x20 + 4*N) == data.len()`; `c.offsets[N] == data.len()`;
  `c.names_end == 0x20 + 4*(N+1) + 2 + 32*N`; the first four name fields match.
* `picture_metadata_lands_on_the_first_payload`:
  `be16(&data, base + 4n) == 7` (the marker), and
  `after + palettes * 1024 == c.offsets[0]` where
  `after = base + 4n + 2 + n + n + 4n + 4n + n + 2` and
  `palettes = be16(&data, base + 4n + 2 + n + n + 4n + 4n + n)`.
  Every dimension is `> 0` and `< 4096`.
* `dimensions_and_storage_forms_decode_every_resource`: 12,961 resources, zero
  failures; declared-0 payloads come back unchanged, declared ones expand to
  exactly the declared length and differ from the stored bytes, and
  `picture::pixel_format(...)` returns `Some` for every one.
* `original_mode_writes_the_expanded_payload`: `extract_pictures_original`
  writes the container slice for plain resources and the LZSS expansion for
  compressed ones, for all 12,961.
* `version_1_scripts_open_and_original_is_byte_exact`:
  `be16(&data, 0x20 + 4*(count+1)) == 4`; `c.names_end == table_end + 2`;
  `c.offsets[count] == data.len()`; `v1::scripts::parse(...)` succeeds;
  `scripts.names.iter().filter(|n| !n.is_empty()).count() >= count / 3`;
  every payload is a byte-exact slice and non-empty;
  `v1::scripts::extract_container(&c, &out_dir, true) == count`;
  each file is `<out_dir>/{i:05}_{stem}.bin` with `stem` computed exactly as
  `safe_name` does (non-[alnum-_.] → `_`, `object{i}` when empty) and holds the
  container's own bytes; total `1283`.

### 14.6 `tests/game1_blobs.rs` — the blob payloads

* `STAMP = b"\x00\x0cgoofy 2.6.3\x00"`; `pack.BLB` has 305 resources; every
  payload opens with `STAMP` and is a byte-exact slice.
* `the_stamp_is_a_length_prefixed_string_the_loader_checks`:
  `u16::from_be_bytes([p[0], p[1]]) == 0x0c` and `p[2..14] == b"goofy 2.6.3\0"`.
* `the_three_smallest_payloads_decode_through_the_readers_layout`
  (`session 1` 49, `session 2` 103, `session 3` 78):
  `p[14..30] == [00,00,00,01, 00,00,00,0c, 00,00,00,28, 17,00,00,ff]`;
  `count = be32(p[30..34])`, `size = be32(p[34..38])`, `size == count * 8`,
  `p.len() == 38 + size`; records are `{u16 a; u16 b; u32 d}` big-endian at
  `38 + 8k`; every non-last record has `d == 19`, the last has `d == 0`; and
  every record has `a > 0 && b > 0`.
* `every_payload_ends_with_a_zero_terminator`:
  `be32(&p[p.len()-4..]) == 0`.
* `every_header_carries_a_count_and_its_byte_size`:
  `be32(p[14..18]) > 0` and `be32(p[18..22]) == count * 12`.
* **`every_payload_round_trips_through_decode_and_encode`**:
  `Blob::decode(payload).unwrap().encode() == payload` for all 305 payloads;
  totals `objects == 209_751` and `records == 8_817_121`.
* `the_payload_arithmetic_closes_on_every_payload`:
  `object_bytes == object_count * 12`; `22 + object_bytes + 4 <= payload.len()`;
  the tag at `objects_end == blob.record_bytes`; `blob.record_bytes ==
  payload.len() - objects_end - 4`; `record_bytes % 8 == 0`; the objects' record
  counts sum to `record_bytes / 8`; each object's `records.len() == record_count`
  and its last record's `event == 0`.
* `every_object_kind_is_one_the_game_switches_on`:
  `(6 ..= 50).contains(&object.kind)` for every object.
* `the_reader_adds_the_stage_offset_to_both_coordinates` (`session 1`'s first
  record): `first.a == 1680`, `first.b == 720`, `first.a as u16 as i32 - 800 ==
  880`, `first.b as u16 as i32 - 800 == -80`, `first.stage_coords() == (880, -80)`,
  and `(first.a.wrapping_add(0xfce0)) as i16 as i32 == 880`.
* `a_payload_that_breaks_a_measured_relation_is_refused` — four synthetic
  mutations that must fail (and a fifth that must decode):
  1. 34-byte payload (stamp + count 1 + object_bytes 12) ⇒ the error contains
     `"object section"` or `"payload is"`.
  2. same length with `object_bytes = 24` ⇒ the error contains `"object_bytes"`.
  3. 46-byte payload (one object of kind 6, one record with event 19) ⇒ the error
     contains `"event 0"` or `"terminator"`.
  4. the same payload with the terminator in place decodes, with 1 object and 1
     record, and `blob.encode() == full`.

---

## 15. Rust-level details the Lean port must reproduce

| Rust construct | What the Lean port must do |
|---|---|
| `chunks_exact(2)` | iterates non-overlapping pairs; a trailing odd byte is **excluded** from the iterator but returned by `remainder()` |
| `Vec::with_capacity(n)` | capacity only; no semantic effect |
| `saturating_sub` | `Nat` subtraction clamped at 0 |
| `wrapping_add` on `i16`/`usize` | two's-complement wrap for `Int16`; `% 1024` for the window cursor |
| `usize` arithmetic | `Nat`; the code never relies on overflow (debug builds panic on overflow, so the shipped code is overflow-free on real data) |
| `checked_add` + `.context("…")` | the context string becomes part of the error chain, printed by `{:#}` after a `: ` |
| `anyhow::Error` `to_string()` vs `{:#}` | `to_string()` = outermost context only; `{:#}` = the whole chain joined with `: `. `extract_container`'s `report.failed` stores `to_string()`; `warn_failure` prints `{:#}` |
| `{index:05}` | zero-padded decimal, minimum width 5 |
| `{attr:02x}` | lowercase hex, zero-padded to 2 digits |
| `{at:#x}`, `{objects_end:#x}` | `0x` prefix + lowercase hex, **no** padding |
| `{kind:>3}`, `{how_many:>6}`, `{count:>6}`, `{how_many:>4}` | right-aligned decimal, minimum width |
| `{:02x?}` on `&[u8]` | `[00, 0c, …]` — square brackets, 2-digit lowercase hex per byte, `, ` separators |
| `{:?}` on `ContainerKind` | `Pictures`, `Sounds`, `Films`, `Blobs`, `Scripts`, `Models`, `Textures`, `Unknown` |
| `String::from_utf8_lossy` | invalid UTF-8 becomes U+FFFD |
| `char::is_ascii_graphic` | `0x21 ..= 0x7e` |
| `trim_end_matches('\0')` (names tag) | strips trailing NULs only |
| `trim_matches('.')` (in `cli::sanitize`) | strips leading and trailing `.`; **`safe_name` in blobs/scripts does not do this** |
| `BTreeMap` iteration | ascending numeric key order |
| `eprintln!` order | stderr only; the notes are emitted during `decode`, before the file is written |
| `f64` division | `rails as f64 / samples as f64` — a `Float` port may instead divide `Nat`s; the `0.005` margin is two orders of magnitude |

---

## 16. Where a doc disagrees with the code (the port must follow the code)

1. **`--original` for sounds writes the *decoded* stream, not the container's
   payload.** The module doc of `src/sound.rs` (lines 362–366) and
   `docs/GAME1.md` §5.1 ("The archival path is untouched: `--original` writes
   `container.payload`, not the swapped stream") both claim the payload is
   written unchanged. The code writes `decoded.pcm`:
   `std::fs::write(&path, &decoded.pcm)`. `tests/sounds.rs`
   (`original_output_is_the_decoded_payload_verbatim`) pins exactly that, and
   additionally asserts equality with `container.payload` **only** for
   `Encoding::Pcm8`. Consequence: a first-game stored-PCM16 resource's `.pcm`
   file is the byte-swapped stream. The port must do the same.
2. **`docs/SOUNDS.md` §6** says the sound "container version" dispatch is
   version-independent ("the codec and the metadata layout below are
   version-independent, which is the rule `src/sound.rs` follows"). True of
   `src/sound.rs` itself; the version differences live in `container.rs` (where
   the arrays are read from) and in the payload-driven override that only the
   first game needs. The port must keep that split: `sound.rs` has **no version
   branch at all**.
3. **`Cargo.toml`'s comment** still names `ffmpeg` as the encoder ("`src/sound.rs`
   shells out to `ffmpeg`"). The code shells out to `flac`; see §11.
4. `docs/GAME1.md` §6 says "the blob prologue is 38 bytes for every payload" is
   **disproved** as a general rule — the object section length varies, so a port
   must derive `objects_end = 22 + object_bytes`, never hard-code 38.
   (`tests/game1_blobs.rs` still uses 38 for the three smallest payloads, where
   it happens to hold.)
5. `docs/GAME1.md` §7 says "`pack.mms` has 210 payloads whose lead is 14" while
   `src/v1/scripts.rs`'s doc comment says 211; both are prose about a
   distribution and neither is asserted — the port must compute the counts
   itself.

---

## 17. Implementation checklist for the Lean port

1. `MmfExtract.Sound.DPCM_TABLE` — 256 `Int16`s, §3.2, verified against
   `docs/SOUNDS.md` §4.
2. `Sound.Encoding`, `SoundFormat`, `DecodedSound`, `ExtractedSound`,
   `ExtractReport` — §2.4.
3. `format_of`, `swap16`, `rail_fraction`, `implied_bits`, `decode`,
   `decode_dpcm`, `output_stem`, `warn_failure`, `extract_container` — §4,
   §3.1, §3.7.
4. `WAV` writer for the `flac` stdin — §12 (44-byte `PCMWAVEFORMAT` header, data
   chunk identical to `decoded.pcm`).
5. `flac` subprocess — §11 (argv, stdio, error strings, no WAV fallback).
6. `MmfExtract.V1` — `uncompressed_len`, `palette_of`, `palette_bank`,
   `palette_count`, `palette_bytes` — §5.
7. `MmfExtract.V1.Lzss.decompress` — §6.
8. `MmfExtract.V1.Metadata.parse` + `V1Metadata` — §7 and §2.5.
9. `MmfExtract.V1.Picture.payload` / `decode` / `pixel_format` — §8.
10. `MmfExtract.V1.Blobs` — §9 (constants, `Record`, `Object`, `Blob`,
    `extract_container`, `blob_json`, `json_escape`, `safe_name`, manifest).
11. `MmfExtract.V1.Scripts` — §10 (`V1_SCRIPT_VERSION`, `V1Scripts`, `parse`,
    `extract_container`, `trailing_name`, `is_printable`, `safe_name`, manifest).
12. Port the assertions of §14 as Lean tests (the unit tests of §14.1 and the
    synthetic-container tests of §14.4/§14.6 run without the game's assets; the
    asset-dependent ones skip when the folder is absent).
