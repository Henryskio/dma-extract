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
| `Dati/HD.PIC` | `Pictures` | 1 | 1682 |
| `Dati/mpApp.PIC` | `Pictures` | 1 | 384 |
| `Dati/pack.PIC` | `Pictures` | 1 | 10,893 |
| `Dati/Topolino.CPK` | `Pictures` | 1 | 2 |
| `Dati/HD.SND` | `Sounds` | 1 | 66 |
| `Dati/mpApp.SND` | `Sounds` | 1 | 152 |
| `Dati/pack.SND` | `Sounds` | 1 | 234 |
| `Dati/pack.BLB` | **`Blobs`** | 1 | 305 |
| `Dati/HD.mms` | `Script` | 1 | 295 |
| `Dati/pack.mms` | `Script` | 1 | 988 |

The executable is `Dati/\*\*\*\*\*\* Magico Artista.exe`; the installer ships it
inside its own tree. Note the extensions differ from the later games
(`.PIC`/`.SND`/`.BLB` instead of `.MMP`/`.MMA`/`.MMF`) — the CLI identifies
containers by their `MMFW` signature, so no special handling is needed.

## 2. Container header — what version 1 changes

The signature, endian marker, version field, file id and timestamp are the
same as versions 2 and 3. Two things differ, and both are visible in any
version-1 file:

1. **The resource count is the u16 at `0x1E`, and it is `N`, not `N + 1`.**
   Versions 2 and 3 store `N` at `0x22`; in version 1 the `0x22` field holds
   something unrelated (`HD.PIC` reads 20288 there while its real count is
   1682).
2. **The offsets table starts at `0x20`, four bytes earlier than the `0x24`
   used by versions 2 and 3.** The two differences cancel in the count but
   not in the base, so reading v1's table from `0x24` shifts every resource
   by one and drops the first one — which is why `pack.BLB` used to yield
   304 rather than 305 resources with its first payload read one record early.

The table holds `N + 1` entries and its last entry is the file size. That is
what pins the base down: in all ten shipped containers
`u32@(0x20 + 4*N) == file length`, and no other base satisfies it.

```text
0x00  16 bytes  signature: "MMFW " + tag, NUL padded
0x10   2 bytes  endian marker ("MM")
0x12   2 bytes  u16 version (1)
0x14   6 bytes  file id
0x1A   8 bytes  timestamp; its high half is the u16 count at 0x1E
0x1E   2 bytes  u16 resource count N
0x20  4*(N+1)   u32 offsets; the last is the file size
        2 bytes padding
       32*N     resource names, NUL padded ASCII
       …        per-kind metadata region (below)
       …        payloads
```

### 2.1 How the count and base were pinned down

For every container, `u32@(0x20 + 4*k)` is followed by values that decode as
the ASCII of the first names only when `k == N`; scanning base `0x24` instead
puts the EOF marker at index `N - 1` and leaves one entry short. Both
observations agree with the count read from `0x1E`:

| container | `u16@0x1E` = N | EOF index from `0x20` |
|---|---:|---:|
| `HD.PIC` | 1682 | 1682 |
| `mpApp.PIC` | 384 | 384 |
| `pack.PIC` | 10893 | 10893 |
| `Topolino.CPK` | 2 | 2 |
| `HD.SND` | 66 | 66 |
| `mpApp.SND` | 152 | 152 |
| `pack.SND` | 234 | 234 |
| `pack.BLB` | 305 | 305 |
| `HD.mms` | 295 | 295 |
| `pack.mms` | 988 | 988 |

## 3. The per-resource metadata region

The version-1 metadata region is *smaller* than versions 2 and 3, but it is
not padding: the loader (`MMPictureResourceFile::Read`, the reader in
`DMA1.exe` that consumes the `00 07` marker) names every array. All three
kinds begin with a `4*N` u32 array; what follows differs by kind.

**Pictures** — `15*N + 4` bytes plus the palette bank:

```text
       4*N      u32 array (object offset 0x377): the resource handle word
        2 bytes marker, always 0x0007
        N       flags byte array   (object offset 0x380; bits 0-1 read)
        N       kind byte array    (object offset 0x384; bits 2-3 read)
       4*N      {u16 height, u16 width} dimension table   (0x388)
       4*N      u32 stored-size table, all zeros          (0x38c)
        N       byte array whose accessor is unknown      (0x390)
        2 bytes palette count
   count*1024    palette bank: 1024 bytes per palette
```

