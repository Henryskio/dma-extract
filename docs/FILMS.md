# \*\*\*\*\*\*'s Magic Artist Deluxe / Magico Artista — MMFW Films Format

Reverse-engineered notes for the Films containers: `/RawAssets/DMA.MMF` (the
third game's, Italian release \*\*\*\*\*\* Magico Artista, 2002), the second
game's `DMA.MMF`, and the 3D game's `DMA3D.MMF`. Everything below was derived
from the shipped files and the games' own executables (`DMA4.exe`,
`DMA.exe`, `DMA3D.exe`); the third game's decoder was checked numerically
against all 139 of its frames, the 3D game's against its 1,024. No external
format documentation was used as authority.

> **Which game this covers.** Container versions 2 and 3, whose `.MMF` files
> differ only in the film fields listed at the end. The 3D game's `DMA3D.MMF`
> is version 3 as well and reads the same layout — it ships three films rather
> than one, which is what exposed the record-vs-table mistake in §2. The
> decoder in `src/film.rs` has no version branch. The first game ships no
> `.MMF` at all: its videos sit in an opaque `Blobs` container (`pack.BLB`)
> whose inner format is undecoded, so there is nothing to describe here yet.
> The blob payload's header *is* measured, though, and that measurement lives
> in **`GAME1.md`** §6.

> **Where each game's deltas live.** This document is the shared chain: it
> describes the layout versions 2 and 3 have in common. What is particular to
> one game is recorded in that game's own document — the second game's
> differences in **`SEQUEL.md`**, the 3D game's in **`DMA3D.md`** §6 — so an
> edit inside one game's chain cannot silently change another's.

> **Engine note.** Despite the `MMFW` container signature this is **not a
> Clickteam Multimedia Fusion 2 application**. `DMA4.exe` carries the PDB path
> `C:\Projects\DeluxeRevision\DMA Art Ware\Intermediate\Release\DMAAW.pdb`,
> was linked with MSVC 6 on 2002-06-15, imports only Win32/MFC/QuickTime-era
> libraries, and its resource-loader errors are prefixed `MME_*`
> (`MME_FILM_RESOURCE_FILE_*`, etc.). There is no `Clickteam`, `CNC`, or
> `MMF2` string in the binary. `MMFW` is this game's in-house container
> signature; the loader family is referred to here as **MME**. The film
> loader is `FUN_004ca309`, the delta-frame decoder `FUN_004bf7e5`, and the
> 32-bit byte reversal used by the RLE codec `FUN_004c396b`.

---

## 1. Container header

`DMA.MMF` holds **one** film resource, `Intro.flm`. The common MMWF header is
the same shape as the picture containers:

```text
0x00  16 bytes  "MMFW Films" + NUL padding
0x10   2 bytes  endian marker "MM"
0x12   2 bytes  u16 version = 3
0x14   6 bytes  file id (68 d7 11 b3 00 00)
0x1A   8 bytes  timestamp field (stored as an f64; value is not a
                conventional date in this file)
0x22   2 bytes  u16 resource count N = 1
0x24  4*(N+1)  u32 offsets: resource i spans [off[i], off[i+1])
       2 bytes  padding
      32*N      resource names, NUL-padded ASCII ("Intro.flm")
```

For `DMA.MMF` the offsets are `0x470` and `0x68A424`, so the payload is
6,856,628 bytes.

**The payload is not container-level LZ77 data.** The generic Films metadata
slot that looks like a decompressed-size table contains
`26,870,306` (0x019A0222), which is not the payload length and must not be
used. The film-specific metadata and the complete inner `.flm` stream live in
the payload region. `--original` therefore writes `payload(0)` verbatim.

## 2. Films metadata block

After the names table comes the generic `4*N` u32 array (zero in the third
game, Mac-style timestamps in the 3D game), then the Films block. Let
`ne = names_end` (for `DMA.MMF`, `0x4E`; for `DMA3D.MMF`, `0x96` with `N == 3`).

