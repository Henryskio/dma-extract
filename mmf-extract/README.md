# mmf-extract — MMFW asset extractor for \*\*\*\*\*\*'s Magic Artist Deluxe

A reverse-engineering extractor for the proprietary `MMFW` containers
(`.MMP` pictures, `.MMA` sounds, `.MMS` scripts, `.MMF` films) shipped with
\*\*\*\*\*\*'s Magic Artist Deluxe / \*\*\*\*\*\* Magico Artista.

It converts the game's own asset archives into modern, lossless formats, and
it can also re-emit every resource in the game's original on-disk format
byte-for-byte when you need to prove nothing was lost.

| Container | Input | Default output (`--original` writes the game's bytes) |
|---|---|---|
| `.MMP` | Pictures | `frame_NNN.png` — 32bpp `[transparency, R, G, B]`, 8bpp paletted, 8bpp coverage masks |
| `.MMA` | Sounds | `NNNNN_name.flac` — unsigned 8-bit PCM plus the game's table-driven differential codec |
| `.MMS` | Scripts | object database: `container.json`, `types.txt`, `strings.txt`, per-object dumps |
| `.MMF` | Films | inner `.flm` chunk stream → per-frame PNG + `manifest.json` |

Measured on the shipped assets of the third game in the series: **30,299
pictures, 683 sounds, 4,813 script objects, 139 film frames**. A second game
in the series (\*\*\*\*\*\* Magico Artista 2, container **version 2**) parses with
the same code: **38,447 pictures, 891 sounds, 1 film**. See
[`docs/SEQUEL.md`](../docs/SEQUEL.md).

## Quick start

```sh
cargo build
./target/debug/mmf-extract /PathAssets -o out                   # modern, lossless
./target/debug/mmf-extract /PathAssets -o out --original        # game's own bytes
./target/debug/mmf-extract /PathAssets -o out --types pictures,sounds
```

* `ASSET_DIR` — folder holding the `.MMP` / `.MMA` / `.MMS` / `.MMF` files
  (positional; files are identified by their `MMFW` signature, not by name).
* `-o, --output DIR` — defaults to the current directory.
* `-t, --types LIST` — `all` (default), `pictures`, `sounds`, `scripts`,
  `films`.
* `--original` — write the game's original format instead of modern files.
* `--raw` — also write the untouched compressed payload of each picture.

Tests need the assets; they skip themselves otherwise:

```sh
DMA_ASSETS=/PathAssets cargo test                # or /RawAssets, the default
MMF_PNG_DIR=/PathAssets/extracted/Pictures cargo test    # colour corpus scan
MMF_TEST_TMP=/tmp/t cargo test                   # scratch dir for round-trip tests
```

Build debug, always: this is the project's convention (`AGENTS.md`). Debug
keeps `debug_assert!` and the overflow checks on, and the panics readable,
which is worth far more here than the run time.

See [`docs/WORKSPACE.md`](../docs/WORKSPACE.md) for what the workspace around
this repository is expected to contain.

## Requirements

`cargo build` needs only the crates pinned in `Cargo.lock` — no network
access, no system libraries.

The **sound** output additionally needs the official `flac` command-line tool
(`apt install flac` / `brew install flac`), which is what `src/sound.rs` uses
to encode. There is no Rust encoder for FLAC here, and ffmpeg's cannot write
8-bit FLAC — it would silently promote the game's 8-bit audio to 16-bit, so
the output would misreport the depth the container stored. Without the tool
the extraction stops and says so, rather than quietly switching to WAV.
`docs/SOUNDS.md` §6.1 has the measurements.

## Documentation

Everything that was reverse-engineered is written up per media type:

| File | Contents |
|---|---|
| [`docs/PICTURES.md`](../docs/PICTURES.md) | container header, metadata region, pixel/palette byte order, 8bpp layouts, colour tests |
| [`docs/SOUNDS.md`](../docs/SOUNDS.md) | sound metadata layout, format byte, differential codec and its predictor table |
| [`docs/SCRIPTS.md`](../docs/SCRIPTS.md) | the serialized object database (types, records, string tails) |
| [`docs/FILMS.md`](../docs/FILMS.md) | the inner `.flm` chunk stream, key/delta frames, timing |
| [`docs/SEQUEL.md`](../docs/SEQUEL.md) | the second game in the series: what differs, and how it was obtained |
| [`docs/WORKSPACE.md`](../docs/WORKSPACE.md) | the expected `/workspace` layout |

Facts in those files were derived from the game's own executable
(`DMA4.exe` / `DMA.exe`) plus the shipped files, and are checked
numerically — the colour tests decode all 30,299 PNGs back from the game's
bytes rather than trusting a render. Known gaps are listed in `STATUS.md` and
in each document's open-questions section, not hidden.

## About how this was written

**This project is fully vibe coded.** Every line of Rust, every test and every
word of the documentation was produced by an AI coding agent working from the
original binaries — no code was typed by hand.

**The outputs were reviewed by a human** before being committed: the extracted
assets were re-decoded and checked byte-for-byte against the containers, the
colour claims were verified numerically on named sprites, and the wrong
conclusions were corrected rather than kept (see the changes recorded in
`git log` — a channel-order bug and a mislabelled format variant are both
documented as history). Treat the human review, not the agent, as the
authority.

Because the agent worked only from the shipped files, the write-ups describe
what the loader in the executable actually does, and separate "measured" from
"inferred" wherever the evidence is thin.

**Coding agents:** instructions for working on this repository live in
[`AGENTS.md`](../AGENTS.md) at the repository root.

## License

Licensed under the **GNU Affero General Public License v3** — see
[`LICENSE`](../LICENSE) at the repository root. **The AGPL v3 covers both the
code and the documentation**: the Rust sources and tests in `mmf-extract/` and
the format write-ups in `docs/` are all licensed under it. Note that AGPL v3
extends to network use: if you run a modified version as a service, you must
offer your users its source. The dependencies in `Cargo.lock` keep their own
licenses.
