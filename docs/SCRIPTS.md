# \*\*\*\*\*\*'s Magic Artist Deluxe — MMFW Scripts Format

Reverse-engineered notes for the `MMFW Scripts` container (`.MMS`) shipped in
\*\*\*\*\*\*'s Magic Artist Deluxe (Italian release: \*\*\*\*\*\* Magico Artista,
2002). Everything below was derived from the game's own executable
(`DMA4.exe`) and verified against `/RawAssets/HD.MMS`; there is only one
Scripts container in the shipped assets.

> **Which game this covers.** Container versions 2 and 3, whose Scripts
> containers differ only in tag spelling — `"MMFW Scripts"` versus
> `"MMFW 3 Script"`. The object database documented here is the shape
> `src/script.rs` decodes. The first game's version has **no equivalent
> region**: its Scripts containers carry a different, still-undecoded layout,
> so they are reported as failures rather than guessed at. That gap is
> recorded in **`GAME1.md`** §7.

> **Engine note.** Despite the `MMFW` signature this is **not a Clickteam
> Multimedia Fusion 2 application**. `DMA4.exe` carries the PDB path
> `C:\Projects\DeluxeRevision\DMA Art Ware\Intermediate\Release\DMAAW.pdb`,
> was linked with MSVC 6 on 2002-06-15, and its loader errors are prefixed
> `MME_*`. There is no `Clickteam`, `CNC`, or `MMF2` string in the binary.
> `MMFW` is the game's own container signature. External format descriptions
> were treated only as hypotheses and are not relied on here.

A Scripts container is **not event bytecode**. It is a serialized global
object database: every resource is one named object, and the metadata names
that object's class. Each payload starts with `element_count` fixed-size
records; some classes append string data after that fixed prefix.

---

## 1. Container header

`HD.MMS` is 3,477,251 bytes and uses the little-endian `"II"` marker and
container version 3.

```text
0x00  16 bytes  "MMFW " + tag, NUL padded   (tag = "Scripts")
0x10   2 bytes  endian marker: "MM" (big-endian) or "II" (little-endian)
0x12   2 bytes  u16 version, must be 3
0x14   6 bytes  file id
0x1A   8 bytes  timestamp (f64)
0x22   2 bytes  u16 resource count N
0x24  4*(N+1)  u32 offsets; resource i spans [off[i], off[i+1])
```

For `HD.MMS`: `N = 4813`, `off[0] = 0x24004`, `off[4813] = 0x350f03`.
There is **no padding and no 32-byte names table** after the offset table.
The serialized object database starts immediately at
`0x24 + 4 * (N + 1) = 0x4b5c`.

Payloads are stored **uncompressed**. The offset table is the only size
information, and the loader computes a resource's length as
`off[i+1] - off[i]`:

| function | role |
| --- | --- |
| `FUN_004c23ed` | generic container open: checks signature and version 3, selects byte order, reads the offset table |
| `FUN_004c22c1` | returns resource `i`'s payload offset |
| `FUN_004c22d2` | returns resource `i`'s payload length (`off[i+1] - off[i]`) |

Consequently `--original` writes the stored bytes verbatim; no LZ77 or other
decompression step applies to this container type.

## 2. Object-database metadata

The metadata is a single serialized block. All integers use the container's
byte order.

```text
u16      script metadata version (11)
u16      unknown_1 (0xffff)
u16      unknown_2 (0xffff)
u16      unknown_3 (2490)
u16      type count T (89)
T x u16  type_aux: fixed record size per class
         string list: T type names
N x u32  element_count per object
N x u16  type index per object
         string list: N object names
```

The object-name list ends exactly at the first payload offset (`0x24004`),
which is what pins the field boundaries down.

### 2.1 Exact locations in `HD.MMS`

| field | offset | size / value |
| --- | ---: | --- |
| metadata version | `0x4b5c` | 2 bytes, `11` |
| `unknown_1` | `0x4b5e` | 2 bytes, `0xffff` |
| `unknown_2` | `0x4b60` | 2 bytes, `0xffff` |
| `unknown_3` | `0x4b62` | 2 bytes, `2490` |
| type count | `0x4b64` | 2 bytes, `89` |
| `type_aux` | `0x4b66` | 89 x u16 (178 bytes) |
| type-name list | `0x4c18` | 1,847 bytes |
| element counts | `0x534f` | 4,813 x u32 (19,252 bytes) |
| type indices | `0x9e83` | 4,813 x u16 (9,626 bytes) |
| object-name list | `0xc41d` | 97,255 bytes |
| metadata end | `0x24004` | first payload |

