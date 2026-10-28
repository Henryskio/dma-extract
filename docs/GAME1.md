# \*\*\*\*\*\* Magico Artista 1 — MMFW version-1 format

Reverse-engineering notes for the **first** game in the series:
*\*\*\*\*\* Magico Artista 1* (Italy, v1.0 Win/Mac, 1997). It is the oldest
build and differs from the third game (`DMA4.exe`) and the second
(`DMA.exe`) in several structural ways, so it is documented separately from
`PICTURES.md` / `SOUNDS.md` rather than being folded into them.

Everything below is derived from the shipped files and the game's own
executable, and checked numerically.

> **Engine note.** Despite the `MMFW` container signature this is **not** a
> Clickteam Multimedia Fusion 2 application; the loader family is in-house
> (`MME`). `DMA.exe`-era builds of this family carry no Clickteam markers.

## 1. Where the files are

The game ships on a CD whose root holds a `Dati/` ("Data") folder. The
interesting members are:

| File | Tag | Version | N |
|---|---|---|---|
| `Dati/HD.PIC` | `Pictures` | 1 | 1681 |
| `Dati/mpApp.PIC` | `Pictures` | 1 | 383 |
| `Dati/pack.PIC` | `Pictures` | 1 | 10,892 |
| `Dati/Topolino.CPK` | `Pictures` | 1 | 1 |
| `Dati/HD.SND` | `Sounds` | 1 | 65 |
| `Dati/mpApp.SND` | `Sounds` | 1 | 151 |
| `Dati/pack.SND` | `Sounds` | 1 | 233 |
| `Dati/pack.BLB` | **`Blobs`** | 1 | 304 |
| `Dati/HD.mms` | `Script` | 1 | 294 |
| `Dati/pack.mms` | `Script` | 1 | 987 |

The executable is `Dati/\*\*\*\*\*\* Magico Artista.exe`; the installer ships it
inside its own tree. Note the extensions differ from the later games
(`.PIC`/`.SND`/`.BLB` instead of `.MMP`/`.MMA`/`.MMF`) — the CLI identifies
containers by their `MMFW` signature, so no special handling is needed.

## 2. Container header — what version 1 changes

The signature, endian marker, version field, file id, timestamp and the
u32 offsets table at `0x24` are the same as versions 2 and 3. Three things
differ:

1. **The resource count is the u16 at `0x1E`, and it stores `N + 1`.**
   Versions 2 and 3 store `N` at `0x22`; in version 1 the `0x22` field
   holds something unrelated (`HD.PIC` reads 20288 there while its real
   count is 1681).
2. **Each offset entry is `(u16 id, u32 offset)` in *some* containers and a
   plain u32 in others** — see §2.2.
3. **There is no metadata region at all.** No CRC array, no attr/flags
   bytes, no dimension table, no decoded-size table, no kind array. The
   bytes between the end of the names and the first payload are padding.

### 2.1 How the count was pinned down

The offsets table runs from `0x24` until an entry equal to the file size.
That entry index is exactly `u16@0x1E - 1`, in all nine containers:

| container | `u16@0x1E` | entries until EOF | `N` | names begin |
|---|---:|---:|---:|---|
| `HD.PIC` | 1682 | 1682 | 1681 | `Cbg___01.PICT` |
| `pack.BLB` | 305 | 305 | 304 | `SbfSs_01.txt` |
| `HD.SND` | 66 | 66 | 65 | `CPtBtS01.aif` |
| `mpApp.SND` | 152 | 152 | 151 | `CBt__S01.aif` |
| `mpApp.PIC` | 384 | 384 | 383 | `AboutBox` |
| `pack.PIC` | 10893 | 10893 | 10,892 | `Cbg___02.pict` |
| `Topolino.CPK` | 2 | 2 | 1 | `Topolino` |
| `HD.mms` | 295 | 295 | 294 | — |
| `pack.mms` | 988 | 988 | 987 | — |

Names are **32-byte** fields starting at `0x24 + 4*(N+1) + 2`, the same as
later versions.

### 2.2 The two offset-table shapes

Measured entry strides differ between containers:

* **u32, 4-byte stride** — `HD.PIC`, `HD.SND`, `mpApp.SND`, `mpApp.PIC`,
  `pack.PIC`, `Topolino.CPK`, `HD.mms`, `pack.mms`. First entries of
  `HD.PIC`: `0x24ac0, 0x27e04, 0x320d4, 0x35954, …`
* **`(u16 id, u32 offset)`, 6-byte stride** — `pack.BLB`. First entries:
  `(2, 0x24ac0), (2, 0x7e04), (3, 0x5954), (3, 0xfc24)`. The u16 is a
  small counter (2, 2, 3, 3, 4, 4 …), i.e. *two entries per id*.

