# Every mistake made on this branch

`main` extracted correctly for what it extracted. This branch's work was to move the
extraction to Lean and then to solve the third game's unsolved payloads. Along the way
a specific set of errors was made, found, and recorded rather than quietly fixed.

This document is the consolidated record. It exists because a project about to be split
and handed to other agents needs its error history in one place: **the same mistakes
repeat when a reader cannot see that they were already made once.** Every entry names
where it was caught and what now prevents it.

Nothing here is a claim of correctness about anything not listed. For the measurement
history see `docs/DMA3D.md` §§11.1–11.55; for the port-specific differences see
`mmf-extract/lean/CORRECTIONS.md`.

---

## 1. Format / measurement mistakes

### 1.1 "The payloads are encrypted" — withdrawn

Claimed from entropy alone: 46 payloads measured 6.38 bits/byte, which was read as a
cipher. **Withdrawn.** The same payloads carry the magic and the record stream every
readable one carries; the entropy was a run of zeros plus compressed data, and the
measurement that distinguished them was a *repeat-period* scan, not a histogram.

*Recorded at:* `docs/DMA3D.md` §11.16, withdrawn at §11.17 and §11.31.
*Lesson:* entropy is a property of a histogram, not of a format. A high-entropy payload
is not evidence of anything until a period or a magic is looked for.

### 1.2 "The `mod II` records are `f32` big-endian" — wrong twice

First reading assumed `f32`, then assumed big-endian. The values are **little-endian
`f32` at four-byte steps**. Caught by reading a payload whose first record is known
(`0.25, 0.5, 0.0, 0.99999` in `r5`) and checking both byte orders against it.

*Recorded at:* `docs/DMA3D.md` §11.22.

### 1.3 "The record stride is 16" — one earlier claim was wrong

The stride is 16 in **most** of the container, and that "most" is the whole finding.
The earlier claim was that the stride is 16 everywhere; payloads whose records sit at
a different offset were read as garbage until the run carrier's own extent was used
rather than a fixed stride.

*Recorded at:* `docs/DMA3D.md` §11.22.

### 1.4 "Not in this file" — the PEF carve boundary

`§11.23` concluded the relocated dispatch thunk was not in the carved file. **Wrong**:
the thunk is 0xfd00 past the image and the only section that could hold it is the
compressed one, which means the conclusion was about the carve, not the address.

*Recorded at:* `docs/DMA3D.md` §11.24, which withdraws §11.23's narrower "not in this
artifact".

### 1.5 "Encrypted" again via `ReadData` — withdrawn correctly

A second encryption claim, this time from `ReadData` holding no cipher. Withdrawn,
because `ReadData` holding no cipher **proved nothing about the payloads**.

*Recorded at:* `docs/DMA3D.md` §11.25, §11.34.

### 1.6 "One pixel away" / divergence — withdrawn

A picture-render divergence was claimed, then measured to zero. Withdrawn at §11.30.

### 1.7 "The `bl` at 0x1633ec is a relocated dispatch" — withdrawn, four sessions later

The longest-lived wrong claim on this branch. A branch instruction at `0x1633ec` was
read as the game's payload-consuming dispatch and its target resolved through a
relocation chain that was never actually decoded. When Ghidra's `RelocSetPosition`
source finally settled the operand split (§11.46), the verdict was: the branch is **not
relocated**, its target lands in no section of the PEF, and the instruction itself was
a **misaligned read** — a byte pattern inside an operand, the same mistake made fifteen
times over in the relocation stream.

*Recorded at:* `docs/DMA3D.md` §11.17 through §11.46.
*Lesson, and the expensive one:* **an instruction boundary is not a fact you can assume
in a stream of fixed-width words.** A stream whose entries are two bytes each and four
bytes each has words that are not entry starts, and filtering a flat array by opcode
finds opcodes inside operands. Everything built on this claim — §§11.9, 11.20, 11.35,
11.36 — was withdrawn with it.

### 1.8 "§11.29's entropy census" — the buckets were not real

`§11.29` reported the unconverted payloads as *81 of no readable structure, 69 of mid
entropy, 61, 20* — a set of buckets. **Measured directly, the spread is continuous
from 1.1 to 6.2 bits per byte with no buckets at all.** Reader decisions were taken on
a shape census that was not real.

*Recorded at:* `docs/DMA3D.md` §11.40, which corrects §11.29.

### 1.9 "The `00 05 45 XX 01` signature precedes texture names" — withdrawn as a signature

Measured as a repeated 5-byte tag in 4 payloads, with a name after it. Scanning the
**whole container** found 23 occurrences across 5 payloads with the tag byte
incrementing monotonically (`0x20, 0x21, 0x22 …`) — coincidental matches inside
structured data. In `r33` the same offset holds `65 74 2e`, the bytes `et.` of a name.

