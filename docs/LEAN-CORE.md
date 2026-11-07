# The Lean extraction core — architecture, data structures and algorithms

This document describes `mmf-extract/lean/` so that it can be worked on, split, or
audited **without reading any other file first**. It covers the boundary with the Rust
crate, every module in the core, the data structures each one declares, and the
algorithm each decoder implements.

For the container *formats* themselves see the per-game chain (`docs/PICTURES.md`,
`docs/SOUNDS.md`, `docs/SCRIPTS.md`, `docs/FILMS.md` for the shared version-2/3
formats, `docs/SEQUEL.md` for the second game's deltas, `docs/GAME1.md` for version 1,
and `docs/DMA3D.md` for the third game's 3D half). For the porting specifications that
were used to write this code, see `mmf-extract/lean/PORT-REST.md` and
`mmf-extract/lean/PORT-SOUND.md`.

---

## 1. The two-halves split

`mmf-extract` is one program in two languages:

| half | what it owns |
| --- | --- |
| **Rust** (`src/`) | the command line, argument parsing, the FLAC/WAV subprocess wiring, and the *reference* decoders the Lean core is checked against |
| **Lean** (`lean/`) | every parser that reads an `MMFW` container and every decoder that turns a resource into a modern format |

The Rust half calls the Lean half through **exactly two C-ABI functions**, both
exported from `MmfExtract/FFI.lean`:

```lean
@[export mmf_extract]   unsafe def extract (request : String) : String
@[export mmf_index]     unsafe def index  (request : String) : String
```

They take a string and return a string. `src/lean.rs` declares the `extern "C"`
signatures and is the only Rust file that names them; `src/main.rs` and `src/cli.rs`
are the only callers.

### 1.1 The request protocol

`mmf_extract` takes a line-oriented request, `key` TAB `value`:

```text
asset_dir	/RawAssets
output	/workspace/extracted
types	pictures,sounds,scripts
original	0
raw	0
```

`Extract.parseRequest` parses it into `Extract.Options`. An unknown key is an error, so
a caller's typo cannot silently extract nothing. `types` accepts `all` plus the six
container-kind names and their common synonyms; an unknown type name throws.

`mmf_index` takes the same shape with three keys — `base`, `texture_dir`,
`model_dir` — and writes the 3D game's `index.html`.

### 1.2 The report protocol

Both functions return the report text that `src/cli.rs` prints verbatim:

```text
pictures	30404
sounds	683
films	1923
scripts	...
blobs	...
models	1165
ambiguous	696
total	1165
failed	2
fail	<path>: <message>
```

A request that cannot be parsed returns `error\t<message>\n` instead of crashing, so
the CLI exits the way it does for its own errors.

### 1.3 Why the boundary is a string and not a struct

The Rust build (`build.rs`) compiles the Lean lake project to
`libmmfextract.so`, generates the extern signatures and links against it. A string in
and a string out keeps the ABI to one `char *`, which means the Rust side never needs
to know a Lean type's layout and the Lean side never needs a `#[repr(C)]` mirror.

---

## 2. The module map

```
lean/src/MmfExtract.lean        root import
lean/src/MmfExtract/
  Types.lean       shared type aliases (ByteArray helpers, UInt16/32)
  Bytes.lean       little-endian readers with bounds errors
  Path.lean        output-path construction, shared with the Rust writer
  FFI.lean         the two exported entry points (§1)
  Container.lean   the single MMFW container parser (all kinds, all versions)
  Extract.lean     Selection/Options/Report, and the run loop
  Picture.lean     8bpp indexed, RLE and mask picture decoders
  Lz77.lean        the shared version-2/3 LZ77 decompressor
  V1.lean          the first game's version-1 entry point
  V1/Lzss.lean     version-1 LZSS decompressor
  V1/Metadata.lean version-1 metadata block reader
  V1/Picture.lean  version-1 picture decoder
  Script.lean      script payload classifier and writers
  Film.lean        film chunk reader and writers
  Sound.lean       sound format detection, DPCM decode, FLAC invocation
  Png.lean         the PNG encoder
  Dma3d.lean       the 3D game's run loop and writers
  Dma3d/Modfile.lean  the seven model-payload readers
  Dma3d/Texfile.lean  the texture readers
  Dma3d/Verify.lean   the --original verification harness
```

`tests/lean_parity.rs` is the parity harness: it runs each Rust decoder and each Lean
decoder over the same shipped bytes and asserts they agree exactly — byte-identical
sounds, pixel-identical pictures, identical script documents.

---

## 3. `Types.lean`, `Bytes.lean`, `Path.lean` — the shared floor

### 3.1 `Bytes.lean`

Every multi-byte read in the core goes through this module. The invariant it enforces
is the one `AGENTS.md` calls *payload length is ground truth*: a read that would run
past the end returns a named error rather than a panic or a zero.

```lean
def u16At (d : ByteArray) (off : Nat) : Except String UInt16
def u32At (d : ByteArray) (off : Nat) : Except String UInt32
def f32At  (d : ByteArray) (off : Nat) : Except String Float
def u32BEAt / u16BEAt ...                    -- the same, big-endian
```

`f32At` is `IEEE-754` bit-cast, not a numeric conversion, so it reproduces whatever
the payload holds including denormals and signed zeros.

### 3.2 `Path.lean`

Two helpers the whole output layer shares:

* `Path.padIndex n` — the zero-padded five-digit index in `{index:05}_` file names
* `Path.sanitize name index` — a resource name made safe for a file name, falling
  back to the index when the name carries no usable character

Both halves must produce the same name, so the Rust extractor re-implements them
bit-for-bit and `lean_parity.rs` asserts the strings match.

---

## 4. `Container.lean` — one parser for every container

This is the module a container-shaped change touches. It parses **all six kinds**
(`Pictures`, `Sounds`, `Films`, `Scripts`, `Blobs`, `Models`, `Textures`) and **all
three versions**, with no per-kind fork.

### 4.1 Data structures

```lean
inductive ContainerKind | pictures | sounds | films | scripts | blobs | models | textures | unknown

structure ScriptMeta where
  elementCount : Nat
  elementSize  : Nat      -- 15 * N: {u16 height, u16 width} pairs
  typeCount    : Nat
  payloadStart : Nat
  ...

structure Container where
  kind        : ContainerKind
  version     : Nat        -- 1, 2 or 3
  bigEndian   : Bool       -- read from the header, never assumed
  resourceCount : Nat
  declaredCount : Nat
  offsets     : Array Nat
  names       : Array String
  attr flags dimensions sizes kinds : ...
  extraU32    : Array Nat
  palettes uncompressedSizes : Array Nat
  payloadStart namesEnd : Nat
  ...
```

`Container.payload i` is the only accessor a decoder should use: it slices
`data[offsets[i]..offsets[i+1]]` and errors when the offsets disagree, which is what
keeps a corrupt entry from becoming a panic.

### 4.2 The parse algorithm

1. **Kind by tag.** `kindOfTag` reads the four bytes at `0x0C` (`PIC`, `SND`, `FIL`,
   `MMS`, `BLB`, `MMM`, `TEX`) and maps to `ContainerKind`. This is the dispatch
   `AGENTS.md` names as the single place a kind is decided.
2. **Endianness.** `endianMarker` reads the two bytes at `0x12`: the shared
   version-2/3 containers differ from version 1 only here, and the marker decides
   every subsequent read's direction. No decoder contains an endian branch.
3. **Version.** From the same header, `1`, `2` or `3`.
4. **Offset table.** `payloadBoundaries` walks the `u32` array from `0x20` (version
   1) or `0x24` (versions 2/3), keeping only entries that are strictly increasing and
   inside the file. The result is both the resource count and the payload extents.
5. **Names.** `namesTable` reads `32 * n` NUL-padded ASCII slots.
6. **Kind-specific metadata.** Pictures read a dimension table, sounds a format
   table, scripts the `ScriptMeta` block. Version 1 has its own layout and is
   delegated to `V1/Metadata.lean`.

The measured container-level facts (offsets, sizes, the per-kind metadata regions) are
pinned by `tests/dma3d.rs::the_metadata_regions_are_measured` and
`tests/lean_parity.rs`.

### 4.3 The version-1 delegation

Version 1 does not branch inside `Container.parse`; it calls into `V1.*`. That is the
worked example `AGENTS.md` gives for "a container whose layout differs is its own
module reached through one dispatcher".

---

## 5. The decoders

### 5.1 `Picture.lean`

**Data structures.** `PixelFormat` (an inductive over the storage forms), `Decoded`
(the RGBA byte plane plus its width and height).

**Algorithm**, in the order `parse` tries them — *payload length decides first, the
flags byte is only a hint*:

1. **Mask.** A payload with no palette is a coverage mask; `maskRgba` expands each
   index byte to `[0,0,0,alpha]`.
2. **Paletted.** `indexedRgba` walks the palette at 4 bytes per entry with a stride of
   3` (packed RGB) or 4, expanding each index byte.
3. **RLE.** `locateRleTable` measures where a row-offset table could sit by checking
   that the candidate entries are monotone and that `rowsLineUp` finds the row data
   contiguous behind them; `decodeRleRows` then decodes each row's run-length stream.
4. **Two-section RLE** (`docs/PICTURES.md` §7.3). A payload whose first section is a
   row table and whose second is a run stream, used by 105 game-3 and 1,181 game-2
   pictures that the single-section reader rejects.

`pixelFormat` decides the case from the payload's own length and the container's
dimension table, not from the `flags` byte.

### 5.2 `Lz77.lean`

The shared version-2/3 decompressor. The window is 4 KiB, the flag byte precedes each
group of eight tokens, and a back-reference emits `length` bytes from
`window_end - distance`. Bounds are checked against the output length so a corrupt
distance cannot read outside the window.

### 5.3 `V1/*`

`V1/Lzss.lean` is a different compressor from `Lz77.lean` — a 4 KiB window with a
12-bit length/4-bit distance split and a separate literal/back-reference flag bit —
and `V1/Metadata.lean` reads the `15*N + 4`-byte block that holds the
`{u16 height, u16 width}` pairs the first game's pictures need and which made 12,935
of 14,026 pictures fail before it was found (`docs/GAME1.md`).

### 5.4 `Script.lean`

**Data structure.** `Layout` an inductive over the script payload shapes the game
ships (string tables, animation brush tables, text lists, records), with
`classify` choosing one from the container's `ScriptMeta` block rather than from the
payload's bytes alone. Each layout has its own writer: `writeStringTable`,
`writeAnimBrushes`, `writeTextList`, `writeRecords`, plus the JSON+text form the
scripts decoder emits (`docs/SCRIPTS.md`).

### 5.5 `Film.lean`

**Data structures.** `Chunk` and `AudioChunk`, with the chunk `type` tags as named
`UInt16` constants (`typeAudio 0x02`, `typeMarker 0x03`, `typeTimestamp 0x06`,
`typeEnd 0x07`, `typeFullFrame 0x0A`, `typeFullFrameV2 0x00`, `typeDelta 0x0B`,
`typeDeltaV2 0x01`) and `maxRecordLen 26`.

**Algorithm.** `isFilmV2` reads the container version and picks the frame tags from
it, so the two frame encodings are one reader with its tag set selected rather than two
readers. Full frames are stored; delta frames are XOR-applied onto the running frame.

### 5.6 `Sound.lean`

**Data structures.** `Format` (an inductive), `SoundFormat`, `DecodedSound`.

**Algorithm.** `formatOf` reads the container's format table, `dpcmTable` holds the
4-bit DPCM expansion steps, and `measureSamples`/`railLevel`/`railMargin` drive the
clipping check that decides whether a stream rails. The FLAC encoding itself is the
external `flac` CLI (`flacTool "flac"`, `flacCompression "-8"`) invoked as a
subprocess, because no Rust crate here encodes FLAC and ffmpeg's encoder cannot do
8-bit (`docs/SOUNDS.md` §6.1).

### 5.7 `Png.lean`

A minimal PNG encoder: the signature, one `IHDR`, one `IDAT` with the per-scanline
filter byte `0`, and the `IEND`. It exists so the picture and film decoders have no
image-codec dependency.

---

## 6. `Dma3d/Modfile.lean` — the seven model readers

The 3D game's `Models` container ships 460 payloads. No single reader covers them; a
**dispatch chain** does, in this order, and the first reader that accepts a payload
writes it. Against the shipped container the chain converts **425 of 460** payloads to a
modern form and writes the remaining 35 through as `.mod`, each reported per resource
rather than skipped:

| # | reader | structure | what it reads |
| --- | --- | --- | --- |
| 1 | `parseMesh` | `Mesh` | a `mod II` record stream whose constant slot is bit-identical, read as a glTF mesh |
| 2 | `parse` | `Profile` | a `mod II` stream whose constant slot is bit-identical and whose other three slots are planar, read as a glTF line loop |
| 3 | `parseMagicRecord` | `MagicRecord` | a `mod II` stream neither of the above claims — written as JSON+text |
| 4 | `parseParamTable` | `ParamTable` | small values at alternating byte positions with ≥80% zero fill — JSON+text |
| 5 | `parseNameTable` | `NameTable` | NUL-terminated texture names in 32-byte slots — JSON+text |
| 6 | `parseValueStream` | `ValueStream` | an `f32` run found *anywhere* in the payload with no constant slot — JSON+text |
| 7 | `parseIndexTable` | `IndexTable` | byte values at alternating positions with ≥90% fill and values above the parameter reader's 128 rule — JSON+text |

### 6.1 Why the chain is ordered this way

The order is the shape's specificity, most specific first, and each step's rejection
reason is a *measured* boundary rather than a preference:

* the mesh and profile readers need the `mod II` magic plus a bit-identical constant
  slot, and the profile reader additionally requires the marker to be exactly `0.0`
  or `1.0` — measured over the 213 accepted profiles, only those two bit patterns
  occur;
* the value-stream reader requires a run of ≥16 values with a spread > 0.519 —
  measured over the 112 accepted streams, the minimum run is 16 and the minimum
  spread is 0.519;
* the parameter reader requires ≥80% of one parity zero *and* ≥95% of the other below
  128 — measured over the nine accepted tables, 0–3% of values reach 128;
* the index reader requires ≥90% fill, ≥90% nonzero, and reads the population whose
  values exceed that 128 rule.

Every one of those thresholds is stated in the code with the measurement in a doc
comment, and `docs/DMA3D.md` §11.53 records the calibration that produced them. A
reader that would need its threshold moved to fit more payloads is not written.

### 6.2 The run-anywhere rule

`parseValueStream` finds its longest run **anywhere** in the payload, not from offset
zero. `r19 nter.mod` and `r156` open with an ASCII region (`00 53 00 55 00 54 00 31`,
a byte and a zero alternating) whose bytes read as `f32` denormals, which the
plausibility rule rejects; a reader that started at zero stopped there and never
reached the coordinate data behind it. Resetting the run instead of ending the search
is what converted 73 payloads (`docs/DMA3D.md` §11.52).

### 6.3 The name-table slice rule

`parseNameTable` accepts a name the container's slice cut off: a printable run that
reaches the payload's end with no NUL after it. 8 of the 19 tables the reader already
acceptes have that shape, so the sliced tail is a property of the format. The cut
names are recorded in a separate `truncated` field and written under their own JSON
key, so no reader of the output can mistake `createBtSelect.38.tg` for a whole name.

---

## 7. `Dma3d.lean`, `Dma3d/Texfile.lean`, `Dma3d/Verify.lean`

### 7.1 The 3D run loop

`extractModels` and `extractTextures` walk the container and write, per resource,
either a modern file or the original bytes. `extractGltf` is the model converter and
`writeIndex` the browsable `index.html`. `Extract3DReport` counts by output family so
the run can prove every payload is accounted for — `tests/dma3d.rs` asserts
`meshes + profiles + records + params + names + streams + index + passthrough ==
resource_count`.

### 7.2 The writers

Every non-glTF output is a JSON+text document with the same four keys — `name`,
`what this is`, `meaning`, `measured` — where `meaning` says plainly what is *not*
determined and `measured` carries only what the bytes show. Five JSON+text writers
exist — `writeRecordJson`, `writeParamJson`, `writeNameJson`, `writeStreamJson`,
`writeIndexJson` — beside the glTF writers and `writeIndex` for `index.html`. Each has
a unit test that reads its own output back, because **four times** this session a writer
produced a document its reader rejected (`docs/DMA3D.md` §11.39 twice, §11.54, §11.55;
the full record is `docs/MISTAKES.md` §2.1).

### 7.3 The texture readers

`parseMask`, `parsePaletted` and the dispatch `parse`. The palette/index split is read
from the payload's own bytes; where the split is not pinned, the smallest admissible
one is read and the run counts the resource in `ambiguous` rather than silently
choosing.

### 7.4 `Verify.lean`

The `--original` harness. It reads back what was written, base64-decodes the embedded
payload from each glTF's data URI (`base64Decode`, `uriPayload`), and asserts the
bytes match the container slice. `checkOriginal` returns a per-resource result and the
harness reports the count of byte-exact files; the third game's **705 textures** and
**460 models** are both verified byte-exact here, which is the claim that every payload
the game ships is recoverable byte-for-byte even where a modern form does not exist yet.

---

## 8. How the two halves stay in agreement

1. **`tests/lean_parity.rs`** runs both halves over the same bytes and asserts equality.
   This is the only thing that is allowed to say the port is correct.
2. **`lean/test/Test.lean`** is the Lean-side harness and re-implements the byte-level
   point checks independently of the Rust tests.
3. **Every new decoder ships with a `--original` test** (`AGENTS.md`): write the
   original bytes and assert byte-equality against the container slice, and assert the
   modern output decodes back to the same pixels or samples.
4. **The Rust half is the writer of modern models.** The Lean half *counts* records,
   profiles, tables and streams and reports the payloads that fit no reader, but does
   not write the modern model files. Writing from both halves produced a double count
   (`models: 1216` where the converter writes 1165) and that mistake is recorded in
   `docs/DMA3D.md` §11.26 and §11.38.

---

## 9. Building and running the Lean half alone

```sh
sh build/build.sh          # compile the lake project to .lake/build/lib
LEAN_PATH=.lake/build/lib/lean lean --run test/Test.lean
```

`cargo build` from `mmf-extract/` does the same via `build.rs`. No network access is
needed; the toolchain is pinned in `lean-toolchain`.
