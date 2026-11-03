# Corrections — where the Lean port differs from the Rust implementation or the docs

This is the record the Lean port keeps of itself: every place the port had to
choose between the Rust code, the Rust docs and the `docs/` chain, and what it
chose and why. It is written so that a reader can tell the difference between a
format fact and a stale sentence.

The rule the port follows: **the measurement wins, then the code, then the prose.**
Where the code and the prose disagree, the code is the closer witness; where the
code disagrees with a measurement (a shipped container, a test's golden number,
the disassembled loader), the measurement wins and the code gets corrected.

§0 is the binary audit, which is the only section whose authority is the game's
own instructions. §1–§7 are the places this port already differs from the Rust
code or the docs. §8 is what those differences prove.

---

## 0. The binary audit — `BINARY-AUDIT.md`

A subagent re-derived five format rules straight from the game executables with
REA/rizin rather than from the Rust source or the docs: the sound format byte's
masks, the 256-entry DPCM table at `0x532778`/file offset `0x132778` in
`DMA4.exe`, the LZ77 control-byte dispatch at `0x4c562d`, the film metadata
loader at `DMA3D.exe`'s `0x477940`, and the two 3D metadata region lengths.

Its findings are in `lean/BINARY-AUDIT.md`, one section per claim with the
instruction addresses that decided it. Where a verdict is **WRONG**, the
corrected rule is implemented in Lean and the correction is recorded below; the
corrections in §9 are the ones that came back from it.

---

## 1. The audio encoder is `flac`, not `ffmpeg`

`Cargo.toml` said the extractor "shells out to `ffmpeg`". It has not for a long
time: `src/sound.rs` shells out to the official `flac` tool, and
`docs/SOUNDS.md` §6.1 records the measurement that decided it — ffmpeg's FLAC
encoder accepts `s16`/`s32` only, so an 8-bit source is silently promoted to
16-bit and the samples round-trip while the file lies about its depth. The
Lean port shells out to the same tool through `IO.Process`
(`MmfExtract/Sound.lean`), and `Cargo.toml`'s comment now says so.

**Corrected in:** `Cargo.toml`; nothing in Lean needed changing.

## 2. `--original` for the first game writes the **decoded** stream

`docs/GAME1.md` §5.1 and `src/sound.rs`'s own comment both say `--original`
writes `container.payload`. The code — and `tests/sounds.rs` — write
**`decoded.pcm`**: for a version-1 stored-16-bit resource that is the
byte-swapped stream, because a big-endian writer stored it
(`GAME1.md` §5.1's own measurement: a quiet 16-bit signal puts almost all of its
energy in its low byte, so whichever byte position is nearly always `00`/`ff`
is the high one).

The Lean port does what the code and the test do: `Extract.extractPictures`'s
original path calls `V1.Picture.payload`, which expands the LZSS stream, and the
sound path writes `decoded.pcm`.

## 3. `textures_meta_len` is `6*N + 2`

`docs/DMA3D.md` §2.1 records the `Textures` metadata region as
`32*N + 4*N + 2*N + 2` — 26,792 bytes for the shipped container — while the
code and `tests/dma3d.rs::the_metadata_regions_are_measured` measure the region
*between the names table and the first payload*, which is `6*N + 2` = 4,232. The
doc's own note ("measured **wrong** at first, as `6*N + 2`") refers to an earlier
state of that sentence; the function and the test are the live measurement.

The Lean parser uses `6 * n + 2` (`MmfExtract/Container.lean`, the same place the
Rust one does), so the region a container declares and the payload boundary
agree.

## 4. The glTF writer does **not** subtract one from indices

`src/dma3d/gltf.rs`'s module doc says the payload's indices are one-based and
"so the writer subtracts one". The code does not: the comment above the buffer
loop records that the payload's indices are already zero-based, and
`tests/dma3d.rs` asserts `back.indices == mesh.indices` for all 144 meshes.

That path is not ported yet (see `lean/README.md`), so nothing had to be chosen;
the rule is recorded here so the port does not inherit the module doc's mistake.

## 5. `StringTable` is classified on the tail length alone