**Sounds** — `5*N + 2` bytes: the same `4*N` array, then one `N` byte array
and 2 bytes. The `4*N` array decodes as **Mac timestamps** (seconds since
1904-01-01): `0xaedb282b` is 1997-04-15, which matches the game's build date.
The `N` byte array is the format byte (`attr`).

**Blobs** — `4*N + 2` bytes: nothing but the handle array and 2 bytes.

The region must end exactly at `offsets[0]`, and it does in every container
(`HD.PIC`: names end at 60590, metadata + 0 palettes end at 85824 = `off[0]`;
`mpApp.PIC`: 13862 → 21674 with 2 palettes). The parser rejects a container
whose metadata does not land there, rather than guessing.

### 3.1 The dimension table

The dimension table is the piece that used to be missing. It is a `4*N` array
of `{u16 height, u16 width}` pairs — **the second u16 is the width**, the same
order as versions 2 and 3. The accessor at `0x417ab4` returns a pointer to the
pair and its caller stores the first u16 one way and the second u16 the
other, so a wrong pair cannot hide.

The measurements that confirm it:

* `Topolino.CPK` has one 95×95 picture. Its payload is 36100 bytes
  (`4 * 95 * 95 = 36100`), and the pair read at that index is `(95, 95)`.
* `HD.PIC` resource 0 (`Cbg___01.PICT`, 64384 bytes) reads `(480, 130)`; with
  `stride(130) = 132` the payload is `1024 + 132*480 = 64384`, a palette plus
  an 8bpp image.
* `HD.PIC` resources 1–5 read `(25, 482)`, `(84, 482)`, `(480, 28)`,
  `(84, 482)`, `(25, 482)` against payloads of 13124, 41680, 41680, 13124 and
  20224 bytes — every one is `1024 + stride(w)*h`.
* 1092 of `HD.PIC`'s 1682 payloads match a raw layout for their measured
  dimensions; before the table was read, none did.

## 4. Picture payloads

Pixel words are the same **32 bits per pixel, `[transparency, R, G, B]`** and
**8 bits per pixel** words as later versions. The storage layouts, decided by
payload length (which is ground truth), are:

| `kind` byte | layout | payload length | `HD.PIC` count |
|---|---|---|---:|
| `0x03` | 8bpp coverage mask, rows padded to 4 | `stride(w)*h` | 170 |
| `0x07` | 8bpp coverage mask, rows padded to 4 | `stride(w)*h` | 790 |
| `0x06` | 8bpp indices + inline 1024-byte palette | `1024+stride(w)*h` | 28 |
| `0x02` | 8bpp indices + inline 1024-byte palette | `1024+stride(w)*h` | 49 |
| `0x00` | 32bpp, rows padded to 4 | `4*stride(w)*h` | 28 |
| `0x08` | 32bpp, rows padded to 4 | `4*stride(w)*h` | 23 |
| `0x08` | 32bpp, flat | `4*w*h` | 40 |
| `0x14`, `0x15`, `0x16`, `0x17`, `0x18` | the two-section run-length format | compressed | 590 |

Two notes on how those rows were separated:

* The 8bpp layouts come in **padded and flat** forms. `HD.PIC` uses the padded
  form almost everywhere (960 resources against 2 flat ones), so the stride
  is `((w + 3) / 4) * 4`. The padded and flat layouts are only distinguishable
  when the width is not a multiple of four, which is why the flat one is tried
  first.
* The kinds `0x14`–`0x18` all set bit 4, and they are the compressed ones.
  Kinds below `0x10` are the raw pixel layouts.

### 4.1 Palette handling

Version 1 stores its palettes in a **bank at the end of the metadata
region**, immediately before the first payload, rather than prefixing them to
each payload the way versions 2 and 3 do. A u16 count is followed by `count`
1024-byte palettes:

| container | palettes in bank |
|---|---:|
| `HD.PIC` | 0 |
| `Topolino.CPK` | 0 |
| `mpApp.PIC` | 2 |
| `pack.PIC` | 1 |

