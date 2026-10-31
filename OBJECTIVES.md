# Objectives — 2026-10-31 night shift

Written by the user before sleeping. Work through these autonomously; keep the
repo's conventions (`AGENTS.md`) and commit as you go. This file is the work
log — update it as you finish each item.

## Naming convention (unchanged, per instruction)

* **DMA1** — Disney's Magic Artist (1997), version 1. `docs/GAME1.md`.
* **DMA2** — Disney's Magic Artist Studio (1999), version 2. `docs/SEQUEL.md`.
* **DMA4** — Disney's Magic Artist Deluxe (2002), version 3. The repo calls it
  **"the third game"** and that stays: *"keep dma 4 as the third game for
  convention."* (`AGENTS.md`, `STATUS.md`, the shared `docs/` chain.)
* **DMA3D** — *Disney Creazioni in 3D* / Disney's Magic Artist 3D: the 3D
  game, new in this shift. Not part of the 1/2/4 numbering.

## 1. Test the extraction program on the English DMA Studio and DMA Deluxe

Both are Joliet CD images on archive.org, carved with `iso_range.py` (the
tool was fixed for Joliet this shift — see `docs/WORKSPACE.md`).

| game | item | iso | assets |
|---|---|---|---|
| DMA2 | `magicartiststudio` | `Studio.iso` | ISO root: `HD.MMA HD.MMP HD.MMS app.MMA app.MMP stmp1.MMP stmp2.MMP DMA.MMF` |
| DMA4 | `DMADeluxe` | `DMADeluxe.iso` | `game/Data/`: `DMA.MMF HD.MMA HD.MMP HD.MMS app.MMA app.MMP stmp1.MMP stmp2.MMP stmp3.MMP`; also `game/DMA4.exe` |

Done: run `mmf-extract` on each, report resource counts, failures and
warnings. Compare DMA4's counts against `/RawAssets` (the Italian DMA4).
`/RawAssets` is read-only input — carve to `/tmp`, never into it.

Italian DMA1 (`MICKEYMA`) was verified earlier this shift: 12,961 PNG,
451 FLAC (223 8-bit + 228 16-bit), 1,279 scripts, 305 blobs, 1 failure —
the `SifBtS10.aif` 7817-byte defect that the Italian release also has.

## 2. Reverse engineer the 3D game and extract its assets in Rust

Source: `/RawAssets/Disney Creazioni in 3D (Italy).zip` (read-only input —
unzip *out* of `/RawAssets`, e.g. to `/tmp`).

* Survey what is in the archive: file types, sizes, any containers, and
  whether the 3D models are a **standard** format (`.X`, `.3DS`, `.ASE`,
  DirectX, Wavefront) before writing any decoder.
* If the containers are too different from versions 1/2/3, put the code in a
  **subfolder** — `src/v1/` is the worked example of the convention. Do not
  fork `src/container.rs` per game.
* Expect 3D models. `docs/` gets a new chain document per game if this one
  needs its own (never one document describing two games).
* Same rules as the rest of the project: lossless output, `--original`
  byte-exact, per-resource failures not fatal, a 1:1 test for every decoder.

## 3. Test the extraction on the English 3D game

`https://archive.org/details/DisneysMagicArtist3D` — carve its assets and run
the extractor from item 2 against them. Report counts and failures.

## Status

- [x] **1. DMA Studio + DMA Deluxe English** — both extract with **zero
  failures**. DMA Studio (English version 2): 39,628 pictures, 891 sounds,
  1 film (193 frames), 7 warnings — the 7 undecodable delta chunks that the
  Italian build also has. DMA Deluxe (English version 3): 30,404 pictures,
  683 sounds, 4,813 script objects, 1 film (139 frames), **0 warnings**.
  Both counts match the Italian releases resource-for-resource except the
  pictures, which are higher by exactly the 105 / 1,181 odd payloads the
  run-length fix added (`docs/PICTURES.md` §7.3) — the README's baseline
  numbers predate that fix.
- [x] **2. RE the 3D game** — containers identified (`MMFW Models` N=460,
  `MMFW Textures` N=705, both container version 3), metadata regions
  measured (`4*N+100`, `6*N+2`), and **1,165 payloads extracted byte-exact**
  on the Italian disc (460 `.mod`, 705 `.tex`). The payload *formats* are
  documented as open: `.mod` begins `mod II` then a little-endian stream of
  `u32`s and floats, and the textures carry a per-resource palette selector
  that is not yet identified. `DMA3D.MMF` (Films) does not decode in this
  build. See `docs/DMA3D.md`.
- [x] **3. English 3D game** — *Disney's Magic Artist 3D (2000)* carved and
  extracted: the same 3 pictures, 145 sounds, 647 scripts, 460 models, 705
  textures, and all 1,165 payloads byte-exact. `DMA3D.exe` is a different
  build from the Italian one (1.3 MB of 1.66 MB differ), so addresses do not
  carry between them, but the formats they read are the same.

### Also done along the way

* `iso_range.py` was broken for Joliet discs (`list` printed `# 0 files`):
  the `.`/`..` test was ASCII-only and rejected every UCS-2 name. Fixed, and
  the tool now also reads a local ISO (a converted BIN/CUE) as well as a URL.
* `AGENTS.md` now says to **always build debug**, which immediately found a
  latent underflow in `src/picture.rs` (`data.len() - PALETTE_BYTES` on a
  short payload). `docs/PICTURES.md` §7.4 records the equivalence check.
* Sound output moved to **FLAC** (683 files, 32.6% smaller than raw PCM),
  written by the official `flac` tool; `docs/SOUNDS.md` §6.1 records why
  ffmpeg and the Rust `flac` crate were both rejected.

### Left for the next shift

* The `.mod` geometry: trace the reader in `DMA3D.exe` (the class is
  `MM3DModelMolecule` / `MM3DModDatabase`; the resource readers report
  `Bad checksum / version / header`) and emit OBJ or a similar open format.
* The texture dimensions and palette selector, so textures can become PNG.
* `DMA3D.MMF`'s film record layout.
