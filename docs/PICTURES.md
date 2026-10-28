# \*\*\*\*\*\*'s Magic Artist Deluxe / Magico Artista — MMFW Picture Format

Reverse-engineered notes for the `.MMP` (Pictures) containers of
\*\*\*\*\*\*'s Magic Artist Deluxe (Italian release: \*\*\*\*\*\* Magico Artista,
2002). Everything here was derived from the game's own executable
(`DMA4.exe`) plus empirical verification against the shipped assets; claims
that were confirmed by rendering are checked numerically with the helper
binaries in `src/bin/`.

> **Engine note.** Despite the `MMFW` container signature this is **not a
> Clickteam Multimedia Fusion 2 application**. `DMA4.exe` carries the PDB
> path `C:\Projects\DeluxeRevision\DMA Art Ware\Intermediate\Release\DMAAW.pdb`,
> was linked with MSVC 6 on 2002-06-15, imports only Win32/MFC/QuickTime-era
> libraries, and its resource-loader errors are prefixed `MME_*`
> (`MME_PICTURE_RESOURCE_FILE_BAD_HEADER`, etc.). There is no `Clickteam`,
> `CNC`, or `MMF2` string in the binary. The header was initially compared
> against the mmex documentation; the layout below is confirmed directly
> against the files and the game's own loader.

---

## 1. Container header

All five picture containers in the game use the big-endian `"MM"` marker and
version 3.

```text
0x00  16 bytes  "MMFW " + tag, NUL padded   (tag = "Pictures")
0x10   2 bytes  endian marker: "MM" (big-endian) or "II" (little-endian)
0x12   2 bytes  u16 version, must be 3
0x14   6 bytes  file id
0x1A   8 bytes  timestamp (f64)
0x22   2 bytes  u16 resource count N
0x24  4*(N+1)  u32 offsets; resource i spans [off[i], off[i+1])
       2 bytes  padding
      32*N      resource names, NUL-padded ASCII
```

Payloads begin at `off[0]`. Each payload is compressed with the game's own
LZ77 variant (a byte-oriented LZ77 with 16/32 KiB windows), implemented in
`src/lz77.rs` and transliterated from `FUN_004c557a` in `DMA4.exe`.

## 2. Metadata region

Between the end of the names and the first payload sits a metadata region that
is exactly **`15 * N + 4` bytes** long in every picture container in this game.
That exact size is what pins the field boundaries down; the individual fields
were then confirmed against the executable's accessor functions.

Let `ne` = end of the names table = `(0x24 + 4*(N+1) + 2) + 32*N`.

| Offset from `ne` | Size | Contents |
| --- | --- | --- |
| `0 * N` | `4*N` | u32 array: per-resource CRC/hash |
| `4 * N` | `N` | attribute byte array |
| `5 * N` | `N` | flags byte array (bit 0 selects 8bpp) |
| `6 * N + 2` | `4*N` | dimension table: `{u16 height, u16 width}` |
| `10 * N + 2` | `4*N` | decompressed size per resource |
| `14 * N + 2` | `N` | kind byte array (palette source) |
| `15 * N + 2` | `2` | padding |

Verified byte-for-byte against all five containers:

| Container | N | metadata bytes | `15*N+4` |
| --- | --- | --- | --- |
| `app.MMP` | 1935 | 29029 | 29029 |
| `HD.MMP` | 6692 | 100384 | 100384 |
| `stmp1.MMP` | 3647 | 54709 | 54709 |
| `stmp2.MMP` | 3855 | 57829 | 57829 |
| `stmp3.MMP` | 14275 | 214129 | 214129 |

### The dimension table stores {height, width}

Each entry is a big-endian `u32` holding two `u16`s. **The first is the
height and the second is the width.**

This was confirmed visually: `ITXTcarrot24` (app.MMP resource 124, table pair
`211, 245`) renders as a coherent picture when read as 245 wide by 211 high,
and as "a bunch of lines" when the pair is read the other way round. The same
held for `ITXTmustard24` (resource 128, pair `229, 262`).

### Flags byte

Bit fields, taken from the executable's accessors:

| Accessor | Expression | Meaning |
| --- | --- | --- |
| `FUN_004c3c00` | `flags & 1` | 1 = 8 bits per pixel, 0 = 32 bits per pixel |
| `FUN_004c5c36` | `flags & 3` | low two bits |
| `FUN_004c3c98` | `(flags & 0xC) >> 2` | bits 2-3 |
| `FUN_004c5c4a` | `flags >> 2` | upper bits |
| `FUN_004c3cc0` | `flags >> 4` | bit 4 selects the byte-swap path |

### Kind byte

`FUN_004c3c14` returns this byte clamped to 0, 1 or 2. The loader
(`FUN_004a80f7`) treats it as:

* **0** — no palette is attached. A 32bpp resource still carries its own
  pixels; an 8bpp resource with kind 0 is a coverage mask (see §3).
* **1** — the resource carries its own 1024-byte palette.
* **2** — the resource shares a palette owned by another resource; the owner
  index is resolved by `FUN_004c3c35`, which maps a kind of `k >= 2` to
  resource `k - 2`.

In this game's files the kind byte is only ever 0 or 1.

## 3. Pixel format

### 32 bits per pixel

The decompressed payload is exactly `width * height * 4` bytes — no header, no
padding, no transform. Each pixel is four bytes in this order:

```text
byte 0 = transparency (255 = fully transparent, 0 = fully opaque)
byte 1 = Red
byte 2 = Green
byte 3 = Blue
```

Alpha is `255 - byte 0`. The colour bytes are straight, non-premultiplied
values.

**Confirmed numerically on the colour-named sprites.** The dominant opaque
tuple in `ITXTmustard24` is the raw bytes `(0, 255, 239, 90)`, which decodes to
RGB(255, 239, 90) — mustard yellow. Reading the colour bytes in the reverse
order would make it RGB(90, 239, 255), sky blue, which is impossible for a
mustard sprite. `ITXTcarrot24`'s dominant opaque colours are RGB(16, 99, 0)
and friends — the green of carrot leaves — alongside orange body pixels.

The background tuple is `(255, 0, 0, 0)`: transparency 255 (fully
transparent). Treating byte 0 as blue, as an earlier revision of these notes
did, forces every opaque pixel to blue = 0 and gives the whole corpus a
yellow/olive cast. That was the "lots of yellow" bug.

### 8 bits per pixel

The decompressed payload is:

```text
offset 0                      1024 bytes   palette: 256 entries of 4 bytes
offset 1024                   stride * H   one palette index per pixel
```

where the row stride is `((width + 3) / 4) * 4` — each row is padded out to a
multiple of four bytes.

**The palette uses the same byte order as the pixels**: byte 0 = transparency,
byte 1 = Red, byte 2 = Green, byte 3 = Blue. Palette entry 0 is the fully
transparent entry and is stored as `(255, 0, 0, 0)`, mirroring the transparent
background tuple found in the 32bpp sprites.

Confirmed numerically on `stmp1.MMP` resource 3363 (`IStriFIRa0120`), a "fire"
stamp: its palette contains the entry `(0, 254, 141, 0)`, which decodes to
RGB(254, 141, 0) — orange. The reverse colour order would give RGB(0, 141,
254), blue.

### 8 bits per pixel without a palette: coverage masks

A payload of exactly `stride * H` bytes and no 1024-byte prefix is **not** a
palette image. It is an 8-bit **coverage (alpha) mask**: each byte is an alpha
value, 0 = transparent and 255 = fully opaque. The game supplies the colour at
runtime (these are the marker/stamp/eraser/nib shapes used by the paint
tools), so the mask itself carries no RGB. The extractor renders a mask as
white with `alpha = byte`; `--original` preserves the raw coverage bytes
exactly.

Evidence: `MarkerStrokeBANANA02.pct` (32x32) is a smooth banana-shaped ramp
(background 0, edges 1-63, interior 192-255);
`ITeraserPHASE_A05_1.pict` (40x40) is a uniform mid-coverage field (a
half-strength eraser); `callig_nib` / `cone_nib` are smooth nib-shaped
gradients. See `src/bin/grid`, which prints these maps as ASCII.

