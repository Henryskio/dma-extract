# \*\*\*\*\*\* Magico Artista 1 — MMFW version-1 format

Reverse-engineering notes for the **first** game in the series:
*\*\*\*\*\* Magico Artista 1* (Italy, v1.0 Win/Mac, 1997). It is the oldest
build and differs from the third game (`DMA4.exe`) and the second
(`DMA.exe`) in several structural ways, so it is documented separately from
`PICTURES.md` / `SOUNDS.md` rather than being folded into them. This file
states **only what version 1 changes**; anything the games share is written
down once in the shared document and referenced from here, the same rule
`src/v1/` follows against `src/`.

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

**Where the working copy lives.** `/PathAssets/Dati` (the ten containers, 265
MB) is carved from the public disc image
[`archive.org/download/disney-magico-artista/TOPOLINO.ISO`](https://archive.org/details/disney-magico-artista),
which is 574,132,224 bytes and serves HTTP range requests, so
`iso_range.py` carves from it without a download. That image is a *fuller*
dump than the one the earlier rounds of these notes used (548,222,976 bytes):
the ISO9660 tree ends at sector 147,066 and the remaining ~273 MB is the Mac
side of this Win/Mac hybrid disc, which no directory record references. The
Windows side — every file above — is byte-identical between the two, and the
counts in the table were re-measured against it. `pack.BLB` is 73,074,150 bytes
on both, which is the check that they are the same disc.

`iso_range.py list` prints `# 0 files` on these images: the disc carries a
Joliet volume descriptor and the tool's directory walker rejects UCS-2 names,
so the ISO9660 tree was read by hand. That hand-read table, ready for
`iso_range.py save <url> <out> <lba> <size>`, is:

| file in `Dati/` | LBA | bytes |
|---|---:|---:|
| `HD.PIC` | 3071 | 11,706,931 |
| `HD.SND` | 8788 | 1,626,231 |
| `HD.MMS` | 9583 | 75,889 |
| `TOPOLINO.CPK` | 10052 | 72,344 |
| `MPAPP.PIC` | 10090 | 7,958,866 |
| `PACK.BLB` | 29715 | 73,074,150 |
| `PACK.PIC` | 65396 | 50,029,743 |
| `PACK.SND` | 89825 | 99,991,486 |
| `PACK.MMS` | 138649 | 574,861 |
| `DISNEYMA.EXE` | 2455 | 1,217,536 |

**The executable.** Throughout this document the game's binary is called
`DMA1.exe`, which is its local name here. The file on the disc is
`Disney's Magic Artist.exe` (the installer also ships it under the Italian
title), carved from the table above as `DISNEYMA.EXE`. Every function address
cited below — `FUN_42e0d4`, `FUN_002eea`, the `0x4183xx` loader family — refers
to that 1,217,536-byte image at its preferred base of `0x400000`, with the
`CODE` section mapped from file offset `0x600`. It is a Win32 PE, i386.

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
       4*N      u32 uncompressed-size table               (0x38c)
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

`entries[0]` is both the table's own byte length and the offset the run data
starts at, and the last entry is the payload's length. The run bytes alternate
transparent/opaque, so a slice's *sum* is the width but only its odd-indexed
bytes are foreground pixels — `picture::foreground_pixels` sums those, and a
colour slice must hold exactly that many records.

The record width is either one palette index or four bytes `(T, R, G, B)`,
and `rle_rgba` tries both: `rec 1` needs a palette somewhere that does not
overlap the table (a prefix, a suffix, or the first game's metadata bank), and
`rec 4` needs none. 590 of `HD.PIC`'s resources decode this way, and the
compressed `kind 0x15` pictures of §4.3 decode as `rec 1` into the bank.

**The remaining failures were compressed, not a new layout.** The `4*N` array
at object offset `0x38c` — long read as an all-zero "stored size" table — is
the **uncompressed size** of each payload, and it separates the failures
perfectly. The table is the state of the art *before* the decompressor of
§4.3; the current state is that every resource decodes (§8).

| container | `size == 0` | `size == len` | `size > len` |
|---|---|---|---|
| `HD.PIC` | 1682 decoded, 0 failed | — | — |
| `Topolino.CPK` | 2 decoded, 0 failed | — | — |
| `mpApp.PIC` | 274 decoded, 0 failed | — | 0 decoded, 110 failed |
| `pack.PIC` | 7775 decoded, 0 failed | 107 decoded, 18 failed | 0 decoded, 2993 failed |

That is the measurement that found the compressed set: `size == 0` covers every
resource that decoded at the time, `size > len` accounts for 3,103 of the
3,121 failures, and `size == len` was the 18 `pack.PIC` resources that looked
stored but are not. Reading the column as "not compressed" is the trap, and
§4.3 closes both of them.

### 4.3 The compressed payloads — an LZSS stream

For a compressed resource the payload is an **LZSS** stream that expands to
`size` bytes, and that expanded stream is an ordinary picture payload: the
same `2h+1` big-endian offset table whose first entry is its own byte length
`2*(2h+1)`, the same alternating transparent/opaque run bytes, and one colour
record per foreground pixel. `pack.PIC` index 9062 (`mickey_j.PICT`, 222×202,
`kind 0x15`, stored 10280 bytes) is the worked example: it expands to exactly
the declared **23300** bytes, whose table's 445 entries run from 890 to 23300,
whose 222 run slices each sum to the width 202, and whose 222 colour slices
each hold exactly one **palette index** per foreground pixel — a `kind 0x15`
resource indexes the metadata palette bank (§4.1) rather than carrying four
bytes per pixel. That is not a single lucky resource: every one of the
12,961 resources now decodes, which is the same check applied to all of them,
and `tests/game1.rs` asserts it against the shipped files.

The discriminator is the declared uncompressed size of §3, and it is
exhaustive rather than a guess:

| `size` | meaning | how many |
|---|---|---|
| `size == 0` | stored as-is | 9,733, across all nine containers |
| `size > len` | an LZSS stream expanding to `size` bytes | `pack.PIC` 2,993, `mpApp.PIC` 110 |
| `size == len` | still a stream, expanding to `size == len` bytes | `pack.PIC` 125 |

The third row is what the earlier notes got wrong: a size equal to the payload
length looks like "stored", and the payloads *do* differ, so they are streams
whose expansion happens to be the same size. Of those 125, 107 decoded anyway
and 18 did not — reading the column as "not compressed" is what left those 18
failing after the other 3,103 were fixed. (The 3,103 is the `size > len` row,
which is where the bulk of the pre-LZSS failures sat.)

The routine is `0x42e0d4` in `DMA1.exe` (`FUN_42e0d4`), transliterated to
`src/v1/lzss.rs`. It is **not** the games-2-and-3 LZ77 (`FUN_004c557a` in
`DMA4.exe`): byte-oriented LZ77 with a match table, versus a 1024-slot ring
with fixed-width tokens here. The two are not interchangeable and share no
code.

**The stream**

```text
u8 control    one bit per token, least significant bit first, eight tokens
              per control byte. bit 1 = a literal, bit 0 = a match.
literal:      the next input byte, verbatim.
match:        the next two input bytes, big-endian:
                distance = word & 0x3FF        (10 bits)
                length   = (byte0 >> 2) + 3    (3..=66, from the other 6 bits)
```

Note *which* bits hold which field: `length = (byte0 >> 2) + 3` puts the
length in the word's **upper** six bits, so the encoded word is
`((length - 3) << 10) | distance`. Reading it as `((length - 3) << 2) |
distance` collides the length into the distance and silently invents matches.
Every token's output bytes are also written to `window[write_cursor & 0x3FF]`
and the cursor advances, so a match copies from a window it is still writing
into and overlapping runs are legal.

**The window.** 1024 bytes. `0x42e0f7` counts to `0x3be` (958) zeroing slots,
and `0x42e104` starts the write cursor at 958 — it zeroes exactly the slots
the cursor has not reached yet. The port zeroes the whole ring instead, which
is equivalent, because the game's window is the fixed global at `0x517540`
and would otherwise carry stale bytes between calls.

Two further consequences worth writing down, because both cost a round of
debugging when the first-game path was built:

* A match's `distance` is a **window slot index**, not a count of bytes back.
  With the cursor starting at 958, `n` bytes back is `(958 + p - n) mod 1024`
  in the encoded word. Emitting the reach-back directly makes a match read the
  zeroed prefix of the ring and produces zeros.
* The decoder stops as soon as it has produced the declared number of bytes;
  any remaining input is padding.

**Coded and tested.** `src/v1/lzss.rs` is the decompressor and
`src/v1/picture.rs` is the payload path that decides whether to call it.
`tests/game1_payloads.rs` carries an **independent** encoder written from the
token layout above, so the decompressor is checked against a second
implementation rather than against itself; writing that encoder is what caught
the bit-layout mistakes in this paragraph. That file also builds a synthetic
version-1 container, so the `--original` byte-exactness is pinned without the
game's CD being mounted.

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

**Extracted: 451 FLACs out of 452 resources**, one warned:
`mpApp.SND` resource 121 `SifBtS10.aif` has 7817 bytes, an odd number, while
its format byte `0x25` sets the 16-bit flag. The extractor warns and counts
the failure rather than guessing which of the two is wrong. Every other
sound is byte-exact in `--original` mode and round-trips through FLAC (the
modern-mode writer, with the 8-bit depth preserved, is described in
`docs/SOUNDS.md` §6).

**Open:** the exact bit meaning of the version-1 format byte. Bits 0–2 match
the later games' `attr` (rate, 16-bit, channels), but bit 5 is set on most
sounds and bit 4 on some, and neither is used by the later `attr`.

### 5.2 The bit depth comes from the payload, because the format byte is wrong for 24 of them

The version-1 sound metadata gives each resource one format byte, and its bit-2
selects the depth. That bit is wrong for **24 of the 451 resources** -- 21 in one
direction (the byte says 8-bit and the payload is 16-bit) and 3 the other way.

There is nothing structural to check the byte against, so the payload decides,
and `AGENTS.md`'s rule for metadata hints applies directly: when the hint
disagrees with the bytes, the bytes win. The override lives in
`implied_bits` in `src/sound.rs` and reports every resource it changes.

**The measurement is the fraction of samples sitting on the rails**, and it works
because both mis-readings put them there by construction:

* 16-bit data read as 8-bit takes every other byte, which is the **high** byte of
  each sample. A quiet signal's high bytes are `0x00`/`0xff`, so the result sits
  on the rails -- 16-51 % of samples.
* 8-bit data read as 16-bit pairs adjacent bytes into samples. 8-bit audio
  clusters around its centre `0x80`, so the pairs cluster around `0x8080`, near
  the negative rail -- 27-37 % of samples.

The correct reading puts 0.00-0.05 % of samples on the rails: a margin of two
orders of magnitude, and a threshold that needs no model of what the audio
should be. That is what makes it usable in an extractor, where a content model
would be a guess.

**How the 24 were confirmed.** A listener auditioned both readings of seven
resources and called the declared one "ear rape" in every case; the same seven
were then re-checked with [earshot](https://github.com/Leonxlnx/earshot) (CLAP
text-audio embeddings, run locally on CPU), which agrees on all seven. The other
17 were found by the rule alone and checked the same way. What earshot hears --
these are per-file descriptions, not a claim that any group is one thing:

| resource | earshot's reading of the corrected file |
|---|---|
| `pack.SND` `MdyIn001.AIF` | Speech 0.87; a voice-over narration (z +4.6) |
| `pack.SND` `MdyIn002.AIF` | Speech 0.83; a voice-over narration (z +4.1) |
| `mpApp.SND` `IahUh066.AIF` | Speech 0.89; a voice-over narration (z +4.2) |
| `mpApp.SND` `IahUh069.AIFF` | Speech 0.82; a voice-over narration (z +4.9) |
| `mpApp.SND` `Smp__S03.aif` | Wind chime 0.89, Chime 0.83; a bar-chime glissando |
| `pack.SND` `Mmk_S01.aif` | Speech 0.45, Plop 0.44; a squelch, digital silence between sounds |
| `pack.SND` `Mpl__S01.aif` | Animal 0.68, Pant 0.56, Dog 0.52; a record scratch |
| `pack.SND` `CreditsMusic.aif` | Music 0.82; cinematic orchestral music, 119.8 s |
| `mpApp.SND` `UVp__M01.aif` | Music 0.72; cheerful ukulele music, 31.0 s |
| `mpApp.SND` `UVp__M02.aif` | Music 0.92; funk music, 29.7 s |
| `pack.SND` `CtaTlS01.aif` | a drawer closing (z +4.5) |
| `pack.SND` `CtaTsS01.aif` | a drawer closing (z +3.8) |
| `mpApp.SND` `CUdBtS01.aif` | a police siren (z +4.1) |
| `mpApp.SND` `ZOOMRECT.AIF` | a record scratch (z +3.1) |
| `HD.SND` `CQtBtS01.aif` | a zipper (z +2.8) |

Every one of the 24 corrected files is a coherent named sound. Against those, the
declared readings are all "no match" with noise descriptions: `MdyIn001`
declared reads as *a power drill (z +3.7)*, `IahUh069` declared as *a phone
vibration buzz*, `Mpl__S01` declared as *a record scratch (z +3.6)* at 27 % of
samples on the rail.

Note that the corrected files are **not all narration**, and were not assumed to
be: three are music, three are foley or UI, and the four speech ones are the
Daisy and hesitation tracks. The evidence is about which depth is right, not
about what the sounds say.

`tests/sound_order.rs` pins it: the 24 flipped resources must decode at the
payload's depth (and must have read the other way in the format byte, so the case
proves something), six correctly declared resources must keep theirs, and every
flipped resource's decoded stream must come off the rails. `--original` is
unaffected throughout: it writes `container.payload`, never a depth.

### 5.1 The 16-bit samples are big-endian, and the shared decoder was wrong

This is a **first-game delta**: the versions-2/3 sound containers hold only
stored 8-bit PCM and 16-bit DPCM output, neither of which has a byte order to
get wrong, so `src/sound.rs` copied stored 16-bit PCM through on the stated
assumption that it was "already little-endian". That assumption is false for
the first game's 228 stored-16-bit resources.

The measurement is a property of the bytes, not of a decoder: a quiet 16-bit
signal puts almost all of its energy in its low byte, so whichever byte
position is nearly always `00`/`ff` is the high one. Across the three
containers, 212 resources carry a decisive signal and **212 of them put the
high byte at the even offset**; one reads the other way and is too short to
call, and 209 are too short to decide at all. So the stored 16-bit order is
big-endian, which is what an AIFF-authored asset from this Win/Mac hybrid disc
would be (`MddL3015.AIF` opens `ff e2 ff ce ff c2`: -30, -50, -62 big-endian,
or -7454, -12530, -15614 the other way — only the first three are plausible
audio).

Why it survived: reading the wrong way is not a loud failure. It multiplies
every sample by 256, which turns a quiet narration into a quiet hiss — so
every resource still decodes to a whole number of samples, `--original` still
writes the payload byte-exactly, and no test that only counts bytes can see it.
It only shows up in the decoded audio, and it showed up as the first game's
lesson tracks measuring **0 % silence** at -45 dB across all 192 of them,
against 19 % for the tool sounds as a control. After the swap the same files
measure 20 % silence with pitch strength 0.45 (against 0.12 before) and 42 %
of their energy in the 300–3400 Hz speech band — which is what narration
looks like.

`swap16` in `src/sound.rs` does the swap on the way into the modern output.
The archival path is untouched: `--original` writes `container.payload`, not
the swapped stream. `tests/sound_order.rs` pins it per resource and across
all 228.

**Where the lesson audio lives.** `docs/GAME1.md` §6 records that the `Blobs`
payloads carry no audio — every object the animation script can name is a
drawing shape. The lesson audio is instead a family of sound resources named
`M?d/M?f/M?l/M?n` plus a type token: `Mdd` `Mdy` `mmk` `Mpl` `Mgf` `Mmn` give
six characters (Donald, Daisy, Mickey, Pluto, Goofy, Minnie) and the tokens
`L` (lesson), `In` (intro) and `l` (a lowercase-typed duplicate family) give
115 + 28 + 23 = 166 files, 30.5 minutes in total. That is a claim about the
**name shape and the container structure only**: what the files contain varies,
and earshot hears speech in some and music or foley in others (§5.2 lists
examples), so the family is not "the narration". The game associates them with
its animations **by name at run time**, never by a hardcoded pairing: all 46
name lookups in the executable are literal strings, and they are UI/tool
sounds (`slider sound`, `tool looping sound`, `color picker drag channel`,
`lesson abort`) plus `session done sound`, which is the one the blob session
itself loads from the constructor at `FUN_004a7764` — a chime for the end of a
session, not narration. No sound name or blob name appears as a literal
anywhere in the image, so the pairing is not recoverable from the code and has
to come from the assets.

## 6. Blobs — the `goofy 2.6.3` animation records, decoded

`pack.BLB` (73,074,150 bytes, 305 entries) is an **animation script
container**, tag `Blobs`. Each payload drives a set of game objects along a
path: a header naming the objects, then one record stream per object. The
format is decoded in `src/v1/blobs.rs` and pinned by `tests/game1_blobs.rs`,
which asserts a 1:1 round trip over all 305 payloads.

**Extracted: 305 `.blb` files** byte-exactly under `--original`, and 305 JSON
payloads plus a manifest in the modern mode.

**Not video.** The container is very likely *misleadingly* large for what it
is. There is no `RIFF`/`AVI`/`ftyp`/`moov`/`mdat` magic anywhere in it, the
stamp never recurs inside a payload, and the bodies are coded **coordinates**
— 8-byte records of a coordinate pair and an event code. `goofy 2.6.3` is the
version stamp of whatever *wrote* those paths, not a video codec (§6.1).
Whether any of these paths are drawn to the screen as the game's animated
sequences is a question about the classes the object kinds select (§6.3), not
about the bytes.

The offset stride is resolved (§2: the table starts at `0x20` and holds
`N + 1` entries), so all 305 resources are addressed correctly.

### 6.1 The payload format

Every payload of the recovered `pack.BLB` (305 of them, 73,061,910 bytes) is
one `goofy 2.6.3` animation script. The layout below is **decoded** and
asserted in `tests/game1_blobs.rs`, including a 1:1 round trip: decoding a
payload and re-encoding it reproduces the container's bytes, exactly.

```text
  0        u16  0x000c                     the stamp's length prefix
  2        "goofy 2.6.3\0"   (12 bytes)    the writer's version stamp
 14        u32  object_count
 18        u32  object_bytes  == object_count * 12
 22        object_count x 12-byte objects:
              u32 kind        6..=0x32     selects the game-object class
              u32 parameter                 handed to that class by address
              u32 records                   how many records follow this object
 22+ob     u32  record_bytes   == the bytes that remain after this field
 26+ob     record_bytes of 8-byte records, in object order, each object's run
           closed by a record whose event is 0

record (8 bytes):  u16 a;  u16 b;  u32 event
```

Totals over the container: **209,751 objects** and **8,817,121 records**.

### 6.2 How each part was measured

Nothing here is inferred from the bytes alone; each relation is what the game's
own parser does, read out of `DISNEYMA.EXE` (the first game's executable — §1
has the carve table). The chain, with the vtable slot numbers for the session
class the `"session done sound"` constructor at `0x4a7763` builds (vtable
`0x51043c`):

```text
  header parser     0x4b5c6b   reads the stamp and the two header u32s, reads
                                object_bytes into an allocated section, then
                                advances its cursor by 14 + 4 + 4 + object_bytes
                                + 4 -> the records begin at 26 + object_bytes
  session step      0x4a7ae0   vtable slot 9; runs the frame loop
    -> advance      0x4a7d4c   turns the returned code into one of three
                               virtual calls on the object at this + 0x60
         -> next    0x4a7e98   pulls the next 12-byte object
              ->   0x4b5ec0    the object iterator: stores the object's THIRD
                               u32 as the stream's record count (object + 0x16)
                               and resets the record index (object + 0x1a) to 0
              ->   0x4a7ee0    a switch on the object's FIRST u32, cases 6..0x32,
                               each constructing one of the game's object classes
              ->   0x42e600    a lookup on the object's index (out + 0xc)
         -> read    0x4b5f34   the 8-byte record reader
              ->   0x4a7bc4    scales the two u16s through a 16.16 multiply and
                               adds a stage origin
```

**The stamp is checked, and it gates the version.** `0x4b5c6b` reads the
`u16` length and the string, then compares them against the string at
`0x512aac` (`goofy 2.6.3`); **only on a match** does it set the stream's
version field (`object + 0xa`) to `0x107`. That is the only writer of that
field, which is why the `-800` below applies to every first-game blob payload.
(A note on what the stamp is *not*: an earlier version of this document claimed
nothing validates it. That was wrong — the compare is at `0x4b5d16`, inside the
parser. The string's single *other* reference, from a data table, is unrelated.)

**The two header counts are the whole section boundary.** `object_count` lands
at `object + 0xe` and `object_bytes` at `object + 0x1e`, and the parser reads
exactly `object_bytes` bytes of objects before skipping a further `u32`. In all
305 payloads `object_bytes == object_count * 12`, so the records start at
`22 + object_bytes + 4`.

**The per-object record count is the object's third `u32`.** The iterator
`0x4b5ec0` reads one 12-byte object at a time from the section and does
`stream->record_count = out[8]; stream->record_index = 0`. The record reader
increments `record_index` and returns `0` for an object's **first** record,
`1` for a middle one and `2` for its **last**, which is how the game knows an
object's list is finished. Two independent checks confirm the partition on the
shipped data: the objects' counts sum to the stream's record count in all 305
payloads, and **all 209,751 objects end their run on an `event == 0`
terminator**.

**The record is 8 bytes.** `0x4b5f34` advances its cursor by 2, 2 and 4, and
swaps all three in place when the flag at `object + 0x22` says the file was not
written in the host's order — so one code path serves both and the wire order
is always big-endian. It then adds `0xfce0` to both `u16`s, which is `-800`, a
stage offset in the game's own coordinate space and not a byte-order fix.

**The object's first `u32` is a class selector.** The switch at `0x4a7ee0`
handles `6` to `0x32` (6..=50), constructing a different object class per
group: 6–15 and 26–30 take the session's resource field, 16–20 and 41–45 share
one vtable, 21–25, 31–35 and 36–40 each get their own, and 46–50 carry a
`0x2f0000` initial value. Every one of the 209,751 objects in the container
falls in that range. The object's second `u32` is stored at `session + 0x54`
and passed *by address* to the constructor `0x47b140`, which keeps the pointer
at `this + 0xc8`; consecutive objects of the same kind are not reconstructed
(`0x4a7ee0` returns early when the kind equals the one at `session + 0x5c`),
which is what the data shows as runs of identical kinds.

**What the coordinates are not.** `0x4a7bc4` scales each `u16` through
`FUN_004a7284`, a 16.16 fixed-point multiply (`(x * y) >> 16`), and adds a
stage origin read from the object at `session + 0x3b` (fields at `+1` and `+3`)
less the globals at `0x50cb78`/`0x50cb7a`. So the stored `u16`s are not raw
pixels, and the JSON output carries both the stored value and the `-800`
normalisation rather than choosing one.

**Not a coded-video stream.** No `RIFF`, `AVI `, `ftyp`, `moov`, `mdat`,
`MACE`, `OggS` or `PK` magic occurs in any of the 305 payloads, and the stamp
never recurs inside one — so each payload is one stream, not a concatenation.
The bodies draw from `0x00`–`0x1f` almost exclusively, ~5.0 bits/byte, but
they are coded *coordinates*: the 8-byte records that carry them are coordinate
pairs and an event code. A stray `1f 8b` in 117 of 305 payloads is a
coincidence of that distribution and is not gzip.

### 6.3 What is still open

* **What the object's second `u32` means.** It is measured as a parameter the
  constructed object holds by pointer (`this + 0xc8`), but the code that
  dereferences it has not been read. On the shipped data it behaves like a
  16.16 fixed-point value that is nearly constant within a payload and moves
  with the object kind — `0xdb999999` (≈56249.6) for `SbfSs_01`,
  `0x28909091` (≈10384.6) for `SbfSs_03` — but that is a description of the
  data, not a reading of the code.
* **What the 20 event codes do.** They are 0..19 with 0 closing each object's
  run; the three-way dispatch at `0x4a7d4c` (slots `+0x1c`, `+0x20`, `+0x24` of
  the object at `this + 0x60`) is where they are consumed, and the event-to-class
  table at `0x4a805a` and below is where the inline constructors live. Reading
  those blocks is what would turn a record stream into a render.

### 6.4 Approaches already tried and rejected

Recorded so they are not retried. Both were attempted before the parser at
`0x4b5c6b` was found, and both fail for the same reason: nothing about their
result is checkable against the game's own reader.

* **Reading the stream back from the terminator and scoring candidate starts by
  the share of small opcodes.** It locates a start for all 305 payloads and
  covers 99.5 % of the container's bytes, but it is **wrong**: on `SbfSs_01` it
  starts the stream at 1230, which is in the middle of the object section, and
  the run of small opcodes there is an artefact of the section's regular
  spacing. The true start is 9086 (`22 + 9060 + 4`).
* **Assuming the section closes at `14 + 8 + count × 12` or `22 + count × 12`.**
  Neither lands on a whole number of 8-byte records — for `SbfSs_01`,
  `22 + 9060 = 9082` leaves a half record. The missing four bytes are the
  `u32 record_bytes` tag the parser skips at `0x4b5df4`.

**The archival output.** `--original` writes all 305 payloads byte-exactly —
73,061,910 bytes across 305 files, zero mismatches — and needs no record size
at all. The modern mode writes one JSON payload per resource (stored and
stage values, object kinds and record counts) plus a container manifest,
sufficient to rebuild each payload byte for byte.

## 7. Scripts — the container opens, the object database does not

`HD.mms` and `pack.mms` use the singular tag `Script`. They carry **no names
table and no metadata region** at all, which is why the versions-2/3
`ScriptMeta` parser rejected them outright and the extractor used to report
`missing script metadata` for the whole container. What they have instead:

```text
0x1E  u16  resource count N            (N = 295 and 988)
0x20  4*(N+1)  u32 offsets; resource i spans [off[i], off[i+1])
        2 bytes  u16, measured as 4 in both containers
        …       payloads, concatenated, starting at offsets[0]
```

Two independent checks pin this down on both containers, and both are asserted
in `tests/game1.rs`: the first payload starts exactly at
`0x20 + 4*(N+1) + 2`, and the last offset equals the file length — `HD.mms`
1218 / 75,889 and `pack.mms` 3990 / 574,861. The `u16` is called a version
rather than padding because it is a constant 4 across both files; a container
reading anything else is rejected, because treating it as padding would be a
guess.

**Extracted: 1,283 objects, byte-exact in `--original` mode.** The payload is
never compressed here, so the stored bytes *are* the payload, and the
byte-exactness test asserts each one against the container's own bytes.

**What is measured about a payload.** Each one opens with a `u32` that reads as
an element count, and ends with a NUL-terminated ASCII **object name**:
`char606`, `char1a`, `rdy53`, `sky333`. The count's distribution is strongly
peaked — `pack.mms` has 210 payloads whose lead is 14 and all 210 are exactly
152 bytes, and 46 whose lead is 51 and all 46 are exactly 521 — so it is a
count of something per-object rather than a class id. 126 of 295 `HD.mms`
payloads and 664 of 988 `pack.mms` payloads carry a trailing name; the rest
carry none, which is the same behaviour `docs/SCRIPTS.md` records for the
later games ("some classes append string data after that fixed prefix" — some
classes append none). Those names are what `src/v1/scripts` uses for
filenames, and the module documents them as an inference for exactly that
reason.

**Open:** the per-class record size between the count and the name. Reading
the region as `count` records of a single fixed length fits almost nowhere —
120 of 295 `HD.mms` payloads fit a 10-byte record, 496 of 988 `pack.mms`
payloads do, and the rest fit 4, 12, 16 or several other widths, one width
sometimes fitting only a handful of payloads. That is the signature of a
per-class `type_aux` table like the one the shared format has, and finding the
per-class widths is the step that turns this into a structured extraction.
The `--original` and `objects/*.txt` outputs report the count and the payload
length per object so the mapping can be built from them.

## 8. Results so far

```sh
./target/release/mmf-extract /PathAssets/Dati -o out --types pictures,sounds,blobs
```

| type | files | failures |
|---|---:|---:|
| sounds | 451 | 1 (`SifBtS10.aif`, see §5) |
| blobs | 305 | **0** (§6; decoded, and `--original` byte-exact) |
| pictures | **12,961** | **0** |
| scripts (objects) | **1,283** | **0** (§7; `--original` only) |

Every first-game picture now decodes. The split that got it there:

| container | resources | stored plain (`size == 0`) | LZSS (`size > 0`) |
|---|---:|---:|---:|
| `HD.PIC` | 1,682 | 1,682 | 0 |
| `mpApp.PIC` | 384 | 274 | 110 |
| `pack.PIC` | 10,893 | 7,775 | 3,118 |
| `Topolino.CPK` | 2 | 2 | 0 |
| **total** | **12,961** | **9,733** | **3,228** |

`--original` output is byte-exact for **13,717 of 13,718** resources; the one
exception is the sound that cannot be decoded at all. For a picture that means
one of two things, depending on the storage form: a stored-plain resource is
the container's own slice, and a compressed one is the LZSS expansion to its
declared size. `tests/game1.rs` asserts both against the container's bytes.

`src/bin/v1pic` reports what each resource is stored as, per resource, and now
prints both the stored length and the expanded one, because the gap between
them is what says a payload was compressed:

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
| picture dimensions are a `4*N` `{height, width}` table | **measured** — no payload length lines up without them, or with the halves swapped |
| `kind` bit 4 marks the compressed sub-formats | **measured** (kinds `0x14`–`0x18` vs `0x00`–`0x08`) |
| sound `4*N` array is Mac timestamps | **measured** — `0xaedb282b` decodes to 1997-04-15 |
| sound format byte is the `N` byte array | **measured** position; bit meaning **open** (§5) |
| payloads are uncompressed | **disproved** — 3,228 of the 12,961 pictures
  are LZSS streams; `size == 0` is the stored-plain case (§4.3) |
| picture pixel word is `[T,R,G,B]` | **measured** on `HD.PIC` resource 0 |
| kinds `0x15`/`0x17` payload format | **measured** — RLE with 1-byte
  palette-index colour records; with the expansion step in place the remainder
  decode too (§4.2) |
| kinds `0x02`/`0x06` payload format | **measured** that these are the
  *compressed* payloads of §4.3, not a distinct layout |
| LZSS token layout | **measured** from `FUN_42e0d4` (`DMA1.exe`) and
  confirmed against every payload: 12,961 of 12,961 decode (§4.3) |
| compressed payloads expand to an ordinary picture payload | **measured** on
  all 3,228 — see §4.3 for `mickey_j.PICT`'s per-row checks |
| a match's `distance` is a window slot, not bytes back | **measured** — the
  cursor starts at 958, so the reach-back has to be rotated (§4.3) |
| `size == len` still means compressed | **measured** — 18 `pack.PIC`
  resources have a size equal to their stored length and differ from it |
| the `0xff`-literal-block prefix | **explained** — a control byte of `0xff`
  is eight literal tokens in a row, so an incompressible run opens with
  `0xff` + 8 bytes (§4.3) |
| which palette entry `mpApp.PIC` indexes into | **judgement**, not measured —
  entry 0, confirmed by `qwen/qwen3.8-27b` on two pictures (§4.1) |
| v1 script container layout (no names table, no metadata region) | **measured** — first payload at `0x20+4*(N+1)+2` and the marker `4`, both asserted in `tests/game1.rs` (§7) |
| v1 script object names come from the payload tail | **measured** — `char606`, `char1a`, `sky333`; an inference as a *rule*, in 126/295 and 664/988 payloads (§7) |
| v1 script per-class record size | **unknown** — no single width fits; this is the open question (§7) |
| a blob record is 8 bytes, `{u16 a; u16 b; u32 event}`, big-endian | **measured** — `FUN_004b5f34`'s cursor advances 2+2+4, and it closes exactly (§6.1) |
| a blob payload is stamp + objects + records | **measured** — the header parser `FUN_004b5c6b` advances `14 + 4 + 4 + object_bytes + 4`, so records begin at `22 + object_bytes + 4` (§6.2) |
| an object header is 12 bytes, `{u32 kind; u32 parameter; u32 records}` | **measured** — `object_bytes == count * 12` in 305/305, and the iterator `FUN_004b5ec0` stores the third field as the record count (§6.2) |
| each object's records are closed by an `event == 0` terminator | **measured** — 209,751 of 209,751 objects, and the reader `FUN_004b5f34` returns 0/1/2 for first/middle/last (§6.2) |
| the object's first `u32` is a class selector | **measured** — the switch at `FUN_004a7ee0` handles 6..=0x32 and every object in the container is inside it (§6.2) |
| the record reader adds `-800` to both coordinates | **measured** — `add word [esi], 0xfce0` at `0x4b5fc7`, gated on the version `0x107` that the stamp sets (§6.2) |
| the blob prologue is 38 bytes for every payload | **measured** on `session 1/2/3`; **disproved** as a general rule — the object section is 12-byte units, and its *length* is what varies (§6.2) |
| the `goofy` stamp is validated by the loader | **measured** — `FUN_004b5c6b` compares it at `0x4b5d16` and only then sets the version (an earlier revision of this table said *disproved*; that was wrong) (§6.2) |
| `pack.BLB` holds an RSS/AVI/QuickTime container | **disproved** — no such magic occurs in any of the 305 payloads, and the stamp never recurs inside one (§6.1) |
| the blob body is coded video | **disproved** — it is coded *coordinates*: the records that carry it are coordinate pairs and an event code (§6.1) |
| the object's second `u32` | **unidentified** — a parameter the constructed object holds by pointer (`this + 0xc8`); on the shipped data it behaves like a 16.16 value nearly constant within a payload, but that is a description, not a reading (§6.3) |
| what the 20 event codes do | **unidentified** — 0..19, with 0 closing each run; the dispatch is at `0x4a7d4c` and the per-event constructors are at `0x4a805a` and below (§6.3) |
| blob payloads re-encode byte-exactly from the decoded form | **measured** — `tests/game1_blobs.rs` asserts it over all 305 payloads (§6.1) |