| Offset | Size | Contents |
| --- | --- | --- |
| `ne` | `4*N` | generic u32 array, `0` in this file |
| `ne + 4*N` | `2` | marker, must be **7** |
| `ne + 4*N + 2` | `N` | per-film table 1, one byte each |
| `+ N` | `N` | per-film table 2, one byte each |
| `+ 2N` | `4*N` | **the rects**, four big-endian u16 per film |
| `+ 2N + 8N` | `4*N` | per-film table 4, one big-endian u32 each |
| `+ 6N + 8N` | `4*N` | per-film table 5, four raw bytes each |
| `+ 10N + 8N` | `2*N` | per-film table 6, one big-endian u16 each |
| `+ 12N + 8N` | `4*N` | per-film table 7, one big-endian u32 each |
| `+ 16N + 8N` | `2*N` | per-film table 8, one big-endian u16 each |
| `+ 26N` | `2` | palette count `P` |
| next | `1024*P` | palettes, 256 RGBA-ish entries each |

The payload must start exactly at the end of the last palette. For
`DMA.MMF`:

```text
0x4E  00 00 00 00                 generic u32 = 0
0x52  00 07                       marker = 7
0x54  19 02                       table 1 = 0x19, table 2 = 0x02
0x56  00 00 00 00 01 9a 02 22     the rect {top, left, bottom, right}
0x5E  00 03 fe ca                 table 4 = 0x0003FECA
0x62  5e 00 00 00                 table 5
0x66  05 dc                       table 6 = 1500
0x68  00 3f 3e 64                 table 7 = 0x003F3E64
0x6C  00 fa                       table 8 = 250
0x6E  00 01                       palette count = 1
0x70  1024-byte palette
0x470 payload starts
```

### The eight per-film tables, and why they are not a record

The loader reads eight per-film tables in this order: `N` bytes, `N` bytes,
**`4*N` big-endian u16**, `N` u32, `4*N` raw bytes, `N` u16, `N` u32, `N` u16.
That is `26*N` bytes — the same total a 26-byte record per film would be, and
for every container shipped with the second and third games `N == 1`, so the
two readings sit on the same bytes and it did not matter which was used.
Reading them as records is wrong the moment there is more than one film, which
is why the 3D game's three films all failed to decode.

**The loader, measured.** `DMA3D.exe`'s film container class is
`MM2DFilmResourceFile` (RTTI string `.?AVMM2DFilmResourceFile@@` at `0x569df0`,
vtable at `0x536930`, whose slot-3 virtual at `0x477930` returns the string
`MMFW Films` at `0x53691c`). Its slot-4 virtual, the metadata reader, is at
`0x477940` and reads:

```text
0x477970  u16 marker, compared against 7      `cmpw $0x7,0x1a(%esp)` at 0x47798b
0x4779ca  N   bytes   stream vt[5], count = this+0x448
0x4779f2  N   bytes   stream vt[5]
0x477a16  4*N u16     0x4ec9d0 — 2*N big-endian u32, each halved into two u16
0x477a40  N   u32     0x4e8810 — 4 bytes each, byte-swapped (`lea 0x0(,%edi,4)`)
0x477a6d  4*N bytes   stream vt[5], count << 2
0x477a92  N   u16     0x4e87c0 — 2 bytes each, byte-swapped (`lea (%edi,%edi)`)
0x477ab9  N   u32     0x4e8810
0x477ae1  N   u16     0x4e87c0, with a third argument
0x477aef  u16 palette count, must be > 0     `jle` to the error at 0x477afc
0x477b52  P * 1024 palette bytes             `shl $0xa`, `add $0x400`
```

The byte swaps are what make every multi-byte field big-endian, and the cap of
100 u32 inside `0x4ec9d0` (`cmp $0x64,%ebx` at `0x4ec9f3`) is a stack-buffer
limit for the name table, not a limit on the rect array.

**The rects.** Table 3 holds four big-endian u16 per film, in resource order,
read as `{top, left, bottom, right}`. For the third game's `Intro.flm`
(`N == 1`, so the rect sits at `ne + 4*N + 4`, the same address a record's
`+2` field would hold) the values are `0, 0, 410, 546` and give

```text
width  = right - left  = 546
height = bottom - top  = 410
stride = stride8(546)  = 548     (rows padded to a multiple of four)
frame  = 548 * 410     = 224,680
```

which is exactly the byte count the film's own key-frame chunk declares it
decodes to. That identity — not the shape of the metadata — is what settles the
field order and the padding.

