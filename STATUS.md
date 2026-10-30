# DMA (\*\*\*\*\*\*'s Magic Artist Deluxe / Magico Artista) — asset extraction status

**Game 3 (\*\*\*\*\*\*'s Magic Artist Deluxe, 2002): all four asset types are
DONE** (pictures, sounds, scripts, films). Format write-ups live in
`/workspace/docs/`: `PICTURES.md`, `SOUNDS.md`, `SCRIPTS.md`, `FILMS.md`.

**Game 2 (\*\*\*\*\*\* Magico Artista 2, 1999 — the earlier build, despite the
numbering) is also extracted** — same engine, same container family, two
format differences handled; see `docs/SEQUEL.md`. Series order: this repo's
primary target is the **third** game in the series.

**Game 1 (\*\*\*\*\*\* Magico Artista 1, 1997) is partly extracted.** Its
`MMFW` containers are **version 1**, and the whole first game used to be
stuck on one thing: version 1 stores no dimension table, so the extractor
could not know any picture's geometry and 12,935 of 14,026 pictures failed.
That is fixed. The dimension table was always there — the version-1 metadata
block is `15*N + 4` bytes long and holds `{u16 height, u16 width}` pairs, and
the offsets table starts at `0x20` rather than `0x24`. See `docs/GAME1.md` for
the measured layout; `tests/game1.rs` pins it down.

Current game-1 totals: **9,840 of 12,961 pictures**, 451 of 452 sounds, 305
blobs, `--original` byte-exact for 13,717 of 13,718 resources. `HD.PIC` and
`Topolino.CPK` extract with zero failures; `pack.PIC` reaches 7,882 of 10,893.

---

## 1. Where the work is

| What | Path |
|---|---|
| Final Rust extractor (the deliverable) | `/workspace/mmf-extract` |
| Builds offline | `cargo build --release` → `target/release/mmf-extract` |
| Extracted pictures (30,404 PNGs) | `/workspace/extracted/Pictures/…`, raw payloads in `/workspace/extracted/Raw/…` |
| Extracted scripts (4,813 objects) | `/workspace/extracted/Scripts/HD/…` |
| Extracted films (139 frames) | `/workspace/extracted/Films/DMA/00000_Intro/…` |
| Extracted sounds (683 WAVs) | `/workspace/extracted/Sounds/{HD,app}/…` |
| Format findings | `/workspace/docs/PICTURES.md` (+ `SOUNDS.md`, `SCRIPTS.md`, `FILMS.md`) |
| First game (1997) format | `/workspace/docs/GAME1.md` |
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
| `app.MMP`, `HD.MMP`, `stmp1.MMP`, `stmp2.MMP`, `stmp3.MMP` | Pictures | images | **DONE** — 30,404 PNGs, 0 failures (the 105 odd payloads now decode) |
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
* 105 odd payloads are a **two-section run-length format**, now fully decoded
  (see `docs/PICTURES.md` §7.3). After the optional 1024-byte palette there is
  a big-endian u16 table of `2*height+1` entries whose first entry is the
  table's own byte size; the first half of the table addresses each row's run
  lengths (which sum to the width and alternate background/foreground starting
  with background), and the second half addresses each row's **colour records —
  one per foreground pixel** (a palette index when kind is 1, four bytes
  `(T,R,G,B)` when kind is 0). The same format is in the sequel's `app.MMP`.

Tests: `cargo test --release` → **33/33 pass** against the real assets
(5 unit tests, 8 in `tests/roundtrip.rs`, 6 in `tests/colors.rs`, 4 in
`tests/sounds.rs`, 3 in `tests/scripts.rs`, 2 in `tests/film_roundtrip.rs`). The color tests include a corpus-wide scan
of `/workspace/extracted/Pictures` (30,404 PNGs): it fails if blue is zero on
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

* Source: the original CD image `TOPOLINO2.ISO` (479,662,080 bytes, MD5
  `0fbcca46cd301925c65f1fba4cdc8836`, verified). Only the system area and the
  game's own files were read from it.
* Two format differences: containers are **version 2** (the parser demanded
  3 and rejected every file), and films use marker **5** with 22-byte
  records, chunk numbers `0x00`/`0x01`, an unpadded raw key frame and
  RLE deltas with no LZ77 wrapper. The film record length is solved from the
  file layout, so both games parse. The 1,181 "odd payload" pictures decode with the same two-section run-length format as game 3 (`docs/PICTURES.md` §7.3), so `app.MMP` and `HD.MMP` now extract with **zero** failures.
* Extracted: **39,628 pictures**, **891 sounds**, **1 film** (193 frames of
  800×600). 1,181 pictures used the same undecoded "odd payload" sub-format as
  game 3 (`ITstampareaHOTSPOT*`, `ttpmo_*`, `TtuPt_*`, …); that format is now
  solved — see `docs/PICTURES.md` §7.3 — so they decode too.
* Unresolved: the second game's `Hd.mms` (`MMFW 3 Script`, `II`, version 2)
  does not match the game-3 object database layout, and 7 of 147 film deltas
  drive the cursor negative.

## 9. Remaining work / known gaps

* ~~"odd" picture payloads~~ — **solved**: 105 in game 3 and 1,181 in game 2
  are a two-section run-length format (`docs/PICTURES.md` §7.3). Both games'
  picture containers now extract with **zero** failures.
* `docs/SOUNDS.md`, `docs/SCRIPTS.md` and `docs/FILMS.md` each end with an
  open-questions list (animation opcodes, film record fields and audio PCM,
  the `0x0008` metadata word, `C` bits 4-7).
Deps available: `anyhow`, `clap`, `flac`, `hound` (WAV), `png`, `sha2`.

---

## 10. The first game's version-1 containers (see `docs/GAME1.md`)

* Source: the public disc image
  <https://archive.org/download/disney-magico-artista/TOPOLINO.ISO>
  (574,132,224 bytes, range-capable; three uploads of the same Windows tree
  exist on archive.org — `disney-magico-artista`, `MagicoArtista`,
  `disney_magico_artista`). Its `Dati/` folder holds the ten containers.
  Working copy carved at **`/PathAssets/Dati`** (265 MB) with `iso_range.py`,
  so `tests/game1.rs` finds it with no `DMA_ASSETS`/`GAME1_ASSETS` set;
  `docs/GAME1.md` §1 tabulates every file's LBA and size so the carve is
  reproducible. The executable cited below as `DMA1.exe` is the disc's
  `Disney's Magic Artist.exe`, carved as `DISNEYMA.EXE` (1,217,536 bytes, i386
  PE, preferred base `0x400000`). The
  image is a fuller dump than the 548,222,976-byte one the earlier notes used:
  the ISO9660 tree ends at sector 147,066 and the rest is the Mac side of this
  Win/Mac hybrid disc, referenced by no directory record. The Windows side is
  byte-identical on both (`pack.BLB` is 73,074,150 bytes on each) and every
  count in this section was re-measured against it. Beware
  `disney-magico-artista-pc-italy/Topolino.bin`: it is a raw `MODE1/2352`
  image, which `iso_range.py` cannot parse.
* **Version 1 parses from `0x20`, not `0x24`.** The resource count is the u16
  at `0x1E` and is `N` (not `N+1`); the offsets table holds `N+1` entries with
  the last equal to the file size. Reading from `0x24` shifts every resource by
  one and drops the first, which is why `pack.BLB` used to yield 304 of 305
  records.
* **Version 1 *has* a per-resource metadata block**, `15*N + 4` bytes plus the
  palette bank: a `4*N` u32 handle array, a `0x0007` marker, `N` flags bytes,
  `N` kind bytes, the `4*N` `{u16 height, u16 width}` **dimension table**, a
  `4*N` **uncompressed-size** table, one `N` byte array, a u16 palette count
  and the palette bank. Sounds are `5*N + 2` (the handle array holds Mac
  timestamps, then the format byte); blobs are `4*N + 2`.
* Extracted: **12,961 of 12,961 pictures (zero failures), 451 of 452 sounds,
  305 blobs**; `--original` is byte-exact for 13,717 of 13,718 resources.
  Per container: `HD.PIC` 1682, `mpApp.PIC` 384, `pack.PIC` 10893,
  `Topolino.CPK` 2. Split by storage form: 9,733 stored plain (`size == 0`),
  3,228 LZSS streams (3,103 with `size > len`, 125 with `size == len`).
* 590 of `HD.PIC`'s pictures use the **same two-section run-length sub-format**
  as games 2 and 3 (`docs/PICTURES.md` §7.3), so that decoder covers all three
  games.
* **Version 1 keeps its palettes in a bank at the end of the metadata
  region**, not prefixed to the payload, so the run-length decoder needed the
  bank plumbed through to read the `kind 0x17` pictures in `pack.PIC` whose
  colour records are single palette indices. `mpApp.PIC` ships two
  entries with no per-resource selector; entry 0 is a vision-confirmed
  judgement, not a measurement.
* **The compressed payloads are LZSS, and the decompressor is `FUN_42e0d4` in
  `DMA1.exe`.** Not the games-2/3 LZ77 (`FUN_004c557a` in `DMA4.exe`):
  byte-oriented LZ77 with a match table there, a 1024-slot ring with
  fixed-width tokens here. Token layout: one control byte per eight tokens,
  LSB first, bit 1 = literal and bit 0 = a match; a match is two big-endian
  bytes holding `distance = word & 0x3FF` and `length = (byte0 >> 2) + 3`, i.e.
  the length lives in the word's **upper** six bits
  (`((length - 3) << 10) | distance`). The window's 1024 slots are zeroed on
  entry (`0x42e0f7`, counting to `0x3be`) and the write cursor starts at 958
  (`0x42e104`), so an encoded `distance` is a **window slot**, not a count of
  bytes back — with the cursor at 958, `n` bytes back is
  `(958 + p - n) mod 1024` in the word. The game's window is the fixed global
  at `0x517540`, so the port zeroes the whole ring (equivalent, and it avoids
  stale bytes between calls). Lives in `src/v1/lzss.rs`.
* **`size == len` still means compressed.** 125 `pack.PIC` resources declare a
  size equal to their own payload length; they look stored but the bytes
  differ, and reading that column as "not compressed" is what left 18 of them
  failing after the other 3,103 were fixed (the other 107 decoded anyway).
  Measured with `v1pic`: `pack.PIC` 7,775 plain / 3,118 lzss.
* **Version-1 Script containers now open, and are byte-exact in `--original`**
  (1,283 objects: `HD.mms` 295, `pack.mms` 988). They used to fail to parse
  entirely. They share neither shape with versions 2/3 — **no names table and
  no metadata region**; there is an offsets table, a `u16` measured as 4, then
  the payloads. Verified on both containers: the first payload starts at
  `0x20 + 4*(N+1) + 2` and the last offset equals the file length. See
  `GAME1.md` §7 and `src/v1/scripts.rs`.
* Unresolved: the `goofy 2.6.3` blob payload format (305 resources, likely the
  game's videos), the **per-class record size** in a v1 script payload (the
  count's distribution is strongly peaked — 210 `pack.mms` payloads with a
  lead of 14 are all exactly 152 bytes — but no single record width fits the
  payloads, which is the `type_aux` signature), and one sound
  (`SifBtS10.aif`) whose format byte disagrees with its length. `GAME1.md` §6.1
  now records what *is* measured about the blobs so the next attempt starts from
  there: the stamp is a watermark the loader never validates (no `cmp`/`mov` of
  those bytes in `DISNEYMA.EXE`; the string's single reference is a data table),
  the body repeats on an 8-byte `(u32, u16, u16)` period, no container magic
  occurs in any payload, and the body draws from a 32-symbol alphabet at 5.04
  bits/byte — a coded stream, not a container. The furthest traced entry point
  is `push "session done sound"` at `0x4a77a5`, in a constructor at `0x4a7763`.
* *Corrected:* `src/film.rs` attributed the version-3 film layout (marker 7,
  26-byte records, chunk types `0x0A`/`0x0B`, padded 8bpp rows, LZ77-wrapped
  deltas) to "the first game". Those measurements are of `DMA.MMF`, which is
  **version 3**; `SEQUEL.md` already had it right. Renamed throughout, and
  version 1 is now rejected in `parse_film_metadata` instead of being folded
  into the third game's path, because no Films container ships with the first
  game (`docs/GAME1.md` §1) and assuming otherwise is a guess.
* Code layout: everything version-1 specific lives in **`src/v1/`**
  (`lzss.rs`, `metadata.rs`, `picture.rs`). `container.rs` keeps only the
  dispatch (count at `0x1E`, table at `0x20`, region handed to
  `v1::metadata::parse`); `picture.rs`/`lz77.rs`/`sound.rs`/`film.rs`/
  `script.rs` contain no version-1 branch. `cli.rs::picture_decoder` and
  `cli.rs::picture_payload` are the only two places in the program that map a
  version to code, which is what keeps one game's edits off the other's path.
* Tests: `tests/game1.rs` pins the measured layout claims and the
  `--original` byte-exactness (plain = the container slice, compressed = the
  LZSS expansion to its declared size); `tests/game1_payloads.rs` carries an
  **independent** LZSS encoder written from the token layout, so the
  decompressor is checked against a second implementation rather than against
  itself, plus a synthetic version-1 container so the `--original` path is
  pinned without the CD mounted.
* Tools: `src/bin/v1pic` reports per-resource dims/kind/storage/layouts, and
  now prints both the stored length and the expanded one, because the gap
  between them is what says a payload was compressed.
