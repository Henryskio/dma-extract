# DMA (Disney's Magic Artist Deluxe / Magico Artista) — asset extraction status

**Pictures and scripts are DONE.** This file is the resume point for the
remaining asset types (sounds, films), which are being reverse-engineered by
subagents. Format write-ups live in `/workspace/docs/`: `PICTURES.md` and
`SCRIPTS.md` (finished), plus `SOUNDS.md`/`FILMS.md` as each is understood.

---

## 1. Where the work is

| What | Path |
|---|---|
| Final Rust extractor (the deliverable) | `/workspace/mmf-extract` |
| Builds offline | `cargo build --release` → `target/release/mmf-extract` |
| Extracted pictures (30,299 PNGs) | `/workspace/extracted/Pictures/…`, raw payloads in `/workspace/extracted/Raw/…` |
| Extracted scripts (4,813 objects) | `/workspace/extracted/Scripts/HD/…` |
| Format findings | `/workspace/docs/PICTURES.md` (+ `SOUNDS.md`, `SCRIPTS.md`, `FILMS.md`) |
| Game archives (read-only) | `/RawAssets/` (see table below) |

CLI of the deliverable (`src/main.rs`, `src/cli.rs`):

```
mmf-extract ASSET_DIR [-o|--output DIR] [-t|--types all,pictures,sounds,films,scripts] [--original] [--raw]
```

* `ASSET_DIR` — folder of unextracted assets (positional).
* `--output` — defaults to the current working directory.
* `--types` — comma-separated list, default `all`.
* `--original` — write the game's own formats instead of modern ones.

## 2. The container files and their types

The `MMFW ` signature + tag (at offset 0) names the kind. `Container::open`
(`src/container.rs`) already parses the shared header/names/offsets/metadata
for **all** of them:

| file | tag | type | status |
|---|---|---|---|
| `app.MMP`, `HD.MMP`, `stmp1.MMP`, `stmp2.MMP`, `stmp3.MMP` | Pictures | images | **DONE** — 30,299 PNGs, 105 "odd" reported failed |
| `HD.MMA`, `app.MMA` | Sounds | audio | stub writes payloads verbatim — RE in progress |
| `HD.MMS` | Scripts | object DB | **DONE** — 4,813 objects, 3,329,791 payload bytes fully accounted |
| `DMA.MMF` | Films | video/frames | stub writes payloads verbatim — RE in progress |

> **Engine identification.** `MMFW` is the game's **in-house** container
> family, not Clickteam Multimedia Fusion 2. Evidence from `DMA4.exe`: PDB
> path `C:\Projects\DeluxeRevision\DMA Art Ware\Intermediate\Release\DMAAW.pdb`;
> MSVC 6 link date 2002-06-15; imports only Win32/MFC/QuickTime-era DLLs;
> loader errors prefixed `MME_*` (`MME_PICTURE_RESOURCE_FILE_BAD_HEADER`, …);
> no `Clickteam`, `CNC`, or `MMF2` string anywhere in the binary. Treat MMF2
> documentation as a non-authoritative prior only; everything here is verified
> against the shipped files and `FUN_*` code in `DMA4.exe`.

## 3. Pictures — what is solved (see `docs/PICTURES.md`)

* Container header, names table, metadata region (`15*N+4` bytes), endianness,
  LZ77 codec (`src/lz77.rs`, round-trips), CRC-32.
* Pixel channel order **byte0 = transparency (255 = fully transparent),
  byte1 = R, byte2 = G, byte3 = B**; `alpha = 255 - byte0`; dims
  `{height,width}`; 8bpp stride `((w+3)/4)*4`.
  - Confirmed on named sprites: `ITXTmustard24` → RGB(255,239,90),
    `IStriFIRa01` → RGB(254,151,4) orange, `ISbigBALc` → pink/purple/yellow
    plus cyan.
  - An earlier revision read `[B,A,R,G]`, which forced B=0 on every opaque
    pixel and tinted the corpus yellow. Regression tests live in
    `tests/colors.rs` + unit tests in `src/picture.rs`.
* **`sizes[i] == 0` ⇒ payload stored uncompressed** (skip LZ77).
* Metadata columns (decompiler-confirmed): `4N`=attr, `5N`=flags (bit0=bpp),
  trailing=kind/palette-source. **Payload length is ground truth** for layout.
