# DMA (Disney's Magic Artist Deluxe / Magico Artista) — asset extraction status

**Game 3 (*Disney's Magic Artist Deluxe*, 2002): all four asset types are
DONE** (pictures, sounds, scripts, films). Format write-ups live in
`/workspace/docs/`: `PICTURES.md`, `SOUNDS.md`, `SCRIPTS.md`, `FILMS.md`.

**Game 2 (*Disney Magico Artista 2*, 1999 — the earlier build, despite the
numbering) is also extracted** — same engine, same container family, two
format differences handled; see `docs/SEQUEL.md`. Series order: this repo's
primary target is the **third** game in the series.

---

## 1. Where the work is

| What | Path |
|---|---|
| Final Rust extractor (the deliverable) | `/workspace/mmf-extract` |
| Builds offline | `cargo build --release` → `target/release/mmf-extract` |
| Extracted pictures (30,299 PNGs) | `/workspace/extracted/Pictures/…`, raw payloads in `/workspace/extracted/Raw/…` |
| Extracted scripts (4,813 objects) | `/workspace/extracted/Scripts/HD/…` |
| Extracted films (139 frames) | `/workspace/extracted/Films/DMA/00000_Intro/…` |
| Extracted sounds (683 WAVs) | `/workspace/extracted/Sounds/{HD,app}/…` |
| Format findings | `/workspace/docs/PICTURES.md` (+ `SOUNDS.md`, `SCRIPTS.md`, `FILMS.md`) |
| Workspace layout contract | `/workspace/docs/WORKSPACE.md` |
| Agent instructions | `/workspace/AGENTS.md` (agents.md format) |
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
| `HD.MMA`, `app.MMA` | Sounds | audio | **DONE** — 683 WAVs (8-bit PCM + custom 16-bit DPCM) |
| `HD.MMS` | Scripts | object DB | **DONE** — 4,813 objects, 3,329,791 payload bytes fully accounted |
| `DMA.MMF` | Films | video/frames | **DONE** — 1 film, 139 frames of 546×410 + manifest |

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

Tests: `cargo test --release` → **28/28 pass** against the real assets
(5 unit tests, 8 in `tests/roundtrip.rs`, 6 in `tests/colors.rs`, 4 in
`tests/sounds.rs`, 3 in `tests/scripts.rs`, 2 in `tests/film_roundtrip.rs`). The color tests include a corpus-wide scan
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

## 5. Films — what is solved (see `docs/FILMS.md`)

* `DMA.MMF` holds **one** film, `Intro.flm` (6,856,628 bytes). The Films
  metadata block is `marker = 7`, one 26-byte per-film record (its four u16s
  at `+2` are `top/left/bottom/right` = 0/0/410/546, matching the decoded
  frame geometry) and one 1024-byte palette in the picture byte order.
* The container `sizes[0]` for Films is **not** a decompressed size
  (26,870,306 vs the real 6,856,628); `--original` writes `payload(0)`
  verbatim and is SHA-256 verified byte-identical to the container slice.
* The inner `.flm` stream is a sequence of `0xF789` chunks:
  `0x0A` full LZ77 frame, `0x0B` delta (20 rects + LZ77 + cursor-relative
  RLE), `0x06` timestamp, `0x02` audio block, `0x03` marker, `0x07` end.
  412 chunks total.
* Decoded 139 frames (133 unique, 6 duplicate timestamps), 83-tick spacing
  (11,537 ticks), 132 deltas decode to their declared sizes; all writes are
  bounds-checked. Audio PCM and the five fields at record `+10..+26` remain
  uninterpreted.
* Modern output: `frame_000.png … frame_138.png` (546×410, palette-expanded,
  lossless) plus `manifest.json`. `tests/film_roundtrip.rs` re-decodes every
  PNG back to the exact palette-expanded RGBA bytes.
* `picture::stride8` / `picture::indexed_rgba` are now `pub(crate)` so the
  film decoder reuses the verified palette expansion.

## 6. Sounds — what is solved (see `docs/SOUNDS.md`)

* Sound metadata is **`14 * N + 2` bytes**, not the picture layout's
  `15 * N + 4`: a `4*N` Mac-timestamp u32 array, a 2-byte word (`0x0008` in
  both shipped files), the format byte array `C`, the flags byte array `E`, the
  (zero) dimension table and the decoded-size table. `Container::open` now
  branches on this; there is no trailing kind array.
* `C` (attr byte) = playback format: bits 0-1 sample rate (11025/22050/44100),
  bit 2 = 16-bit, bit 3 = stereo. `E & 3` = encoding selector: 0 = stored PCM,
  nonzero = the game's differential codec.
* Two encodings: **unsigned 8-bit PCM** stored verbatim (192 resources), and a
  table-driven **differential codec** (491 resources) whose 256-entry `i16`
  predictor table lives at `DMA4.exe` offset `0x132778` (decoder
  `FUN_004c6b80`): one stored byte → one signed 16-bit sample, with per-channel
  predictors and a silence code `0xF8`. Decoded size is `2 ×` stored bytes.
* All 683 resources decode with zero failures; the decoded-size table is used
  as an integrity check and any mismatch is reported and skipped.
* `--original` writes the decoded sample stream verbatim as `.pcm`; modern mode
  writes `WAVE_FORMAT_PCM` WAVs with the rate/channel/bit depth from `C`
  (lossless: the tests read each WAV back with `hound` and compare samples).
* Known-file checks: `Ball30sec.AIF` → 28.799 s at 22,050 Hz stereo, and
  `silence.aif` → exactly one second of zeros — both confirm the format byte.
* Open: the `0x0008` word and `C` bits 4-7 are not yet interpreted; the codec
  has no vendor name in the executable.

## 7. Tooling notes

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
  `scriptprobe`, `filmprobe`.
* crates.io reachable; `cargo build --release` works offline.

## 8. The second game in the series (see `docs/SEQUEL.md`)

* Source: archive.org item `disney-magico-artista-2`, file `TOPOLINO2.ISO`
  (479,662,080 bytes, MD5 `0fbcca46cd301925c65f1fba4cdc8836`, verified). Only
  the system area and the game's own files were read from it.
* Two format differences: containers are **version 2** (the parser demanded
  3 and rejected every file), and films use marker **5** with 22-byte
  records, chunk numbers `0x00`/`0x01`, an unpadded raw key frame and
  RLE deltas with no LZ77 wrapper. The film record length is solved from the
  file layout, so both games parse.
* Extracted: **38,447 pictures**, **891 sounds**, **1 film** (193 frames of
  800×600). 1,181 pictures are the same undecoded "odd payload" sub-format as
  game 3 (RLE-like: `ITstampareaHOTSPOT*`, `ttpmo_*`, `TtuPt_*`, …).
* Unresolved: the second game's `Hd.mms` (`MMFW 3 Script`, `II`, version 2)
  does not match the game-3 object database layout, and 7 of 147 film deltas
  drive the cursor negative.

## 9. Remaining work / known gaps

* 105 "odd" picture payloads in `HD.MMP` (an effect-specific sub-format) are
  still undecoded and reported as failures; see `oddparse_HD.txt`.
* `docs/SOUNDS.md`, `docs/SCRIPTS.md` and `docs/FILMS.md` each end with an
  open-questions list (animation opcodes, film record fields and audio PCM,
  the `0x0008` metadata word, `C` bits 4-7).
Deps available: `anyhow`, `clap`, `flac`, `hound` (WAV), `png`, `sha2`.