### Which layout a resource uses

Decided by the payload length, which is unambiguous:

```text
len == width * height * 4                     -> 32bpp
len == 1024 + ((width+3)/4*4) * height        -> 8bpp with own palette
len == ((width+3)/4*4) * height               -> 8bpp coverage mask
```

## 4. Loader byte transforms

The executable can byte-swap picture data on load. Both transforms are pure
permutations of the four bytes within a pixel, so every possible transform
reduces to one of the 24 permutations of a pixel's bytes — and the user
verification above picked out exactly one of them.

* `FUN_004c3a62` — `bswap` followed by `ror $0x10`: swaps the two 16-bit
  half-words of each 32-bit word (bytes 0<->1 and 2<->3).
* `FUN_004c3aa6` — plain 32-bit `bswap`: reverses all four bytes.
* `FUN_004b6d3a` — reverses the first `0x100` bytes (the palette) and advances
  the data pointer past it.

The loader (`FUN_004a8344`) applies them as follows:

* `kind == 1` → reverse the leading 0x100 bytes (the palette) and skip them.
* `flags >> 4 == 0` and the resource is 32bpp → full 32-bit reversal.
* otherwise → the 16-bit half-word swap over the whole resource.

**In this game's shipped assets the effective result is the identity
transform**, which is why the raw stored bytes already carry the
transparency/red/green/blue order described above.

## 5. Known exceptions in `HD.MMP`

`HD.MMP` holds 6692 resources. 6313 decode cleanly with the rules above. The
remainder fall into two groups:

* **105 resources** whose payload length matches neither the 32bpp nor the
  8bpp formula. Examples: `MITDrawer1Open032.PICT` (table 16x20, 820 bytes,
  whose bytes look like 16-bit values `00 52 00 54 00 56 ...`),
  `ISselectBoxFlipSelect.pct` (44x44, 2320 bytes, bytes look like
  `ff 00 00 00 eb 00 00 00 ...`), and `ISselectBoxROLLON.PCT` (60x60, 3481
  bytes, whose bytes look like 8-bit greyscale triples).
* **484 resources** that are 8bpp coverage masks with **no palette**
  (`len == stride * height`), e.g. `MarkerStrokeBANANA02.pct` (32x32, 1024
  bytes). Each byte is alpha coverage for a shape the game tints at runtime.

Both groups carry a distinct attribute/flags byte in the metadata. The
484 masks are fully decoded (white + alpha); the 105 unusual ones remain a
distinct sub-format the extractor reports as failed rather than guessing.
See §7 for the full resolution.

## 6. Output

The extractor writes lossless PNG (RGBA, 8 bits per channel) for each
resource, and also writes the untouched compressed payload bytes for each
resource into a `Raw/` subdirectory, byte-for-byte identical to the slice of
the container they came from.

---

## 7. Payload classification — measured on all five containers

Two rules added after the notes above:

* **A zero in the decompressed-size table (`10*N+2`) means the payload is
  stored *uncompressed*** — it must not be fed to the LZ77 decoder. This was
  the cause of all 23 "decompression failures" in `HD.MMP` (e.g. `#2475`
  `ITXSDRT1_A01.PICT`, 6x6, is exactly 144 = w·h·4 bytes; `#6189`
  `ICtxtr2.pct`, 128x128, is exactly 17408 = 1024 + 128·128).
* Classification is by payload length, in this order:
  `len == w*h*4` → 32bpp; `len == 1024 + stride*h` → 8bpp with own palette;
  `len == stride*h` → 8bpp **without** a palette; otherwise → *odd*.

Measured with `src/bin/analyze`:

| container | N | uncompressed-32 | uncompressed-8 | 32bpp | 8bpp+pal | 8bpp no pal | odd | lz77 fail |
|---|---|---|---|---|---|---|---|---|
| `app.MMP` | 1935 | 0 | 0 | 1934 | 1 | 0 | 0 | 0 |
| `HD.MMP` | 6692 | 15 | 8 | 5669 | 411 | **484** | **105** | 0 |
| `stmp1.MMP` | 3647 | 0 | 0 | 150 | 3497 | 0 | 0 | 0 |
| `stmp2.MMP` | 3855 | 0 | 0 | 542 | 3313 | 0 | 0 | 0 |
| `stmp3.MMP` | 14275 | 0 | 0 | 324 | 13951 | 0 | 0 | 0 |