**The 3D game's three films, measured the same way** (`DMA3D.MMF`, also version
3, `N == 3`):

| film | rect | width | height | stride | frame bytes | key-frame chunk says |
| --- | --- | --- | --- | --- | --- | --- |
| `introAxDisney` | `0, 0, 600, 800` | 800 | 600 | 800 | 480,000 | 480,000 |
| `introAxImageBuilder` | `60, 107, 486, 696` | 589 | 426 | 592 | 252,192 | 252,192 |
| `introAxSplash` | `60, 80, 540, 720` | 640 | 480 | 640 | 307,200 | 307,200 |

`introAxImageBuilder` is the one that shows why the rect and the stride have to
be measured rather than inferred: its width is 589, which pads to a 592-byte
stride, and only that reading consumes the frame buffer exactly. Reading the
rect as `{left, top, right, bottom}` instead gives 636×379 and a buffer 11,148
bytes short.

The other five tables are preserved verbatim in the manifest
(`record_hex` reassembles them in the reader's order); their meaning is still
open. In particular table 1's `0x19` is **not** a frame rate: the measured
timestamp step is 83 ticks, i.e. about 12.05 fps, not 25 fps.

**Key frames are not one per film.** The second and third games ship a single
key-frame chunk. `DMA3D.MMF`'s films re-send a key frame mid-stream — 45, 12
and 1 across its three films — in the pattern `timestamp, key frame` or
`timestamp, delta`, so a key frame anywhere after the first replaces the frame
buffer rather than being an error.

### Palette

The palette is 1024 bytes: 256 entries of four bytes in the same order as the
picture format:

```text
byte 0 = transparency (255 = fully transparent, 0 = fully opaque)
byte 1 = Red
byte 2 = Green
byte 3 = Blue
```

Alpha is `255 - transparency`. Entry 0 is `(255, 0, 0, 0)` — transparent
black. Entry 254 is `(0, 0, 0, 0)` — **opaque black**, which is the film's
background. Entry 255 has the same RGBA value as entry 254 but is not used by
any frame. All 254 used entries (`1..254`) have distinct RGBA values.

## 3. Inner `.flm` chunk stream

The payload at `0x470` is a sequence of 4-byte-aligned chunks:

```text
u16 tag = 0xF789
u16 type
u32 length            (big-endian; includes any padding to a 4-byte boundary)
length bytes payload
```

The chunk walk ends exactly at the payload end (`0x68A424`). There are 412
chunks:

| Type | Count | Length(s) | Meaning |
| --- | --- | --- | --- |
| `0x02` | 139 | 588..7,336 | audio block |
| `0x03` | 1 | 4 | marker, payload `00 00 00 00` |
| `0x06` | 138 | 4 | frame timestamp, one u32 BE |
| `0x07` | 1 | 0 | end of stream |
| `0x0A` | 1 | 3,584 | full frame |
| `0x0B` | 132 | 660..94,204 | delta frame |

The first chunk is the type-`0x0A` full frame. Audio chunks and video chunks
are interleaved; the type-`0x03` marker appears after the last audio block and
before the final run of video timestamps. The type-`0x07` chunk is empty and
last.

## 4. Full frame (type 0x0A)

Payload:

```text
u32 A              length of the LZ77 stream
u32 B              decoded length
A bytes            LZ77 stream
0..3 bytes         padding
```

For `DMA.MMF`: `A = 3,573`, `B = 224,680`, chunk length `3,584`
(`8 + 3573 + 3` padding). The decompressed bytes are exactly `B = 548 * 410`
bytes and are the **first frame at time zero**.

The frame buffer is 8bpp palette indices:

```text
width   = 546
height  = 410
stride  = 548 = ((546 + 3) / 4) * 4
size    = 548 * 410 = 224,680 bytes
```

Each row uses 546 bytes; the last two bytes of each row are padding and are
zero in this file.

The first frame is mostly index 254 (opaque black). Its non-254 content
occupies the box `x=233..312`, `y=148..261` (8,541 pixels), which is the
seed of the intro animation.

## 5. Delta frame (type 0x0B)

Payload:

```text
u32 A                         length of the LZ77 stream
u32 B                         decoded RLE length
20 * (4 * u16 BE)             rectangle table
A bytes                       LZ77 stream
0..3 bytes                    padding
```

Chunk length = `168 + A + padding`. In `DMA.MMF`, `A` ranges from 490 to
94,034 and `B` from 1,332 to 170,384. **Every one of the 132 delta chunks
decompresses to exactly B bytes**; this was checked for all of them.

Each rectangle is stored on disk as four u16 in the order
`(top, left, bottom, right)`. The extractor stores them as
`[left, top, right, bottom]` in the manifest. The rectangles are **advisory**:
in 19 of the 132 deltas at least one RLE write falls outside the union of the
20 rectangles, and 44 of the 2,640 rectangle slots are all-zero. The RLE
stream, not the rectangle list, is authoritative.

### Cursor-relative RLE

The decompressed delta is a sequence of 4-byte records, all big-endian:

```text
i16 offset_delta
i16 length
```

The cursor starts at 0. For every record, add `offset_delta` to the cursor,
then:

* `length == 0` — cursor move only, no bytes written.
* `length > 0` — copy `length` literal bytes from the delta stream, advance
  the cursor.
* `length < 0` — read the next four bytes as a pattern, **byte-reverse the
  32-bit word** (`FUN_004c396b`: `b0 b1 b2 b3` becomes `b3 b2 b1 b0`), repeat
  those four bytes `-length` times, advance the cursor.

All writes are bounds-checked against the 224,680-byte frame. Across the 132
deltas the stream contains 307,113 records: 54 move-only, 290,066 literal and
16,993 pattern records, writing 9,615,984 bytes in total. Every stream
consumes exactly its declared `B` bytes.

Worked example, first delta stream:

```text
7d 00 | 00 00     offset +32000, length 0  -> cursor 32000
7d 00 | 00 00     offset +32000, length 0  -> cursor 64000
43 bc | 00 08     offset +17340, length 8  -> cursor 81340
fe f9 ...         eight literal bytes
```

`81340 = 148 * 548 + 236`, i.e. row 148, column 236, matching the first
non-background rectangle of frame 1.

## 6. Timeline

The type-`0x06` chunks carry one u32 timestamp each. The first 19 audio blocks
precede the first timestamp; afterwards audio and video are interleaved.

```text
frame 0:   full frame from type 0x0A, start tick 0
frame 1:   type 0x06 tick 83, followed by a type 0x0B delta
frame 2:   tick 166, delta
...
frame 138: tick 11454, delta
```

There are 138 timestamps with a constant 83-tick step. A timestamp defines the
next frame; a following `0x0B` updates the frame buffer, and if no `0x0B`
follows, the frame repeats the previous buffer. Six timestamps have no delta:

```text
duplicate frame indices: 6, 51, 52, 53, 137, 138
```

That gives **139 frames in timeline order**, of which **133 are visually
unique**. Frame 0 runs for the first 83 ticks; the last frame runs to
`11454 + 83 = 11537` ticks. The extractor treats one tick as one millisecond
(83 ticks ≈ 12.05 fps, total ≈ 11.537 s). This unit is an inference from the
constant step; no fps/unit field has been identified in the record.

Duplicate frames are still written as PNGs so that `frame_NNN.png` always
corresponds 1:1 with the timeline; the manifest marks them with
`"duplicate": true`.

## 7. Audio (type 0x02)

Each audio chunk is:

```text
u32 data_len       PCM byte count
u32 zero
u32 block_index
data_len bytes     PCM
0..3 bytes         padding
```

`chunk_len = 12 + data_len + padding`. There are 139 blocks:

* **127 data blocks**, indices `0..126`; total PCM 874,546 bytes.
* **12 sync blocks**, index `0xFFFFFFFF`; total 49,102 bytes.

The common data length is 7,320 or 7,322 bytes. At 44.1 kHz, 16-bit mono,
7,320 bytes is ≈83 ms, matching the frame step — but the container carries no
sample-rate/format field, so this is a hypothesis rather than a confirmed
decode. The audio is **not** decoded by this extractor; the raw `.flm`
preserved by `--original` contains every PCM byte and every sync block.

The audio chunk count (139) equals the frame count in this film, but the
container stores no explicit frame index per audio block, so the extractor
does not claim a 1:1 frame association; it records the chunk list and offsets
in the manifest.

## 8. Other chunks

* Type `0x03`, one chunk, payload `00 00 00 00`, sits between the last audio
  block and the final run of video timestamps. Its role is not confirmed; it
  is consistent with an end-of-audio marker.
* Type `0x07`, zero-length, ends the inner stream. The chunk walk ends exactly
  at the payload end.

## 9. Extraction output

### Original mode (`--original`)

```text
Films/DMA/00000_Intro.flm
```

This is the container payload byte-for-byte: **6,856,628 bytes**, SHA-256
`25a70ad1c9db9daf244e4fcbda773eea9cece9734a5e8f3a36bdf53500c57e14`.
It contains the compressed LZ77 streams, all palette and record bytes, the
complete PCM track and every chunk, so no information is lost. The generic
container `sizes` value is never consulted.

### Modern mode (default)

```text
Films/DMA/00000_Intro/
  frame_000.png ... frame_138.png     139 PNGs, 546x410 RGBA
  manifest.json
```

Each PNG expands the frame's 8bpp indices through the film palette:

```text
RGBA = (palette[i].red, palette[i].green, palette[i].blue,
        255 - palette[i].transparency)
```

The PNGs are lossless renders at the pixel level. The manifest includes the
exact 1024-byte palette as `palette_hex`, and all 254 used entries have unique
RGBA values in this film, so the palette + PNGs reconstruct the used indices.
The PNG does not itself store the index values, though; the raw `.flm` remains
the authoritative byte-exact form.

`manifest.json` contains:

* film geometry, stride, frame size, record hex, bounds and palette;
* total frames, unique frames, duration and the tick/timebase note;
* chunk count and per-type counts, with offsets relative to the `.flm` start;
* one entry per frame: `start_ms`, `duration_ms`, `duplicate`, PNG name,
  source chunk offsets and the 20 advisory delta rectangles;
* the complete audio chunk map (offset, length, data length, block index,
  sync flag).

The extractor returns the number of **films** handled (1 for `DMA.MMF`);
frame counts live in the manifest and in the tests.

## 10. Verification

The film tests live in `tests/film_roundtrip.rs` and use the real assets when
present:

* `original_film_is_the_verbatim_inner_flm` calls the production extractor,
  asserts the written `.flm` equals `Container::payload(0)` and the on-disk
  slice `[0x470, 0x68A424)`, checks the bogus `sizes[0] = 26,870,306`, and
  confirms the inner stream parses to 412 chunks ending at the payload end.
* `modern_film_writes_lossless_pngs_and_timeline` checks 139 frames / 133
  unique, the duplicate indices, the 83-tick timeline and total 11,537 ticks,
  139 audio chunks with 12 sync blocks, the golden SHA-256 of frames 0 and
  138, and decodes **every** PNG back to the exact palette-expanded RGBA
  bytes.

Golden frame hashes (SHA-256 of the raw 224,680-byte index buffers):

```text
frame 0:   153192a1d8a2f4ea0ba3285ba29531a687e5749b4dfd238e4891d63dff64c8c8
frame 138: 661f73dda583c23eef4ecd3398bb32987534d1fe0487247d9a23c6002127f202
```

`src/bin/imgstat` can sanity-check the PNGs without loading them into an
image viewer: every frame must report `546x410`, and the top colour is
opaque black (the background index 254).

Run:

```sh
cargo test --release
./target/release/mmf-extract /RawAssets \
  -o /workspace/mmex-film/extracted_test --types films
```

## 11. Open questions

* Five of the eight per-film tables (all but the rects) are preserved verbatim
  in the manifest but not interpreted. Tables 4 and 7 are big-endian u32 and
  tables 1, 2 and 5 are raw bytes; table 6 is a u16 that reads 1500 in all
  three of the 3D game's films and 250 in the third and second games'.
* The type-`0x03` marker's exact role is unconfirmed.
* The audio sample rate/format is inferred from byte counts, not declared in
  the container; audio is not decoded.
* The 20-rectangle table is advisory and sometimes does not cover every RLE
  write, so it cannot replace the RLE stream.
* The tick unit is treated as milliseconds because 83 ticks ≈ 12 fps; no
  explicit timebase field has been found.
