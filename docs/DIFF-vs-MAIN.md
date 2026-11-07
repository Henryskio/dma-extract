# `lean-rewrite` vs `main` — every difference

This document lists **every** difference between the `lean-rewrite` branch and `main`
(80 commits: `3ba9688e` on `main` → the branch head). It is written so that someone
picking up one piece of the project can tell instantly whether touching it means
touching the Lean core, the Rust reference implementation, or only the docs.

No file is deleted anywhere. Nothing is renamed. Everything is either **new** (39
files) or **modified in place** (28 files).

Totals: **67 files, +21,685 / −794 lines.**

---

## 1. The headline change: the extraction moved to Lean

`main` extracted in Rust. This branch moves every container parse, every decoder and
every output write into **Lean 4**, compiled to `libmmfextract.so` and reached from
Rust through one C-ABI function. The Rust half keeps the command line and the
*reference decoders* the Lean port is checked against.

| | `main` | `lean-rewrite` |
| --- | --- | --- |
| extraction language | Rust | **Lean 4** (6,415 lines in `lean/src/`) |
| Rust's role | the extractor | the CLI + the reference decoders |
| bridge | none | `build.rs` + `src/lean.rs` |
| parity harness | none | **`tests/lean_parity.rs` (570 lines)** |
| Lean-side tests | none | **`lean/test/Test.lean` (473 lines)** |

The measured result is the same extraction both halves produce: 30,404 pictures,
683 sounds, 705 textures, 1,923 films, and 425 of the third game's 460 model payloads
converted. The counts are pinned in `docs/DMA3D.md` §11.55 and `STATUS.md`.

---

## 2. Every added file (39)

### 2.1 The Lean core itself — 27 files

| file | lines | what it is |
| --- | --- | --- |
| `lean/src/MmfExtract.lean` | — | root import |
| `lean/src/MmfExtract/Types.lean` | 30 | shared type aliases |
| `lean/src/MmfExtract/Bytes.lean` | — | little/big-endian readers with bounds errors |
| `lean/src/MmfExtract/Path.lean` | 62 | output-path construction |
| `lean/src/MmfExtract/FFI.lean` | 144 | **the two exported entry points** |
| `lean/src/MmfExtract/Container.lean` | 508 | the single `MMFW` parser, all kinds and versions |
| `lean/src/MmfExtract/Extract.lean` | 303 | `Selection`/`Options`/`Report` and the run loop |
| `lean/src/MmfExtract/Picture.lean` | 462 | mask, paletted and RLE picture decoders |
| `lean/src/MmfExtract/Lz77.lean` | 287 | the shared v2/v3 decompressor |
| `lean/src/MmfExtract/V1.lean` | 50 | version-1 entry point |
| `lean/src/MmfExtract/V1/Lzss.lean` | 120 | version-1 LZSS |
| `lean/src/MmfExtract/V1/Metadata.lean` | 146 | the version-1 metadata block |
| `lean/src/MmfExtract/V1/Picture.lean` | 53 | version-1 pictures |
| `lean/src/MmfExtract/Script.lean` | 731 | script classifier and writers |
| `lean/src/MmfExtract/Film.lean` | 841 | film chunk reader and writers |
| `lean/src/MmfExtract/Sound.lean` | 496 | sound detection, DPCM, FLAC invocation |
| `lean/src/MmfExtract/Png.lean` | 107 | the PNG encoder |
| `lean/src/MmfExtract/Dma3d.lean` | 517 | the 3D run loop and writers |
| `lean/src/MmfExtract/Dma3d/Modfile.lean` | 763 | **the seven model-payload readers** |
| `lean/src/MmfExtract/Dma3d/Texfile.lean` | 413 | the texture readers |
| `lean/src/MmfExtract/Dma3d/Verify.lean` | 278 | the `--original` verification harness |
| `lean/lakefile.lean` | — | the Lake build |
| `lean/lean-toolchain` | — | the pinned toolchain |
| `lean/.gitignore` | — | ignores `.lake/build` |
| `lean/synth/mkmodel.sh` | 73 | synthesizes a test model payload |

`docs/LEAN-CORE.md` describes each module's data structures and algorithm in full.

### 2.2 The Lean-side test harness — 3 files

| file | lines |
| --- | --- |
| `lean/test/Test.lean` | 473 |
| `lean/test/carve3d.py` | 147 |
| `lean/test/run3d.sh` | 44 |

### 2.3 The Rust bridge — 2 files

| file | lines | what it is |
| --- | --- | --- |
| `mmf-extract/build.rs` | — | runs `lake build`, generates the extern signatures, links `libmmfextract.so` |
| `mmf-extract/src/lean.rs` | 234 | the only Rust file that names `mmf_extract` / `mmf_index` |