`docs/SCRIPTS.md` §4.3 describes the `(u16 id, u32 offset, u32 reserved)` entries,
but `src/script.rs::classify` decides a `StringTable` from the tail alone — so a
payload can be classified `string-table` and then print
`# could not decode StringTable`. That is intentional (a container that
mis-describes an object still gets a dump), `HD.MMS` contains none, and the Lean
port keeps the same classifier, so the corpus it writes is the one the Rust
extractor writes.

## 6. The film metadata solve takes the **first** candidate, not the largest

`src/film.rs`'s `MAX_RECORD_LEN` doc says 26 is "the longest per-film record
seen", and the solve steps `16..=26` in twos, stopping at the first table/palette
count pair that ends exactly at the payload start. The second game's 22-byte
table is therefore found before a 26-byte one and used.

The Lean port iterates the same list and returns the first match
(`MmfExtract/Film.lean`, `parseFilmMetadata`).

## 7. The film metadata region is big-endian by construction

`parse_film_metadata` never reads `container.big_endian`: every read is an
explicit `u16::from_be_bytes` or `read_u16_checked` (which is big-endian), and
`docs/FILMS.md` §2 agrees ("4*N u16 big-endian"). A container whose endian
marker is "II" would still have its film tables read big-endian, which is what
the measured containers do.

The Lean port reads them big-endian too (`Bytes.u16At data off true`).

---

## 8. Four bugs the port itself had, and how they were caught


Recorded because they are the evidence that the parity test is doing real work.

1. **The version-1 `--original` path wrote the stored payload.** The driver
   called `Container.payload` directly, so a compressed picture's `.dat` came
   out as the LZSS stream instead of its expansion. `tests/game1.rs`
   (`original_mode_writes_the_expanded_payload`) caught it byte for byte
   against `v1::lzss::decompress`. Fixed by dispatching on the container
   version, the way the Rust `cli::picture_payload` does.

2. **The WAV header was big-endian.** `flac` reported
   `ERROR: unsupported format type 256` — 256 is `0x0100`, a byte-swapped `1`,
   which is exactly what a big-endian `WAVE_FORMAT_PCM` reads as. Fixed by
   writing the header little-endian, as the hound-emulating table in
   `PORT-SOUND.md` §12 records.

3. **The report protocol tore on a newline.** `flac`'s stderr ends with `\n`, and
   the failure message carried it into the report's line protocol, so the Rust
   side saw an empty line and reported `unknown report line ''`. Fixed by
   trimming the complaint (`MmfExtract/Sound.lean`), and by skipping empty lines
   nowhere — the protocol is exact, so the producer fixes it, not the consumer.

4. **The differential PCM decoder wrote its samples big-endian.** `le16` put the
   high byte first, so every differential sample was byte-swapped. Nothing in the
   shipped containers' stored-PCM path could notice, and the parity test was
   *silently skipping* — it selected containers by file stem (`app` for
   `app.MMA`) instead of by extension, so it compared nothing and reported
   success. Two fixes: the byte order, and the selector. The regression that
   would have caught the byte order alone is `test/Test.lean`'s
   `checkDpcm`, which reproduces `src/sound.rs`'s three unit-test vectors
   (the silence code, and the per-channel predictor in mono and stereo), plus
   `checkDifferentialSounds`, which decodes all 491 differential resources in
   `app.MMA` and `HD.MMA` and checks the measured contract — one stored byte to
   one 16-bit sample, twice the payload, whole frames. With those in place the
   sound parity test reports 329 sounds byte for byte identical, differential
   resources included.

The last item is the one worth remembering: a parity test that silently selects
nothing is worse than no test, because it reports success. Both selectors now
name the extension (`MMP`/`PIC`/`CPK`, `MMA`/`SND`) and both assert that they
found at least one container.

---

## 9. The binary audit's verdicts

`lean/BINARY-AUDIT.md` re-derived five rules from the game's own instructions
with REA/rizin rather than from the Rust source or the docs. **Nothing in
`lean/src/MmfExtract/` needed correcting** — which is the point of writing the
port in a separate language and then checking it against the binaries. What the
audit found wrong was documentation, and the corrections are recorded here.