The block is 128,168 bytes. The loader path for it is `FUN_004c32b2`:

1. call the generic header opener (`FUN_004c23ed`);
2. read and check the metadata version (`11`);
3. read the three unknown u16 fields;
4. read the type count and allocate the per-type record-size vector;
5. read `type_count` u16 record sizes;
6. read the type-name string list;
7. read `N` u32 element counts, then `N` u16 type indices;
8. read the object-name string list.

The accessors used later are `FUN_004c30b7` (record size for a type index),
`FUN_004c30c9` (element count for an object) and `FUN_004c30da` (type index
for an object).

### 2.2 String-list wire format

Both metadata string lists and the payload strings described below use the
same length-prefixed shape:

```text
u16      count
u32      blob length
count x u32  offsets, relative to the start of the blob
blob     NUL-terminated strings
```

For `HD.MMS`:

* type names: 89 entries, 1,485-byte blob;
* object names: 4,813 entries, 77,997-byte blob.

The object-name list is the only name source for a Scripts container; the
generic `Container::names` accessor is populated from it.

## 3. Payload model: fixed records plus an optional tail

For object `i`, let:

* `count = element_counts[i]`;
* `type = object_types[i]`;
* `record_size = type_aux[type]`.

Every payload begins with `count * record_size` bytes of fixed records. In
`HD.MMS` this prefix is always present and never exceeds the payload length.
The loader's object-loading path (`FUN_004a30fa`) uses the same split: it
allocates the payload, stores `count` and `record_size`, and byte-swaps the
fixed prefix as 16-bit words when the reader reports big-endian data. All 89
`type_aux` values are even (2–48), so the prefix always divides into u16 words.

Some classes append class-specific data after the fixed prefix. Across all
4,813 objects:

| layout | objects | tail bytes | tail contents |
| --- | ---: | ---: | --- |
| fixed records only | 1,813 | 0 | — |
| Animation | 2,783 | 37,636 | one NUL object-name string |
| creditsTextEntry | 155 | 2,829 | one NUL text string |
| StdBrush | 46 | 2,847 | `count` NUL brush-name strings |
| AnimBrush | 12 | 1,024 | `(u16 ref, u16 name_offset, u32 flags)` entries + name blob |
| TextList | 3 | 40 | `count` u32 offsets + string blob |
| StringTable | 1 | 2,417 | `(u16 id, u32 offset, u32 zero)` entries + string blob |
| **total** | **4,813** | **46,793** | |

Measured byte coverage:

```text
payload bytes       3,329,791
fixed record bytes  3,282,998
tail bytes             46,793
opaque bytes                0
```

Every payload is therefore accounted for structurally; there are no unknown
blob resources in this container.

### 3.1 Generic records

For classes without a decoded tail, the extractor prints each record's raw
bytes and, where the record size divides naturally, its u16/u32 fields. Those
field displays are a viewing aid, not a recovered schema. The only class with
a visible opcode-like structure is `Animation`:

* record size 10 bytes = five u16 values;
* the first u16 takes small values (observed 0–14, mostly 0–7);
* the remaining four u16 values look like arguments (including negative
  values such as `0xfff5`).

Example (`ISbigBALa`, object 187, first six of 197 records):

```text
00 00 00 00 00 00 00 00 00 00   op 0 args 0 0 0 0
01 00 01 00 00 00 00 00 00 00   op 1 args 1 0 0 0
02 00 00 00 00 00 00 00 00 00   op 2 args 0 0 0 0
03 00 00 00 00 00 00 00 00 00   op 3 args 0 0 0 0
05 00 01 00 53 00 00 00 00 00   op 5 args 1 83 0 0
06 00 00 00 01 00 14 00 f5 ff   op 6 args 0 1 20 65525
```

The name `op` is an extractor label only; the opcode table and argument
semantics have not been recovered.

## 4. Class-specific tails

### 4.1 Animation, creditsTextEntry, StdBrush

These tails are plain NUL-terminated strings:

* `Animation`: exactly one string, the object's own name; it normally equals
  the metadata object name.