The bank is what makes the `0x15`/`0x17` run-length resources decodable: 7,748
resources in `pack.PIC` and 35 in `mpApp.PIC` have **one-byte colour records
that are palette indices**, their table starting at offset 0 so no palette can
be prefixed or suffixed to the payload. Before the bank was plumbed through,
`picture::decode` had nowhere to look and reported all of them as failures.

`mpApp.PIC` ships two palettes and **nothing in the container says which entry
a picture indexes into** — there is no per-resource selector field. Entry 0 is
the one used, and that choice rests on a vision check rather than a measured
field: rendering `dmalog074`/`dmalog079` and `nameLess_230`/`nameLess_231`
through both entries and asking `qwen/qwen3.8-27b` which is the intended
rendering returns "FIRST" (entry 0) for two independent pictures. The two
entries differ in 254 of 256 slots including the low-numbered ones the
resources actually use, so the choice is real for those 35 resources and it
affects colour only — every entry in both palettes is opaque.

Resources with a palette prefixed to the payload (`kind 0x02` and `0x06`) do
not need the bank at all; it is consulted only as a fallback after every
in-payload placement has failed to line up.

### 4.2 The run-length sub-format

Kinds `0x14`–`0x18` reuse the **same two-section run-length format** already
documented in `PICTURES.md` §7.3 and used by the second and third games:

```text
u16[2h+1]     big-endian offset table; entries[0] == 2*(2h+1)
    entries[0..=h]    row i's run-length slice (sums to the width)
    entries[h..=2h]   row i's colour slice (one colour per foreground pixel)
```

No container reports a `kind` of 1 for these, so there is no palette prefix
and the colour records are four bytes `(T, R, G, B)` each. 590 of `HD.PIC`'s
resources decode this way; the acceptance test is that every row holds exactly
one colour record per foreground pixel.

**Still open:** kinds `0x15` and `0x17` in `mpApp.PIC` and `pack.PIC`
(`37merge*`, `dismag*`, `tooLong*`, `MdyLa*`) keep a payload that matches no
raw layout and does not carry the `2h+1` table. Their bytes begin with u16
values whose high byte is `0x00`/`0x01`/`0x02`/`0x03` and whose low byte
climbs by 3–5, e.g. `ff 03 7a 03 7d 03 80 03 85 ff 03 8a 03 8f 03 94 03 99`,
and end in 4-byte records with a constant first byte. `src/bin/v1pic` reports
them as failures.

## 5. Sounds — extracted, lossless

Sounds use the shared `SoundFormat` rules of the later games, but the format
byte now comes from a **measured** place: the `N` byte array that ends the
version-1 sound metadata region (§3). Before the metadata was decoded the byte
was unavailable and everything was assumed 8-bit; the byte says otherwise for
some containers.

* `HD.SND` — 66 resources, first `CPtBtS01.aif`. 64 of the 66 format bytes are
  `0x01`, which reads as 8-bit mono — so `HD.SND` is indeed 8-bit unsigned PCM
  and its payloads begin `80 80 80 80 …` with `0x80` as silence, exactly as the
  older notes said.
* `mpApp.SND` — 152 resources, first `CBt__S01.aif`. The bytes are spread over
  `0x21`, `0x31`, `0x35`, `0x01`, `0x04`, `0x05`, `0x25`, `0x26` and `0x00`,
  so this container mixes depths.
* `pack.SND` — 234 resources, first `itsMusic.aif`. Mostly `0x35` (16-bit
  mono) with a minority at `0x21` (8-bit mono). Its payloads begin
  `81 81 7f 80 …` — ordinary PCM ramps.

**Extracted: 451 WAVs out of 452 resources**, one warned:
`mpApp.SND` resource 121 `SifBtS10.aif` has 7817 bytes, an odd number, while
its format byte `0x25` sets the 16-bit flag. The extractor warns and counts
the failure rather than guessing which of the two is wrong. Every other
sound is byte-exact in `--original` mode and round-trips through WAV.

**Open:** the exact bit meaning of the version-1 format byte. Bits 0–2 match
the later games' `attr` (rate, 16-bit, channels), but bit 5 is set on most
sounds and bit 4 on some, and neither is used by the later `attr`.