### 2.4 The parity harness — 1 file

| file | lines |
| --- | --- |
| `mmf-extract/tests/lean_parity.rs` | 570 |

Runs both halves over the same shipped bytes and asserts they agree exactly.

### 2.5 Analysis bins — 3 files

`src/bin/texdump.rs` (201), `src/bin/texmeasure.rs` (124), `src/bin/texcount.rs` (51).
All texture probes; all build, none required by the extraction.

### 2.6 The Lean port's own working notes — 5 files

`lean/PORT-REST.md` (3,686 lines), `lean/PORT-SOUND.md` (2,091 lines),
`lean/CORRECTIONS.md` (211), `lean/BINARY-AUDIT.md` (706), `lean/README.md` (111).

`PORT-REST.md` and `PORT-SOUND.md` are byte-exact porting specifications written so
the Lean modules could be implemented without reading the Rust. **They are working
documents from the port, not format documentation** — the format facts are in
`docs/`. If the project is split, these belong with the porting history, not with a
format owner.

---

## 3. Every modified file (28)

### 3.1 The build and the bridge

#### `mmf-extract/Cargo.toml`
Adds the `build.rs` build script.

#### `mmf-extract/build.rs` (new — see §2.3)
Compiles the Lean project, emits the two extern signatures, links the library.

#### `mmf-extract/src/lean.rs` (new — see §2.3)
Declares `mmf_extract` / `mmf_index`, `Request`/`Report` structs, and
`write_3d_index`.

#### `mmf-extract/src/lib.rs` (+18 / −9)
**Doc-comment only.** The module map in the crate doc now names `MmfExtract.*` for
the Lean side and states that the Rust decoders exist as the reference implementation
the port is checked against. No behaviour change.

#### `mmf-extract/src/main.rs` (+22 / −2)
One **behavioural** change: adds a `--modern` flag, `conflicts_with = "original"`, so
the pair can be read side by side and a contradiction is a clap error rather than
silently picking one. `--original` remains the same mode it always was.

#### `mmf-extract/src/container.rs` (+202 / −80)
The shared parser gains the version-1 delegation this branch's work exposed, plus
doc evidence for the `0x24`/`0x20` offset base, the `32 * n` name table, and the
endianness marker. **The dispatch by kind and version stays the only two places that
map a version to code**, per `AGENTS.md`. The measured container facts are pinned by
`tests/dma3d.rs::the_metadata_regions_are_measured`.

#### `mmf-extract/src/cli.rs` (+224 / −287)
Two changes:

1. **`picture_decoder` and `picture_payload`** are now the only two places a container
   version maps to code — they select the Rust or `v1` decoder, exactly as the Lean
   `V1.*` modules are selected on the Lean side.
2. **`blobs_extractor` and `extract_blobs_verbatim` were removed** (−287): version-2/3
   blob containers are written through verbatim by a path that no longer needs a
   version branch. The first game's blobs still route through `v1::blobs`.

#### `mmf-extract/src/lz77.rs` (+8)
**Doc-comment only, but it records a format finding.** The `0x4000` in the distance
table is a *base*, not an extra step: the assembly's end-of-stream test subtracts the
offset first and only then the base, which is why a zero offset with bit 3 clear ends
the stream rather than reaching 0x4000 bytes back. The copy uses
`0x4000 + (O0>>2) + (O1<<6)` with bit 3 subtracting a further `0x4000`. Provenanced to
`lean/BINARY-AUDIT.md` §3.

### 3.2 The third game's 3D half — the largest Rust changes

These files carry the readers the branch's 3D work added. **Both halves implement
every reader**; the Rust files below are the reference implementation the Lean
`Dma3d/Modfile.lean` is checked against.

#### `mmf-extract/src/dma3d/modfile.rs` (+1,035 / −5)
Seven readers, each in both halves:

| reader | shape it reads | documented at |
| --- | --- | --- |
| `parse_mesh` | `mod II` chain with a bit-identical constant slot | §11.9, §11.26 |
| `parse_profile` | `mod II` chain with a marker of exactly 0.0 or 1.0 | §11.25, §11.26 |
| `parse_magic_record` | `mod II` stream neither of the above claims | §11.31, §11.38 |
| `parse_param_table` | small values, alternating positions, ≥80% fill | §11.37, §11.39 |
| `parse_name_table` | NUL-terminated texture names in 32-byte slots, plus names the slice cut off | §11.40, §11.54 |
| `parse_value_stream` | `f32` run found **anywhere**, no constant slot | §11.41, §11.52 |
| `parse_index_table` | byte values, alternating positions, ≥90% fill, values above the 128 rule | §11.55 |