* `creditsTextEntry`: exactly one string, the displayed credit text (may be
  empty, e.g. `firstBlank`).
* `StdBrush`: exactly `count` strings, the brush names, in record order.

`StdBrush` records are 48 bytes each. Example: `FineSpray` (object 1384) has
5 records (240 bytes) followed by the five names `FineSpray 1` …
`FineSpray 5`.

### 4.2 AnimBrush

`AnimBrush` records are 8 bytes:

```text
u16  reference object index
u16  name offset (relative to the name blob)
u32  flags (observed 0 and 0x10000)
```

The tail is a blob of `count` sequential NUL names. The name offsets are
sequential and the blob ends immediately after the last string. The reference
points at another object in the same container (for example the `AnimBrush`
named `WhippedCream` references objects 1389–1393).

Example (`WhippedCream`, object 1393, 5 records):

```text
6d 05 00 00 00 00 00 00   ref=1389 offset=0x0000 flags=0x0 -> "Whipped Cream1"
6e 05 0f 00 00 00 00 00   ref=1390 offset=0x000f flags=0x0 -> "Whipped Cream2"
6f 05 1e 00 00 00 00 00   ref=1391 offset=0x001e flags=0x0 -> "Whipped Cream3"
70 05 2d 00 00 00 00 00   ref=1392 offset=0x002d flags=0x0 -> "Whipped Cream4"
71 05 3c 00 00 00 00 00   ref=1393 offset=0x003c flags=0x0 -> "Whipped Cream5"
```

### 4.3 StringTable

`HD.MMS` has one `StringTable`, object 0 `localizationStrs`: 130 records of
10 bytes, followed by a 2,417-byte string blob (3,717 bytes total).

```text
u16  id
u32  offset into the shared blob
u32  reserved (always 0 in this file)
```

Properties verified against the file:

* ids are 0–129, unique;
* entries 42–44 use offset sentinels `0xffff`, `0xfffe`, `0xfffd` instead of
  strings — these three entries have no text;
* strings may share suffixes; for example several blueprint-warning entries
  point into the middle of a longer string;
* ids 118–120 are stored in the order 120, 118, 119, so record order is not a
  reliable proxy for id order;
* the 127 resolvable entries cover the blob completely (no unreferenced
  bytes), and the final string ends at blob offset 2,417.

Example records:

```text
id=0  offset=0x0000 -> "Too many blueprints!  Render or remove some existing ones first."
id=1  offset=0x0041 -> "Too many blueprints!  Render or remove some existing ones first. (Some may be on the other half of the canvas.)"
id=8  offset=0x0109 -> "\xa9 ******"
id=15 offset=0x017c -> "****** Interactive"
```

### 4.4 TextList

`TextList` payloads contain `count` u32 offsets followed by a shared
NUL-string blob. The offsets are relative to the start of the blob. The three
objects in `HD.MMS` are:

| object | name | count | offsets | strings |
| ---: | --- | ---: | --- | --- |
| 4810 | `creationsPath` | 3 | `0, 3, 10` | `..`, `Common`, `Creations` |
| 4811 | `commonDataPath` | 3 | `0, 3, 10` | `..`, `Common`, `Data` |
| 4812 | `appDataPath` | 1 | `0` | `data` |

Example: object 4810 is 32 bytes —

```text
00 00 00 00  03 00 00 00  0a 00 00 00   "..\0Common\0Creations\0"
```

The earlier idea that this class had a `reserved/count/length` header was
wrong: the first u32 is simply the first string offset, which is normally 0.

## 5. Type table

The container defines 89 classes, but only a handful carry most objects.
The largest classes are:

| type index | name | record size | objects |
| ---: | --- | ---: | ---: |
| 0 | Animation | 10 | 2,783 |
| 35 | DMAStampGroup | 10 | 299 |
| 80 | ModifierVisual | 10 | 276 |
| 34 | DMAStampRenderingData | 10 | 269 |
| 32 | DMAAnimationList | 2 | 173 |
| 43 | creditsTextEntry | 12 | 155 |
| 54 | AnimBrushSet | 10 | 106 |
| 51 | ToolSounds | 10 | 74 |
| 14 | DMAPictAtts | 8 | 73 |
| 33 | DMAStampRowList | 10 | 66 |
| 42 | StringTable | 10 | 1 |