## 6. Blobs — extracted verbatim, not decoded

`pack.BLB` (73,074,150 bytes, 305 entries) is an **opaque payload
container**, tag `Blobs`. The `Blobs` container kind writes each payload
verbatim in both modes, since nothing about the inner format is known yet.

**Extracted: 305 `.blb` files.**

The first payload starts `00 0c 67 6f 6f 66 79 20 32 2e 36 2e 33` — i.e.
`0x000c` then the ASCII string **`goofy 2.6.3`**, a codec/product version
stamp. The container is very likely the carrier for the game's **videos**
(the ones with sound), so decoding it is the highest-value next step.

**Open:** the blob payload format. `goofy 2.6.3` is a strong lead. The offset
stride is resolved (§2: the table starts at `0x20` and holds `N + 1`
entries), so all 305 resources are now addressed correctly.

## 7. Scripts — deliberately not decoded

`HD.mms` and `pack.mms` use the singular tag `Script`. Their object database
does **not** start right after the offsets table, so the game-3 `ScriptMeta`
parser rejects them and the extractor reports `missing script metadata`
rather than guessing.

The offset tables themselves are valid, so per-resource verbatim extraction is
straightforward once the layout is known: `HD.mms`'s first payload starts at
1622 and `pack.mms`'s at 4336.

**Open:** the v1 script object-database layout. The 404-byte region between
the names and the first `HD.mms` payload holds `00 00 00 64 05 7a 00 00 05 7b
00 00 …` — repeating 4-byte records whose second u16 climbs by one, which
looks like a per-object table of some kind but does not scroll to the
game-3 layout.

## 8. Results so far

```sh
./target/release/mmf-extract /PathAssets/Dati -o out --types pictures,sounds,blobs
```

| type | files | failures |
|---|---:|---:|
| sounds | 451 | 1 (`SifBtS10.aif`, see §5) |
| blobs | 305 | 0 |
| pictures | 9,840 | 3,121 |

`--original` output is byte-exact for **13,717 of 13,718** resources; the one
exception is the sound that cannot be decoded at all.

`HD.PIC` (1682) and `Topolino.CPK` (2) extract with **zero failures**;
`mpApp.PIC` reaches 274 of 384 and `pack.PIC` 7,882 of 10,893.
`src/bin/v1pic` reports what each resource is stored as, per resource:

```sh
./target/release/v1pic /PathAssets/Dati/HD.PIC
```

## 9. Summary of what is measured vs inferred

| claim | status |
|---|---|
| count at `0x1E` is `N` (not `N+1`) | **measured** on all ten containers |
| offsets table starts at `0x20`, not `0x24` | **measured** — `u32@(0x20+4N)` is the EOF marker |
| names are 32 bytes after the offsets table | **measured** |
| picture metadata is `15*N + 4` + palette bank | **measured**; it lands exactly on `off[0]` |
| picture dimensions are a `4*N` `{height, width}` table | **measured** — 1092/1682 `HD.PIC` lengths agree with them |
| `kind` bit 4 marks the compressed sub-formats | **measured** (kinds `0x14`–`0x18` vs `0x00`–`0x08`) |
| sound `4*N` array is Mac timestamps | **measured** — `0xaedb282b` decodes to 1997-04-15 |
| sound format byte is the `N` byte array | **measured** position; bit meaning **open** (§5) |
| payloads uncompressed | **measured** on pictures and sounds |
| picture pixel word is `[T,R,G,B]` | **measured** on `HD.PIC` resource 0 |
| kinds `0x15`/`0x17` payload format | **measured** for the 7,748 `pack.PIC`
  resources — RLE with 1-byte palette-index colour records; the remainder
  still fail |
| kinds `0x02`/`0x06` payload format | **unknown** — no table, no raw layout |
| which palette entry `mpApp.PIC` indexes into | **judgement**, not measured —
  entry 0, confirmed by `qwen/qwen3.8-27b` on two pictures (§4.1) |
| v1 script object-database layout | **unknown** — reported as failures |
| `goofy 2.6.3` identifies a video codec | **inferred** from the string alone |