The extractor currently parses version 1 with the plain **u32** stride,
which is why `pack.BLB` yields 304 rather than 305 resources and its first
payload is read one record early. See §6.

## 3. Payloads are uncompressed

There is no size table, so `Container::sizes` is all zeros and every
resource reads as "stored uncompressed". Confirmed by looking at the bytes:
no LZ77 control stream, no `0xff` sentinel, no length header.

## 4. Pictures — payload format known, geometry not

The pixel data is the same **32 bits per pixel, `[transparency, R, G, B]`**
word as later versions. `HD.PIC` resource 0 (`Cbg___01.PICT`, 13,124 bytes)
begins:

```text
00 ff ff ff  00 91 91 91  00 c3 66 39  00 ce 60 2b …
```

— transparent white, then a grey ramp, exactly the game-3 layout.

**But version 1 stores no dimension table**, so the decoder cannot know a
payload's `width`/`height`. The lengths do not factor into plausible
images by themselves (`smpbth01.pict` is 2714 bytes = 2 × 23 × 59, and
`(2714-2)/4 = 678`, not a rectangle), so the extractor reports these as
failures rather than guessing a geometry.

The remaining ~89,650-byte padding region between the names and the first
payload in `HD.PIC` is **not** a metadata table — it holds per-resource
hashes (u16 runs such as `30561, 29797, 29279, 12592 …`) plus at least one
32-byte name (`water_100_00_00`), and does not scroll to anything usable.

**Open:** where v1 stores picture dimensions. The likely home is a field
inside the hashed records, or the name string itself.

## 5. Sounds — extracted, lossless

Sounds parse with the shared `SoundFormat` byte rules inherited from later
games: the payload is unsigned 8-bit PCM (`0x80` = silence) or the
differential codec, and the loader's format byte is read from the same
object offset. In the shipped files:

* `HD.SND` — 65 resources, first `CPtBtS01.aif`
* `mpApp.SND` — 151 resources, first `CBt__S01.aif`
* `pack.SND` — 233 resources, first `itsMusic.aif`

`HD.SND` payloads are byte streams beginning `80 80 80 80 …`; `pack.SND`
begins `81 81 7f 80 …` — ordinary PCM ramps.

**Extracted: 449 WAVs** with zero failures, written by the same
`--original`/modern paths as the other games.

## 6. Blobs — extracted verbatim, not decoded

`pack.BLB` (73,074,150 bytes, 304 entries) is an **opaque payload
container**, tag `Blobs`. The `Blobs` container kind writes each payload
verbatim in both modes, since nothing about the inner format is known yet.

**Extracted: 304 `.blb` files.**

The first payload starts `00 0c 67 6f 6f 66 79 20 32 2e 36 2e 33` — i.e.
`0x000c` then the ASCII string **`goofy 2.6.3`**, a codec/product version
stamp. The container is very likely the carrier for the game's **videos**
(the ones with sound), so decoding it is the highest-value next step.

**Open:** the blob payload format. `goofy 2.6.3` is a strong lead; the
`(u16 id, u32 offset)` offset stride also needs handling before the blobs
can be addressed by index.

## 7. Scripts — deliberately not decoded

`HD.mms` and `pack.mms` use the singular tag `Script` and their object
database does **not** start right after the offsets table, so the
game-3 `ScriptMeta` parser rejects them. The extractor reports
`missing script metadata` rather than guessing.

The offset tables themselves are valid (`HD.mms` first entries
`0x656, 0x7ea, 0x97e, 0xb12 …`), so per-resource verbatim extraction is
straightforward whenever the payload layout is understood.

**Open:** the v1 script object-database layout.

## 8. Results so far

```sh
./target/release/mmf-extract /PathAssets/Dati -o out --types pictures,sounds,blobs
```

| type | files | failures |
|---|---:|---:|
| sounds | 449 | 0 |
| blobs | 304 | 0 |
| pictures | 1,091 (partial) | 12,935 (no dimensions) |

The 1,091 pictures that do extract are the ones whose geometry the
payload length happens to pin down unambiguously; the rest fail loudly.

## 9. Summary of what is measured vs inferred

| claim | status |
|---|---|
| count at `0x1E` is `N+1` | **measured** on all nine containers |
| names are 32 bytes after the offsets table | **measured** |
| no metadata region in v1 | **measured** (no size table, bytes are padding) |
| payloads uncompressed | **measured** on pictures and sounds |
| picture pixel word is `[T,R,G,B]` | **measured** on `HD.PIC` resource 0 |
| sound PCM interpretation | **measured** (rate/depth via the format byte) |
| `pack.BLB` offset stride is 6 bytes | **measured** |
| `goofy 2.6.3` identifies a video codec | **inferred** from the string alone |
| v1 picture dimensions | **unknown** — reported as failures |
