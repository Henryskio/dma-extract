# Disney Magico Artista 2 — extraction of the second game in the series

> **Series order.** The primary target of this repository is *Disney's Magic
> Artist Deluxe* (Italian release *Disney Magico Artista*, `DMA4.exe`, linked
> 2002) — that is the **third** game in the series. This page covers
> *Disney Magico Artista 2* (`DMA.exe`, linked 1999) — the **second** game,
> and therefore the *older* build, despite being lumped in with a
> "sequel".

*Disney Magico Artista 2* uses the **same in-house `MMFW` container family and
the same payload encodings** as the third game, so the existing extractor
works on it after two small format differences are handled. Nothing new had
to be built.

## 1. Source

The game is on archive.org as `disney-magico-artista-2` (item metadata:
`https://archive.org/metadata/disney-magico-artista-2`). The useful file is
`TOPOLINO2.ISO` (479,662,080 bytes, MD5 `0fbcca46cd301925c65f1fba4cdc8836`,
verified after download).

Only the *system area* of the image was needed to work out its layout, and
only the game's own files were extracted from it:

```sh
python3 iso_range.py list "https://archive.org/download/disney-magico-artista-2/TOPOLINO2.ISO"
python3 extract_iso.py copy /tmp/sequel/TOPOLINO2.ISO <dest> '^(DMA\.MMF|HD\.MMA|HD\.MMP|Hd\.mms|app\.MMA|app\.MMP|stmp1\.MMP|stmp2\.MMP|Sample|Sketch)$' '^Setup/DMA2/'
```

The disc's root holds `app.MMA`, `app.MMP`, `HD.MMA`, `HD.MMP`, `Hd.mms`,
`DMA.MMF`, `stmp1.MMP`, `stmp2.MMP` (no `stmp3`), plus two
extension-less 1,856,239-byte files named `Sample` and `Sketch` (both start
with `Disney Art Studi…`, not containers). The game executable is
`Setup/DMA2/HD/DMA.exe` (1,645,056 bytes) — the installer ships it inside its
own tree.

## 2. Engine identification (same family as the third game)

`DMA.exe` carries the same `MME_*` loader errors (`MME_PICTURE_RESOURCE_FILE_*`,
`MME_SOUND_RESOURCE_FILE_*`, `MME_FILM_RESOURCE_FILE_*`, …) and adds three new
resource classes: **movie** (`MME_MOVIE_RESOURCE_FILE_*`), **tune**
(`MME_TUNE_RESOURCE_FILE_*`) and **blob** (`MME_BLOB_RESOURCE_FILE_*`). There is
still no `Clickteam`/`CNC`/`MMF2` marker. Its UI strings are Italian
("*Disney Magico Artista 2*"), and it was linked 1999-09-16.

## 3. Format differences from the third game

| | third game (Deluxe, 2002) | second game (2000-ish) |
|---|---|---|
| container version (`0x12`) | 3 | **2** |
| picture metadata | `15*N+4` | identical (verified: 24,004 = 15·1600+4 etc.) |
| sound metadata | `14*N+2` | identical (verified: 3,320 = 14·237+2) |
| `HD.MMS` tag | `"MMFW Scripts"` | **`"MMFW 3 Script"`**, `II`, version 2 |
| film metadata marker | 7 | 5 |
| film per-record length | 26 bytes | 22 bytes |
| film inner chunks | full `0x0A`, delta `0x0B` | **full `0x00`, delta `0x01`** |
| film key frame | header + LZ77, rows padded to 4 | **raw indices, rows unpadded** |
| film delta | header + rects + LZ77 + RLE | **rects + RLE, no LZ77 wrapper** |

Each difference is handled in the parser (`src/container.rs`, `src/film.rs`).
The film record length is *solved* from the file layout (record length +
palette count such that the metadata ends exactly at the payload start), so
both 26- and 22-byte records are accepted.

### The 4 Galois-case picture bug

`Container::open` rejected the second game's files outright because it demanded
version 3. Allowing 2 was the only container-level change needed for
pictures and sounds.

## 4. Results

```text
./target/release/mmf-extract /tmp/sequel/files -o /tmp/sequel-extracted --types all

Extracted:
  pictures: 38447        (5 containers: app 1600, HD 4365, stmp1 15368, stmp2 18295)
  sounds:   891          (HD 237, app 654)
  films:    1            (Intro.flm: 193 frames, 141 unique, 800x600, 16,019 ticks)
  scripts:  0            (see open questions)
  failed:   1181 pictures  (the same undecoded "odd payload" sub-format as the
                           third game — see below)
```

Spot checks:

* WAVs open with the expected headers (e.g. 22,050 Hz mono 8-bit for
  `CPtBtS01`, 22,050 Hz 16-bit for `CreditsMusic`/`OpeningMusic`).
* `Intro.flm` frames decode to the exact 800×600 key frame; dimensions come
  from the 22-byte record's `top/left/bottom/right` u16s
  (0, 0, 600, 800).
* 137 of 147 delta chunks decode; the other 7 drive the cursor negative
  (see open questions) and their frames reuse the previous image.

### The 1,181 "odd" picture payloads

These are the same undecoded sub-format as the third game's 105 (an RLE-like
stream; payloads start with patterns such as `00 00 49 01 52 01 …` row-offset
tables or `ff 00 00 00 00 fe fe fe …` palette ramps). The extractor reports
them as failures rather than guessing, exactly as before. They are named
`ITstampareaHOTSPOT*`, `ttpmo_*`, `cudbtf*`, `TtuPt_*`, `ttutw_*`,
`ITXTcarrot02`, … — the hotspot/thumbnail/tutorial art.

## 5. Open questions

* The second game's script container (`Hd.mms`, `MMFW 3 Script`, `II`, version 2,
  declaring 61,201 resources) does not parse with the third game's object
  database layout; `off[0]` only works read as little-endian and the metadata
  block after the offset table is not the same shape. Documented here rather
  than guessed.
* 7 of 147 film delta chunks decode with the third game's cursor-relative RLE
  rule and then write at a negative frame offset, so the delta grammar must
  differ in a detail that is not yet identified.
* The new `MME_MOVIE_*`/`MME_TUNE_*`/`MME_BLOB_*` resource classes are not
  represented by any shipped container.