The *useful* thing inside the false signature was real: two of those payloads carry
names the container's slice cut off. That part became a reader (§11.54).

*Recorded at:* `docs/DMA3D.md` §11.54.
*Lesson:* a pattern found in 4 of 115 is not a signature until it is looked for in the
other 345 and the *absence* is measured too.

### 1.10 "`r19` and `r156` are the alternating-zero shape" — withdrawn

Concluded from `odd-zero ≥ 90%` that four payloads were §11.37's shape read as text.
Follow-up measurement found §11.50 had only tested the odd parity; **six** payloads
pass once the even parity is tested too, and `r19`/`r156` have under a third of their
bytes zero in *either* half, so they are not that shape at all. The next step a
reviewer proposed was built on this error, and measuring it took the reviewer's step
and found it unavailable.

*Recorded at:* `docs/DMA3D.md` §11.51, which withdraws §11.50.

### 1.11 "The 5-byte signature leads to a name-table variant" — the same error again

The reviewer's next step named `r76` and `r80` as name-table payloads. Dumping the
bytes at those offsets shows the "signature" is followed by `f32` coordinates
(`1.0, -0.286`) — it was noise inside float data, and `r76`/`r80` carry no names at
all.

*Recorded at:* this branch's §11.55 measurement, summarised in §11.46.

### 1.12 Documentation corrections found by the binary audit

Four prose errors, all found by re-deriving from the shipped executables with REA/rizin
rather than from the Rust or the docs:

1. **`docs/DMA3D.md` §2.1's `Textures` region was wrong.** It recorded `38*N + 2`
   (26,792 bytes for N=705) — 6.3× the shipped container's real 4,232 bytes, which is
   `6*N + 2`. The doc's own history note ("measured wrong at first, as `6*N + 2`") had
   it **backwards**. §2.1 now records both.
2. **`MODELS_META_TRAILER`'s comment described the wrong content.** The same 100 bytes
   on the Italian disc are 32-byte-padded `.mod` names, an `0xe2ee` marker and a
   16-byte GUID, not the `u16 2`/timestamp/`DMA3D.MMX` form the comment described. Only
   the *length* is asserted now.
3. **`docs/DMA3D.md` §6's three film rects were one disc's, not the other's.** The
   Italian disc measures `(0,0,480,640)`/`(0,27,441,616)`/`(0,0,480,640)` where the
   English disc the suite runs against holds `(0,0,600,800)`/`(60,107,486,696)`/
   `(60,80,540,720)`. Both sets give the same widths and the same stride detail that
   proves the field order, so §6 now records both and says which disc each came from.
4. **`docs/PICTURES.md` / `Lz77.lean`'s prose for the `0x4000` window was loose.** The
   table's `+ 0x4000+` reads like an extra step; the assembly reads the base in two
   places. The code was right and is unchanged; the prose now spells out what the base
   is.

*Recorded at:* `mmf-extract/lean/CORRECTIONS.md` §9, `lean/BINARY-AUDIT.md`.

---

## 2. Code bugs

### 2.1 The four JSON writers that were not checked against their reader

Every one of these was found by **reading the file the writer had just produced** with
`json.load`. Three of the four were found *after* the extraction had already reported
success, because the run reported file counts and not document validity.

