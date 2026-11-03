# `lean/` — the extraction core

Everything that parses an `MMFW` container or decodes a resource lives here, in
Lean 4. The Rust crate around this directory keeps the command line and nothing
else.

## Build

```sh
cargo build                       # from /workspace/mmf-extract: runs both
sh build/build.sh                 # this directory alone
```

`build/build.sh` is the whole pipeline:

1. `lake build` — compiles the library and emits the C for each module into
   `.lake/build/ir/`.
2. `leanc -c <module>.c -o … -DLEAN_EXPORTING` — `LEAN_EXPORTING` is the switch
   that makes `LEAN_EXPORT` actually export; without it every Lean symbol is
   local and the Rust side cannot find the entry point.
3. `leanc -shared -o build/libmmfextract.so … -lleanshared` — links the Lean
   runtime in, so the library is self-contained.

`src/lean.rs` (Rust) declares the resulting C ABI; `build.rs` generates those
declarations by reading the symbols the built library exports, because the
module initializer's name carries the Lake package name (`mmf-extract-lean`
becomes `mmf_x2dextract_x2dlean`).

## The boundary

One function, one request string in, one report string out
(`src/MmfExtract/FFI.lean`):

```text
asset_dir	/RawAssets
output	/workspace/extracted
types	pictures,sounds
original	0
raw	0
```

```text
pictures	30404
sounds	683
total	31187
failed	2
fail	/RawAssets/app.MMP: resource 12 'name': <reason>
```

Line-oriented on purpose: neither side has to know the other's types, and a
report is diffable text.

## Modules

| Module | What it holds | Mirrors |
|---|---|---|
| `Types` | the two types every module shares (kind, dimensions) | `src/container.rs`'s enums |
| `Bytes` | byte reads, UTF-8 decoding, hex formatting | — |
| `Path` | file-name helpers and the resource-name sanitizer | `src/cli.rs::sanitize` |
| `Container` | the shared (v2/v3) parser, the Scripts object database, the common header | `src/container.rs` |
| `V1.Metadata` | the first game's metadata region | `src/v1/metadata.rs` |
| `V1.Lzss` | the first game's LZSS decompressor | `src/v1/lzss.rs` |
| `V1.Picture`, `V1` | the first game's picture path and palette bank | `src/v1/picture.rs`, `src/v1/mod.rs` |
| `Lz77` | the shared LZ77 decompressor and its compressor | `src/lz77.rs` |
| `Png` | a PNG writer (stored deflate blocks, CRC-32, Adler-32) | the `png` crate's output |
| `Picture` | the shared picture decoder | `src/picture.rs` |
| `Sound` | the sound decoders, the WAV bytes and the `flac` subprocess | `src/sound.rs` |
| `Film` | the film decoder: chunks, deltas, RLE, manifest | `src/film.rs` |
| `Dma3d.Texfile` | the 3D game's texture decode and orientation decision | `src/dma3d/texfile.rs` |
| `Dma3d` | the 3D game's driver: verbatim, PNGs, the index page | `src/dma3d.rs` |
| `Script` | the Scripts corpus (JSON + text dumps) | `src/script.rs` |
| `Extract` | the driver: which container, which decoder, which output | `src/cli.rs::run` |
| `FFI` | the one exported entry point | — |

The layering rule is the repository's: a game's own module states that game's
deltas and points at the shared module rather than restating it. `Container`
imports `V1.Metadata` and never the other way round, which is why the shared
parser and the first game's cannot be edited into each other.

## What is not ported yet

`Dma3d`'s **modern model path** — `src/dma3d/gltf.rs`, `modfile.rs`, `anim.rs`,
`render.rs` (3,073 lines of Rust in total: the mesh parse, the glTF writer, the
animation records and the preview renderer). `lean/PORT-REST.md` is the
byte-exact specification for it. Until it is ported, `Dma3d.extractGltf` reports
every model as a failure with that reason, because writing the payload through
under a `.gltf` name would be the silent format change `AGENTS.md` forbids; use
`--original` for the verbatim payload.

## Test

```sh
lake build
LEAN_PATH=.lake/build/lib/lean lean --run test/Test.lean
```

`test/Test.lean` checks the LZ77 round-trip and prints what the container parser
makes of `/RawAssets`. The byte-exact assertions live in the Rust suite:
`tests/lean_parity.rs` compares this core's output against the Rust reference
decoders resource by resource.