Every threshold in those readers is stated with the measurement that produced it
(§11.53's calibration), and each reader has unit tests in both halves.

#### `mmf-extract/src/dma3d/gltf.rs` (+541 / −1)
Six writers. The JSON+text writers all share the four keys `name`, `what this is`,
`meaning`, `measured`, where `meaning` says what is *not* determined. **Four separate
JSON-validity bugs** were found and fixed here this session, each now covered by a
test that reads the writer's own output back — see `docs/MISTAKES.md`.

#### `mmf-extract/src/dma3d.rs` (+271 / −171)
The dispatch chain (mesh → profile → record → param → name → stream → index →
passthrough) and `Extract3DReport`'s per-family counts, so a run can prove every
payload is accounted for.

#### `mmf-extract/src/dma3d/texfile.rs` (+252 / −30)
The texture readers: masks, paletted, and the palette/index split read from the
payload's own bytes. Where the split is not pinned the smallest admissible one is read
and the resource is counted in `ambiguous`.

#### `mmf-extract/src/dma3d/render.rs` (+104 / −1)
The preview rasterisers for meshes and profiles.

### 3.3 The tests

| file | +/− | what changed |
| --- | --- | --- |
| `tests/lean_parity.rs` | **+570** | new — the parity harness |
| `tests/dma3d.rs` | +509 / −104 | the accounting test now closes on **eight** output families, and the "payloads that fit no reader" filter grew a term per new reader |
| `tests/game1.rs` | +75 / −8 | version-1 dimension-table and metadata tests |
| `tests/roundtrip.rs` | +57 / −23 | every new decoder's 1:1 `--original` test |
| `tests/game1_payloads.rs` | +25 / −16 | first-game payload tests |
| `tests/dma3d_numbers.rs` | +8 | the third game's container-level numbers |

---

## 4. Every modified doc

| doc | +/− | what was added |
| --- | --- | --- |
| `docs/DMA3D.md` | +3,224 / −41 | §§11.14–11.55: the 3D chain's full measured history, including every withdrawn claim |
| `docs/PICTURES.md` | +56 | the two-section RLE format (§7.3), the shared reader rules |
| `docs/SEQUEL.md` | +8 | the second game's deltas name the Lean decoders |
| `docs/SCRIPTS.md` | +8 | the scripts decoder names the Lean side |
| `docs/FILMS.md` | +8 | the film chunk reader names the Lean side |
| `docs/SOUNDS.md` | +6 | the FLAC wiring names the Lean side |
| `docs/GAME1.md` | +10 | version-1 names the Lean side |
| `docs/WORKSPACE.md` | +1 | workspace layout |
| `STATUS.md` | +107 / −11 | the current counts, including 425 of 460 models |
| `README.md` | +9 | the Lean core, the two halves, the parity harness |
| `AGENTS.md` | +15 | the rules the port had to follow (debug builds only, never weaken an assertion, per-resource failures, the 1:1 `--original` test) |

`docs/LEAN-CORE.md` and `docs/MISTAKES.md` are new on this branch.

---

## 5. What did *not* change

* **No container format changed.** The `MMFW` layouts are the same bytes they were on
  `main`; this branch adds readers for payloads `main` also could not read.
* **No file was deleted or renamed.**
* **The Rust reference decoders still exist**, because `src/bin/`'s analysis bins and
  `tests/lean_parity.rs` read them. They are the evidence a Lean decode is compared
  with, not dead code.
* **The output file names and formats are unchanged**, so an extraction run on this
  branch produces the same paths `main` did for the resources both convert.

---

## 6. For whoever splits the project

The natural seams, in dependency order:

1. **`lean/src/MmfExtract/Bytes.lean`, `Types.lean`, `Path.lean`** — the shared floor,
   no format knowledge. Anything can depend on these.
2. **`Container.lean` + `Extract.lean` + `FFI.lean`** — the parse/dispatch/report spine.
   Every decoder depends on `Container`; nothing depends on a decoder.
3. **`Picture.lean` / `Lz77.lean` / `V1/*` / `Script.lean` / `Film.lean` / `Sound.lean`
   / `Png.lean`** — one asset family each, mutually independent.
4. **`Dma3d/*`** — the 3D half, independent of the other families but the only place
   seven readers share one dispatch chain.
5. **`src/`** — the CLI plus the reference decoders, kept for parity. A split that
   drops a decododer from Rust must also drop its half of `tests/lean_parity.rs`.
6. **`docs/`** — one chain per game; an edit inside one game's chain cannot change
   another's. `docs/LEAN-CORE.md` is the module map, `docs/DMA3D.md` §§11+ is the 3D
   measurement history, `docs/MISTAKES.md` is the error record.