The full 89-entry table is in Appendix A. `record_size` is the per-element
fixed size from `type_aux`; it is not a schema description.

## 6. Losslessness and extractor output

The extractor is `mmf-extract` in `/workspace/mmf-extract`.

### `--original`

One file per object:

```text
{index:05}_{sanitized-name}.bin
```

The file contains exactly the bytes in `[off[i], off[i+1])`. This is
byte-for-byte identical to the container slice and to the payload the loader
allocates, because Scripts payloads are not compressed.

### Modern (default)

The output directory is `Scripts/HD/` and contains:

```text
container.json          header, metadata, coverage, type histogram, object index
types.txt               all 89 type rows: index, record size, object count, name
strings.txt             type names, object names, decoded payload strings
objects/{index:05}_{name}.txt
                        one annotated dump per object
```

Each object dump states the object name, type index/name, element count and
record size, payload offset/size and chosen layout, then prints every fixed
record with raw bytes and decoded fields. Tails are decoded into strings or
string-table entries; every remaining byte is shown in an annotated hexdump.
The dump ends with an assertion-style line such as
`# All 3717 payload bytes are represented above.` Objects with no recognised
structure fall back to a full hexdump (`opaque` layout); `HD.MMS` has none.

The modern format is a faithful **text disassembly**, not a re-serializable
binary format. Re-serialization is not attempted; `--original` is the
lossless path.

## 7. Tests

`tests/scripts.rs` contains three tests, mirroring the picture-container
round-trip style:

1. **`original_script_payloads_are_byte_identical`** — extracts all 4,813
   objects with `--original`, asserts one file per object, and compares each
   file both with `Container::payload(i)` and with the exact
   `[off[i], off[i+1])` slice of the on-disk container. It also asserts
   `plain_len == payload_len` for every object.
2. **`script_metadata_accounts_for_every_payload_byte`** — pins the metadata
   counts and version, checks every `count * record_size` prefix fits its
   payload, checks all non-tail classes are exact record arrays, and asserts
   the measured totals (3,329,791 payload = 3,282,998 records + 46,793 tails).
3. **`modern_script_output_is_structured_and_complete`** — extracts the modern
   corpus, checks `opaque_bytes: 0`, the 89-row type table, representative
   strings from `StringTable`, `TextList` and `AnimBrush`, and the decoded
   `localizationStrs` dump.

Run with:

```sh
cd /workspace/mmf-extract
cargo test --release
./target/release/mmf-extract /RawAssets -o /workspace/extracted --types scripts
```

The real extraction reports `scripts: 4813`, `total: 4813`.

## 8. Open questions

* `unknown_1 = 0xffff` gates an optional codepage conversion of the string
  tail in `FUN_004a30fa`; `0xffff` disables it in this build.
* `unknown_2 = 0xffff` and `unknown_3 = 2490` have no established meaning.
  The loader reads them into the runtime object but no consumer was decoded.
* The `Animation` first u16 is opcode-like, but no opcode table or argument
  meaning has been recovered.
* Records of the other 88 classes are printed as raw u16/u32 fields; only the
  tail layouts documented above are structurally decoded.
* `StringTable` sentinel values `0xfffd`/`0xfffe`/`0xffff` are treated as
  “no string”; their individual meanings are not distinguished.
* The `AnimBrush` `flags` values (`0` and `0x10000`) are recorded but their
  semantics are unknown.

---

## Appendix A — complete type table