* 484 bare 8bpp payloads are **coverage/alpha masks** (each byte is alpha,
  0 = transparent; white + alpha in PNG), *not* palette images. Verified
  spatially with `src/bin/grid` (banana-shaped ramp, uniform eraser field,
  nib gradients). The earlier "borrowed palette" rule was wrong and removed.
* 105 odd payloads (a second-stage/effect sub-format) are **not** decoded —
  reported as failures. Decoder matrix in `/workspace/oddparse_HD.txt`.

Tests: `cargo test --release` → **20/20 pass** against the real assets
(8 in `tests/roundtrip.rs`, 6 in `tests/colors.rs`, 3 in `tests/scripts.rs`,
3 unit tests in `src/picture.rs`). The color tests include a corpus-wide scan
of `/workspace/extracted/Pictures` (30,299 PNGs): it fails if blue is zero on
more than 15% of opaque pixels or is far more often zero than red/green.
Current measurement: R 5.81%, G 5.45%, B 8.71%.

## 4. Scripts — what is solved (see `docs/SCRIPTS.md`)

* `HD.MMS` is **not event bytecode**. It is a serialized global object
  database: **4,813 named objects in 89 classes**, little-endian, metadata
  version 11.
* It has no 32-byte names table and no picture metadata. Immediately after
  the offset table: the type table (per-class record size + class names),
  per-object element counts, per-object type indices, and the object-name
  string table, ending exactly at the first payload.
* Payloads are **stored uncompressed**; `off[i+1] - off[i]` is the length.
  Each payload is `element_count × record_size` fixed records plus an optional
  class-specific tail (Animation name, credits text, `StdBrush` names,
  `AnimBrush` references, `TextList`/`StringTable` blobs).
* Measured coverage: **3,329,791 payload bytes = 3,282,998 record bytes +
  46,793 tail bytes**, with **0 opaque bytes**.
* `--original` writes 4,813 verbatim `.bin` files; modern mode writes
  `container.json`, `types.txt`, `strings.txt` and 4,813 annotated per-object
  dumps. Decoded strings include the 130-entry UI/localization `StringTable`,
  credits text, brush names, class names and object names. The Animation
  records are exposed as `op` + four u16 arguments; the opcode table is still
  unknown.
* `tests/scripts.rs` verifies every `--original` file byte-for-byte against
  its `[off[i], off[i+1])` container slice.

## 5. Tooling notes

* **Ghidra 12.2 headless** at `/root/ghidra_12.2_PUBLIC/support/analyzeHeadless`.
  Project `/tmp/ghproj/DMA` already imported (`-process DMA4.exe -noanalysis`).
  Script `/tmp/ghscripts/DecompAddrs.java <addr>…` disassembles + decompiles
  named addresses. Run with `GHIDRA_HEADLESS_MAXMEM=2G` (8 GB box). Delete
  stale `/tmp/ghproj/DMA.lock*` if a run is killed.
  * Useful entry points already decompiled (logs in `/tmp/ghidra1.log`,
    `/tmp/ghidra3.log`): `FUN_004a80f7` (picture loader), `FUN_004c4038`
    (picture-file open / metadata read order), `FUN_004c23ed` (a container
    header reader), stream readers `FUN_004b4387` (u32), `FUN_004b596e`
    (u32→{u16,u16}), `FUN_004b6cf8` (1024-byte palette).
* **REA MCP**: `open_binary` works on `/RawAssets/DMA4.exe`, but
  `procedure_pseudo_code` rejects installed Ghidra 12.2 (wants 12.1.x).
* Python 3 is installed (systems without it can use the Rust bins in `src/bin/`).
  Analysis bins present: `analyze`, `probe`, `owners`, `oddparse`, `cols`,
  `invariants`, `imgstat`, `imgcheck`, `chan`, `palscan`, `grid`, `dump`,
  `scriptprobe`.
* crates.io reachable; `cargo build --release` works offline.

## 6. Next steps (sounds / films)

Each type is being handled by a subagent that:
1. Parses its container with `Container::open` + a custom probe bin.
2. Reverse-engineers the per-resource payload encoding (magic bytes first,
   then Ghidra for the decoder).
3. Documents findings in `docs/SOUNDS.md` / `FILMS.md`.
4. Implements extraction in `src/sound.rs` / `src/film.rs` to a modern
   lossless format, and a `--original` verbatim path.
5. Adds a round-trip test and runs `cargo test --release`.

Deps available: `anyhow`, `clap`, `flac`, `hound` (WAV), `png`, `sha2`.