| # | Claim | Verdict | The evidence |
|---|---|---|---|
| 1 | `attr & 3` → 11025/22050/44100, `attr & 4` → 16-bit, `attr & 8` → stereo | CORRECT | `FUN_004c5b32` `AND EAX,0x3` @`0x4c5b40`; `FUN_004c5b46` @`0x4c5b54`; `FUN_004c5b5d` @`0x4c5b6b`; `FUN_004c5dd4` materialises `0x2b11`/`0x5622`/`0xac44` with 2 and 3 folding to 44100 |
| 2 | the 256-entry `i16` DPCM table at `0x532778` / file offset `0x132778`, silence `0xf8` | CORRECT | 256 values read, **zero** differing indices against `MmfExtract/Sound.lean`'s `dpcmTable`; `FUN_004c6b80` compares `0xf8`; entry 248 is `0` |
| 3 | the LZ77 four control forms at `0x4c562d`, the end-of-stream marker at `0x4c56bd` | CORRECT | the full instruction listing over `0x4c557a..0x4c56fd`; the marker is `CMP EDI,ECX / JZ` and fires exactly when `base == 0 && bit 3 clear` |
| 4 | the film metadata tables, the rect field order, the third game's chunk types | Layout CORRECT, chunk types CORRECT (third game); `0x477940` and the second game's `0x00`/`0x01` NOT VERIFIED | field-by-field parse of the shipped `DMA3D.MMF`; a `f7 89` kind scan of all three `.flm` payloads. The Windows `DMA3D.exe` is not on the 3D disc (only a Mac PPC build) and `DMA.exe` is absent from this workspace |
| 5 | `models_meta_len = 4*N + 100`, `textures_meta_len = 6*N + 2` | CORRECT | 1,940 vs `4*460 + 100` on `DMA3D.MMM`; 4,232 vs `6*705 + 2` on `DMA3D.MMX`, both measured `names_end` → `offsets[0]` |

The four documentation corrections that followed, each now made:

1. **`docs/DMA3D.md` §2.1's `Textures` region was wrong.** It recorded
   `32*N + 4*N + 2*N + 2` (`38*N + 2` = 26,792 bytes for N=705) — a region 6.3
   times the size of the one the shipped container has. The measured region is
   4,232 bytes, which is `6*N + 2`: the value `src/dma3d.rs` and
   `MmfExtract/Container.lean` both already had. The doc's own history note
   ("measured wrong at first, as `6*N + 2`") had it backwards. **§2.1 now
   records both measured lengths**, with the audit's byte-level derivation.
2. **`src/dma3d.rs`'s `MODELS_META_TRAILER` comment described the wrong
   content.** The audit read the Italian disc's same 100 bytes as
   32-byte-padded `.mod` names, an `0xe2ee` marker and a 16-byte GUID —
   not `u16 2`, `u16 1`, a timestamp, a `DMA3D.MMX` name and `0xff` padding.
   The comment now records both readings and says only the *length* is asserted,
   because nothing parses the trailer either way. `docs/DMA3D.md` §2.1 carries
   the same note, and the `DMA3D.MMX` cross-reference claim is dropped.
3. **`docs/DMA3D.md` §6's three film rects were one disc's, not the other's.**
   The Italian disc measures `(0,0,480,640)`, `(0,27,441,616)`, `(0,0,480,640)`
   where the English disc (which the test suite runs against) holds
   `(0,0,600,800)`, `(60,107,486,696)`, `(60,80,540,720)`. Both sets give the
   same widths and the same 589→592 stride detail that proves the field order,
   so §6 now records both and says which disc each came from.
4. **`docs/PICTURES.md` / `Lz77.lean`'s prose shorthand for the `0x4000` window
   was loose.** The table's `+ 0x4000+` reads like an extra step; the assembly
   reads the base in two places (the marker's comparison, then the copy), which
   is why a zero offset with bit 3 clear ends the stream. The code was right and
   is unchanged; the prose now spells out what the base is.

One claim the audit could not verify from any binary in this workspace is
recorded as unverified rather than dropped: the `MM2DFilmResourceFile` vtable
slot 4 disassembly at `DMA3D.exe`'s `0x477940`, because the 3D disc ships only a
Mac PPC build of the game. The runtime-observed parts of that claim — the table
order, the rect field order, the third game's chunk types — were confirmed
byte-for-byte on the shipped container instead, and §4 of `docs/FILMS.md` says
which is which.