| # | writer | the bug | caught by | now pinned by |
| --- | --- | --- | --- | --- |
| 1 | `write_param_json` (§11.39) | strings written with Rust's `\` line continuation — a backslash **and a newline** inside a JSON string literal | `json.load` on the output | `param_json_tests::a_written_table_document_is_valid_json` |
| 2 | `write_param_json` (§11.39) | `"fill is zero for": 89% of its bytes` — not a JSON value at all | `json.load` | same test |
| 3 | `write_name_json` (§11.54) | an empty list written as **nothing** after the colon, so 11 of 190 documents had a member with no value | `json.load` | `name_json_tests::a_written_table_document_is_valid_json` |
| 4 | `write_index_json` (§11.55) | the percentage and the range unquoted, **and** the `meaning` literal carrying a `\` continuation | `json.load` | `index_json_tests::a_written_index_document_is_valid_json` |

**This is the single most repeated mistake of the branch — four times, across four
different writers.** The cause is not unfamiliarity with JSON: it is that the writer and
its reader were written in the same pass, so nothing checked the seam. `AGENTS.md`
already required "every new decoder ships with a test that reads its output back"; this
is where that rule earned its place, and the fix was applied retroactively to all six
JSON+text writers.

### 2.2 A Lean test that was unfalsifiable

`parse_name_table` first carried a `first_is_truncated` field, set by asking whether the
payload's first byte was the name's first byte. **The test is true whether or not the
name is cut**, so the field could only ever be filled by guessing. Removed rather than
filled; the names are recorded verbatim instead. §11.40 records it.

A struct field that can only be filled by guessing is the same mistake as a threshold
that can only be made to fit, one level down.

### 2.3 Two probes that were written badly and produced zero

Two measuring bins returned `0 of 42` and would have ended two investigations, both
because of a bug in the *probe*:

1. A spread test written `lo - hi > 0.5` instead of `hi - lo > 0.5`, which is always
   false.
2. A run-length counter that returned **bytes** where the reader counted **values**, so
   a 60-byte run read as 60 values and passed a 16-value threshold it did not meet.

Both were found by comparing the probe's answer against the reader's own accepted
population, which is why the calibration step came to be the first thing done rather
than the last. §11.41, §11.53.

### 2.4 The extraction hook that silently did not apply

An edit to `src/dma3d.rs` to wire a new reader into the chain failed with an
`AssertionError` — the anchor text did not match — and the failure was not noticed
because it was the last line of a script whose earlier output had already scrolled
past. The extraction then ran with the reader compiled, its unit tests green, and its
hook absent, so the reported count did not move. §11.40's reader needed a second pass
to land.

*Lesson:* an edit script that ends in an assertion failure must be treated as "the edit
did not happen", and the count must be checked after it, not the build.

### 2.5 A committed analysis bin overwritten by a scratch bin

A measurement bin was created as `src/bin/probe.rs` and deleted afterwards. That name
was **already a committed file** (commit `2b55b75`, picture colour-channel work).
`git status` caught it as a deletion and it was restored with
`git checkout HEAD --` and re-verified to compile. §11.49 / the §11.49 commit.

*Lesson:* use a unique name for a scratch bin. `probe` is exactly the name someone else
will have used.

---

## 3. Port-specific bugs — found because the port was in a second language

These are in `mmf-extract/lean/CORRECTIONS.md` §8, listed here for completeness because
they are the reason the port exists in a separate language at all: a Rust bug that
copies itself is invisible, and a Lean bug that copies itself is a diff.

1. **The version-1 `--original` path wrote the stored payload** rather than the decoded
   stream. Caught by the `--original` round-trip test.
2. **The WAV header was big-endian.** `flac` reported it, which is why the encoder is
   run rather than piped silently.
3. **The report protocol tore on a newline.** `flac`'s stderr ends with `\n`, and the
   report's line-oriented form meant one stray terminator desynchronised every
   following field.
4. **The differential PCM decoder wrote its samples big-endian.** `le16` put the byte
   pairs in the wrong order, so the decoded audio was noise rather than mismatched-but-
   plausible.

Four more decisions where the port chose differently from the prose or the code, each
recorded with its evidence in `CORRECTIONS.md` §§1–7: the encoder is `flac` and not
`ffmpeg`; `--original` for the first game writes the *decoded* stream;
`textures_meta_len` is `6*N + 2`; the glTF writer does not subtract one from indices;
`StringTable` is classified on the tail length alone; the film metadata solve takes the
**first** candidate, not the largest; the film metadata region is big-endian by
construction.

---

## 4. A methodological mistake that cost the most

The pattern across §1.7, §1.9, §1.10, §1.11 and §11.49 is not four unrelated errors.
It is one: **a claim was stated, then work was built on it, and only later was the
claim checked against the whole population.**

* `0x1633ec` — read once, built on for four sessions, then shown to be a misaligned
  operand (§1.7).
* The 5-byte signature — found in 4 payloads, never looked for in the other 445 until
  it was (§1.9).
* The UTF-16 parity — tested on one parity, so the group it named was wrong (§1.10).

The correction that now exists in the code is the *calibration* step: before any
threshold is moved or any reader is written, the accepted population is measured and
the threshold is placed on the boundary **that population** draws. §11.41, §11.53 and
§11.55 are all that step applied, and it is the reason the 3D readers' thresholds are
now stated with their measurements rather than argued for.

**For whoever picks this up:** the four withdrawn claims above are recorded precisely so
that they are *not* re-derived. If a future measurement looks like it is recovering
§1.7's dispatch, §1.9's signature or §1.10's UTF-16 shape, check the section that
withdrew it first — the measurement that killed it is still in the docs.

---

## 5. What was *not* a mistake, and is worth not re-litigating

* **Writing the 3D records, tables, name tables and streams as JSON+text rather than
  glTF.** They are data records, not geometry; writing them as a line loop would invent
  a shape the payload does not carry. `AGENTS.md` forbids exactly that, and the
  `meaning` field in each document says what is not determined.
* **Leaving 35 model payloads unconverted and reported per resource.** The alternative
  was a reader whose thresholds were moved until they fit, which is a guess. Every
  threshold now in the code sits on a measured boundary.
* **Not guessing the name-table slot→resource mapping.** §11.47/§11.48 measured that the
  `Models` container's name table is interleaved with non-name records and that the
  mapping is not sequential; a reader that assumed it would silently rename 460
  resources and change every output filename.