| idx | name | record size | objects |
| ---: | --- | ---: | ---: |
| 0 | Animation | 10 | 2783 |
| 1 | DMAPicture | 10 | 7 |
| 2 | DMA2PictureButton | 14 | 21 |
| 3 | RUPictureScrollBarVisual | 34 | 2 |
| 4 | RURadioButton | 30 | 1 |
| 5 | Dialog2PictureButton | 18 | 3 |
| 6 | DialogElements | 10 | 3 |
| 7 | AWPointStruct | 4 | 15 |
| 8 | UUBoxList | 8 | 0 |
| 9 | TextList | 4 | 3 |
| 10 | DMAUUPointList | 10 | 0 |
| 11 | DMATrashContentsAtts | 12 | 1 |
| 12 | DMAButtonAtts | 12 | 30 |
| 13 | DMA3PosButtonAtts | 16 | 0 |
| 14 | DMAPictAtts | 8 | 73 |
| 15 | DMASliderAtts | 10 | 2 |
| 16 | DMAPictureList | 4 | 23 |
| 17 | DMAPictureListList | 10 | 1 |
| 18 | DMAPictureListListPlusSpacing | 10 | 1 |
| 19 | DMAscrollList | 10 | 0 |
| 20 | DMARowScrollList | 10 | 1 |
| 21 | DMAModifierGroup | 8 | 0 |
| 22 | DMAModifier | 10 | 1 |
| 23 | DMASlider | 10 | 0 |
| 24 | DMAModifierList | 10 | 0 |
| 25 | DMATwoPictureButton | 8 | 9 |
| 26 | DMALocalStampRolloverButton | 10 | 1 |
| 27 | DMALocalStampRenderButton | 10 | 1 |
| 28 | DMALocalStampModifyButtons | 10 | 1 |
| 29 | DMAGlobalStampRenderButons | 10 | 1 |
| 30 | DMACustomDialogBox | 10 | 2 |
| 31 | DMASoundList | 4 | 3 |
| 32 | DMAAnimationList | 2 | 173 |
| 33 | DMAStampRowList | 10 | 66 |
| 34 | DMAStampRenderingData | 10 | 269 |
| 35 | DMAStampGroup | 10 | 299 |
| 36 | DMAStampMusicList | 4 | 15 |
| 37 | DMAGenrePackData | 10 | 15 |
| 38 | DMAStampGenreList | 10 | 1 |
| 39 | DMAGenrePackTableOfContents | 10 | 0 |
| 40 | StringList | 4 | 0 |
| 41 | fontInfo | 10 | 0 |
| 42 | StringTable | 10 | 1 |
| 43 | creditsTextEntry | 12 | 155 |
| 44 | creditsSpaceEntry | 2 | 21 |
| 45 | DMACreditsPage | 10 | 7 |
| 46 | DMACreditsAudio | 4 | 1 |
| 47 | DMACreditsPageList | 10 | 2 |
| 48 | PaneDefinition | 10 | 20 |
| 49 | ProjectLayout | 10 | 20 |
| 50 | AllProjectLayouts | 10 | 2 |
| 51 | ToolSounds | 10 | 74 |
| 52 | RopeSound | 2 | 1 |
| 53 | StdBrush | 48 | 46 |
| 54 | AnimBrushSet | 10 | 106 |
| 55 | AnimBrush | 8 | 12 |
| 56 | AnimBrushNibSets | 10 | 1 |
| 57 | JumboPencilLocations | 10 | 5 |
| 58 | BundleLocations | 10 | 5 |
| 59 | CrayonBlockLocations | 10 | 5 |
| 60 | TubeSounds | 2 | 1 |
| 61 | Patterns | 10 | 1 |
| 62 | AnimatedPaint | 4 | 10 |
| 63 | AnimatedPaintGroup | 2 | 2 |
| 64 | CreationScene | 10 | 1 |
| 65 | ArrowPositionStructs | 10 | 1 |
| 66 | UnderTheHoodScene | 10 | 1 |
| 67 | SlideShowStuffStruct | 10 | 1 |
| 68 | SketchBookListStruct | 10 | 1 |
| 69 | DfltTexture | 10 | 1 |
| 70 | CursorsForTools | 10 | 1 |
| 71 | ToolButtons | 10 | 1 |
| 72 | ToolSizeButtons | 10 | 0 |
| 73 | shapeSelectionButtons | 10 | 1 |
| 74 | ColorPicker | 10 | 1 |
| 75 | BlenderColorPicker | 40 | 1 |
| 76 | StampManipulatorSounds | 10 | 1 |
| 77 | ModifierStrokes | 2 | 1 |
| 78 | ModifierData | 8 | 29 |
| 79 | ModifierList | 4 | 53 |
| 80 | ModifierVisual | 10 | 276 |
| 81 | ModifierPushButton | 10 | 5 |
| 82 | ModifierCheckBox | 16 | 26 |
| 83 | ModifierRadioButtonSet | 10 | 12 |
| 84 | ModifierCycleThrough | 10 | 40 |
| 85 | RadioButtonCycleThrough | 10 | 1 |
| 86 | CollectionScript | 10 | 31 |
| 87 | TransitionShape | 18 | 1 |
| 88 | DMAAnimatedButton | 8 | 1 |
