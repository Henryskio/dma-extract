# \*\*\*\*\*\*'s Magic Artist Deluxe / Magico Artista — MMFW Films Format

Reverse-engineered notes for the single Films container shipped with the game,
`/RawAssets/DMA.MMF` (Italian release: \*\*\*\*\*\* Magico Artista, 2002).
Everything below was derived from the shipped files and the game's own
executable (`DMA4.exe`); the decoder was checked numerically against all 139
frames. No external format documentation was used as authority.

> **Which game this covers.** Container versions 2 and 3, whose `.MMF` files
> differ only in the film fields listed at the end. The decoder in
> `src/film.rs` has no version branch. The first game ships no `.MMF` at all:
> its videos sit in an opaque `Blobs` container (`pack.BLB`) whose inner
> format is undecoded, so there is nothing to describe here yet. The blob
> payload's header *is* measured, though, and that measurement lives in
> **`GAME1.md`** §6.

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

After the names table comes the generic `4*N` u32 array (zero here), then the
Films block. Let `ne = names_end` (for `DMA.MMF`, `0x4E`).

| Offset | Size | Contents |
| --- | --- | --- |
| `ne` | `4*N` | generic u32 array, `0` in this file |
| `ne + 4*N` | `2` | marker, must be **7** |
| `ne + 4*N + 2` | `26*N` | one 26-byte record per film |
| `ne + 4*N + 2 + 26*N` | `2` | palette count `P` |
| next | `1024*P` | palettes, 256 RGBA-ish entries each |

The payload must start exactly at the end of the last palette. For
`DMA.MMF`:

```text
0x4E  00 00 00 00                 generic u32 = 0
0x52  00 07                       marker = 7
0x54  19 02 00 00 00 00 01 9a 02 22 00 03 fe ca 5e 00
      00 00 05 dc 00 3f 3e 64 00 fa          26-byte record
0x6E  00 01                       palette count = 1
0x70  1024-byte palette
0x470 payload starts
```

### The 26-byte film record

The loader reads the record as eight per-film fields, in this order:
`N` bytes, `N` bytes, an 8-byte group (two u32s split into four u16s),
then u32, u32, u16, u32, u16. The exact bytes for `Intro.flm` are:

```text
19 02 | 00 00 00 00 01 9a 02 22 | 00 03 fe ca | 5e 00 00 00 |
05 dc | 00 3f 3e 64 | 00 fa
```

| Record offset | Size | Value | Meaning |
| --- | --- | --- | --- |
| `+0` | u8 | `0x19` (25) | unknown |
| `+1` | u8 | `0x02` (2) | unknown |
| `+2` | 4×u16 BE | `0, 0, 410, 546` | working interpretation: `top, left, bottom, right` |
| `+10` | u32 BE | `0x0003FECA` (261,834) | unknown |
| `+14` | u32 BE | `0x5E000000` (1,577,058,304) | unknown |
| `+18` | u16 BE | `1500` | unknown |
| `+20` | u32 BE | `0x003F3E64` (4,140,644) | unknown |
| `+24` | u16 BE | `250` | unknown |

The four u16 values at `+2` are certain; the field names `top/left/bottom/
right` are the working interpretation. They yield

```text
width  = right - left  = 546
height = bottom - top  = 410
```

which is exactly the decoded frame geometry. The five remaining fields are
preserved verbatim as `record_hex` in the manifest; their meaning is still
open. In particular, `0x19` is **not** a frame rate: the measured timestamp
step is 83 ticks, i.e. about 12.05 fps, not 25 fps.

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

* The five unknown record fields (`+10..+26`) are preserved but not
  interpreted.
* The type-`0x03` marker's exact role is unconfirmed.
* The audio sample rate/format is inferred from byte counts, not declared in
  the container; audio is not decoded.
* The 20-rectangle table is advisory and sometimes does not cover every RLE
  write, so it cannot replace the RLE stream.
* The tick unit is treated as milliseconds because 83 ticks ≈ 12 fps; no
  explicit timebase field has been found.