**Four of the five picture containers are fully decoded by the rules above.**
Only `HD.MMP` has unresolved classes.

### 7.1 Metadata columns — resolved via decompilation

`FUN_004c4038` (the picture-file opener) reads the metadata region in this
order, and the accessors in `FUN_004c3c00`/`FUN_004c3c14` read the exact
object offsets those reads populate (offset = slot index × 4):

| file column | object offset | accessor(s) | meaning |
|---|---|---|---|
| `4*N` u32 table | — | — | per-resource **CRC/hash** (~3·10⁹, or 0) |
| 1st N-byte array (at `4N`) | `+0x278` | — | **attribute byte**, purpose unknown |
| 2nd N-byte array (at `5N`) | `+0x284` | `FUN_004c3c00` (bit 0 = bpp), `FUN_004c3c98` (bits 2-3), `FUN_004c5c36` (bits 0-1) | **flags byte** |
| dims (`{height,width}` pairs) | `+0x290` | `FUN_004c3caf` (8 bytes/resource) | dimensions |
| sizes (`10*N+2`) | `+0x29c` | `FUN_004c3ced` | decompressed size (0 ⇒ stored uncompressed) |
| trailing N-byte array (at `14*N+2`) | `+0x2a8` | `FUN_004c3c14` (clamps >1 to 2), `FUN_004c3c35` | **kind byte = palette source** |

The kind byte is the palette-source selector (`FUN_004a80f7`):
**0** → no palette; **1** → a 1024-byte palette is prefixed to the payload
(the pixels start at `+0x400`); **k ≥ 2** → use file palette-bank entry `k−2`
(`FUN_004c3c35` returns `k−2`, `FUN_004c3d15` indexes the bank at `+0x2b4`).

**However, neither the flags bit 0 nor the kind byte is a reliable signal in
practice** — measured against payload length there are a handful of
contradictions in every container (`src/bin/invariants`), and the kind byte
only ever takes the values **0 and 1** (the palette-bank path `k≥2` is never
used, and there is no palette bank stored in these files: the bank count read
after the metadata is 0). **Payload length remains the ground truth** for
deciding 32bpp vs 8bpp-with-palette vs 8bpp-without.

### 7.2 The 484 palette-less 8bpp payloads (resolved: coverage masks)

The kind byte is **0** for all 484 and the payload length is `stride·h`. They
are **8-bit coverage masks**, not palette images: each byte is alpha, 0 outside
the shape and up to 255 inside it. They are the marker/stamp/eraser/nib shapes
the paint tools tint at runtime; the colour is supplied by the tool, so no
palette exists or is needed.

This was established by looking at the index values spatially (`src/bin/grid`):

* `MarkerStrokeBANANA02.pct` — a smooth banana-shaped ramp: 0 outside, 1-63
  on the antialiased edge, 192-255 in the interior.
* `ITeraserPHASE_A05_1.pict` — a uniform mid-coverage 40x40 field (a
  half-strength eraser).
* `callig_nib`, `cone_nib` — smooth nib-shaped gradients.
* `MarkerStrokeBirdFEET*`, `MarkerStrokeLIPS*` — the same kind of stamp
  coverage.

An earlier revision borrowed the closest preceding palette for these; that
was wrong and produced white/beige stamps (the banana "yellow" seen with the
old channel order was an artifact of the blue-channel bug). The extractor now
writes them as white RGBA with `alpha = byte`, which preserves the mask
exactly; `--original` writes the raw coverage bytes.

### 7.3 The 105 odd payloads (resolved: a two-section run-length format)

Their `attr`/`flags` bytes cluster as `attr=0x31, flags=0x1d` (98 of them),
plus a few `MITDrawer1Open032/033/034` (`attr=0x30, flags=0x0c/0x1c`). They
decompress to a length that matches **no** raw pixel layout, because they are
not raw pixels at all: the payload is a **run-length description**.

