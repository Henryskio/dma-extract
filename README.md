# \*\*\*\*\*\*'s Magic Artist Deluxe / Magico Artista — MMFW asset extraction

Reverse-engineering extraction of the proprietary `MMFW` asset archives
(`.MMP` pictures, `.MMA` sounds, `.MMS` scripts, `.MMF` films) of \*\*\*\*\*\*'s
Magic Artist Deluxe / Magico Artista, plus the second game in the series
(\*\*\*\*\*\* Magico Artista 2).

| Start here | |
|---|---|
| `mmf-extract/README.md` | the extractor: what it produces, how to run it |
| `docs/` | the reverse-engineered formats, one document per asset type |
| `docs/WORKSPACE.md` | what this workspace is supposed to contain |
| `STATUS.md` | current state: counts, tests, known gaps |
| `AGENTS.md` | instructions for coding agents working here |

The code and the documentation are licensed under the **GNU Affero General
Public License v3** (`LICENSE`). See `mmf-extract/README.md` for the build and
the "this project is fully vibe coded" notice.

**The extraction is written in Lean.** The Lean core in `mmf-extract/lean/`
(`libmmfextract.so`, built by `mmf-extract/build.rs`) is what decodes every
container: pictures, sounds, scripts, films, and the 3D game's textures, models
and drawing profiles. The Rust side keeps the command line and a reference
implementation of each decoder that `mmf-extract/tests/lean_parity.rs` checks the
Lean core against — byte-identical sounds, pixel-identical pictures, identical
scripts. A format change therefore means editing both halves together, and each
`docs/` chain records the measurements the decoders rest on.
