# Odd-payload sub-format — findings so far (working notes)

These are scratch notes for the investigation of the 105 picture resources in
`HD.MMP` (game 3) and the 1,181 in game 2 whose decoded length matches no known
pixel layout. Kept outside `docs/` until the format is solved.

## Ground truth already established

The decompressed payload (after the container's LZ77 stage, which is correct and
unchanged) has this shape:

```text
[kind == 1]  1024 bytes   palette (the loader skips these when kind is 1)
u16[N]                      offset table, big-endian, ASCII order as stored
bytes                       run-length and colour data
```

`N = 2*height + 1`, where `height` comes from the container's dimension table.
`entries[0]` is **the byte size of the table itself** (`2*N`), which pins the
table's start and length exactly.

Verified on `HD.MMP` resource 6221 (`ISselectBoxFlipSelect.pct`, 44x44, kind 1,
flags 0x0c, attr 0x10, decompressed 2320 bytes):

* table at payload offset 1024, 89 entries, `entries[0] = 178 = 89*2`
* `data = payload[1024+178 ..]`, 1118 bytes
* Reading the entries as consecutive slice boundaries, **slices `[0..43]` are 44
  rows and every one of them sums to exactly 44 = the image width.**
* Rows 0-7 are a single byte `0x2c` (= 44, one full-width run).
* Rows 8-41 are three bytes each, e.g. `[0x10,0x0d,0x0f]` = 16+13+15 = 44.
* Row 42 is seven bytes `[0x14,0x01,0x01,0x05,0x01,0x01,0x0f]` = 44.
* Row 43 is one byte `0x2c`.
* Slices `[44..87]` are 44 further slices whose lengths follow a bell curve
  (0,0,0,0,0,0,0,0,13,18,22,24,27,27,30,31,32,33,34,34,34,35,35,35,35,35,35,
  35,34,34,33,33,31,31,29,27,25,23,21,19,14,7,0) and whose byte values are
  small **palette indices** (1..0x2e, all below the 48 meaningful palette
  entries). Their sums are *not* the width.

Total run lengths = 118 bytes; total second-section bytes = 1000.

The same shape holds for `ISselectBoxROLLON.PCT` (60x60, 121 entries) and
`ISsingleStampBWRender.PCT` (60x60, 121 entries).

## What is NOT yet known

**Where the colour for each run comes from.** The second section's slices hold
palette indices but do not line up one-per-run with the first section (row 8 has
3 runs but its slice is 13 bytes; row 42 has 7 runs and its slice is 7 bytes —
only row 42 matches).

## What has been ruled out

* PackBits, `(count,byte)`, `(count,3-byte)`, `(count,4-byte)` RLE with zero
  meaning 1 or 256 — 0 hits on all 105 (`/workspace/oddparse_HD.txt`).
* Splitting the table at `h` (halves) or at `2i` (interleaved) does not make the
  run counts and colour counts agree for every row.
* A second `LZ77` stage: `src/lz77.rs` has no format dispatch and already
  reproduces the container's recorded decompressed size exactly.
* Windows `BI_RLE8` over the whole post-palette region: the payload does not end
  in `00 01`.

## Binary facts already measured

DMA4.exe (SHA-256 `d8ff696e…`), Ghidra image base `0x400000`, `.text` =
`0x401000-0x4e6fff`.

* `FUN_004a8344` — the picture loader. Reads the container's size via
  `FUN_004c3ced`, reads the stored size via `FUN_004c22d2`, allocates via
  `FUN_004a8594`, decompresses via `FUN_004c557a`, then applies the kind/flags
  byte transforms (`FUN_004b6d3a`, `FUN_004c3aa6`, `FUN_004bdbd1`). It does
  **no** RLE.
* `FUN_004a80f7` / `FUN_004a81fe` — surface descriptors. Set bpp from
  `FUN_004c3c00` bit 0, buffer pointer at `+0x24`, stride at `+0x28`
  (`(width*(bpp/8)+3) & ~3`). Skip the palette with `+0x400` when kind is 1.
* Callers of the dims accessor `FUN_004c3caf`: only `0x4a80f7`, `0x4a81fe`,
  `0x4a8344`. So the row walker does **not** call that accessor — it gets width
  and height as parameters.
* The picture class's vtable is at `0x4f1834` (see `mov DWORD PTR [esi],0x4f1834`
  at `0x4a8008`). Entries include `0x4a861f, 0x4a859c, 0x4a85a4, 0x4a85fb,
  0x4a807f, 0x4a809e, 0x4a80c8, 0x4a80e3, 0x4a80f7, 0x4a81fe, 0x4a826e,
  0x4a7e26, 0x4a7e32, 0x4a7e6e, 0x4a82ff`.
* `FUN_004c3c98` (flags bits 2-3) has **no callers**, so the odd sub-format is
  not selected by that accessor. The `attr` byte lives at object offset `0x278`
  and no function in `0x4a8000-0x4a9000` references it.

## Tools built during the investigation (all in `src/bin/`, all build clean)

| bin | what it does |
|---|---|
| `oddparse` | the original negative decoder matrix |
| `oddsplit` | dumps metadata + first bytes + record probes |
| `oddfull` | full payload as u32 BE words, runs, group shape |
| `oddrow` | locates the u16 table, checks per-row byte sums vs width |
| `odddec` | decodes both table layouts and scores them |
| `oddslices` | dumps every slice the table describes, with sums |
| `oddsplit2` | scores every split point of the table |

## Reproducing

```sh
cd /workspace/mmf-extract && cargo build --release
./target/release/oddslices /RawAssets/HD.MMP 6221
./target/release/oddfull   /RawAssets/HD.MMP 6221
```