```text
[kind == 1]  1024 bytes  palette (skipped by the loader when kind is 1)
u16[2h+1]                big-endian offset table, h = the container's height
    entries[0]           the table's own byte size, 2*(2h+1); it is also the
                         offset at which the data begins
    entries[0..=h]       byte offset where row i's run-length slice starts
    entries[h..=2h]      byte offset where row i's colour slice starts
bytes                    run lengths, then colours, concatenated
```

Row `i`'s run lengths are the bytes `[entries[i], entries[i+1])` and **sum to
the image width**. The runs **alternate background / foreground, starting with
background**, so the runs at odd indices paint pixels and the ones at even
indices are fully transparent.

Row `i`'s colour slice `[entries[h+i], entries[h+i+1])` holds **exactly one
colour per foreground pixel**, consumed left to right:

* `kind == 1` — one **palette index** per foreground pixel, into the
  1024-byte prefix;
* `kind == 0` — four bytes per foreground pixel, `(T, R, G, B)`, the same
  transparency-first order as everywhere else.

The single fact that unlocked this was that the colour slice is **one colour
per foreground pixel**, not one per run. That is why the colour slice is much
longer than the run list — the per-row slice lengths follow a bell curve
because the middle rows have the longest foreground runs — and why rows that
are a single full-width background run have an **empty** colour slice.

Verified on `HD.MMP` with `src/bin/oddfix`, which scores
`sum(foreground run lengths) == number of colour records` per row:

| resource | dims | kind | rows matching |
|---|---|---|---|
| `ISselectBoxFlipSelect.pct` | 44x44 | 1 | 44 / 44 |
| `ISselectBoxROLLON.PCT` | 60x60 | 1 | 60 / 60 |
| `MITDrawer1Open032.PICT` | 16x20 | 0 | 20 / 20 |
| `ISsingleStampBWRender.PCT` | 60x60 | 1 | 60 / 60 |

The same format is present in the sequel's `app.MMP`, where 1,177 of 1,181
odd resources carry the identical `2h+1` table (`src/bin/oddscan`).

The last row of a `kind == 0` payload carries a few trailing bytes past the
width sum; they are not pixels and are ignored.

Rendered results confirm the reading rather than merely satisfying it:
`ISsingleStampBWRender.PCT` decodes to a greyscale ramp (255 / 128 / 45 / 9),
matching its name; `ISselectBoxFlipSelect.pct` produces white, red
`(205,29,20)` and 47 distinct semi-transparent tones — a selection highlight.
All 105 resources now decode; `HD.MMP` has **zero** failures.

---

## 8. Colour tests

Colour correctness is covered by automated tests, added after a
channel-order bug tinted the whole corpus yellow:

* **Unit tests** (`src/picture.rs`): the exact byte mapping
  `[transparency,R,G,B] → RGBA` (including blue and half-transparent cases),
  the mask expansion (white RGB, byte as alpha), and the palette entry order.
* **Named-sprite tests** (`tests/colors.rs`):
  * `ITXTmustard24` → dominant opaque RGB(255, 239, 90), yellow;
  * `ITXTcarrot24` → contains green foliage;
  * `ISbigBALc` → contains warm colours *and* a real cyan/blue;
  * `IStriFIRa01` (8bpp palette) → contains orange RGB(254, 151, 4);
  * `MarkerStrokeBANANA04` and `ITeraserPHASE_A05_1` → bare 8bpp decodes as
    a white coverage mask with many alpha levels.
* **Corpus scan** (`corpus_has_no_systematic_zero_blue_channel`): decodes all
  30,299 extracted PNGs and asserts that the fraction of opaque pixels with
  blue = 0 is below 15% and comparable to the red/green fractions. Current
  measurement: red 5.81%, green 5.45%, blue 8.71% (the old bug made blue
  exactly 100%). Pure black is only 4.30%.

`cargo test --release` runs the whole suite (17 tests). The standalone
`src/bin/imgcheck` prints the same corpus statistics for manual checks.

---

Progress/resume notes for this session live in `/workspace/STATUS.md`;
the extractor source is `/workspace/mmf-extract`.
