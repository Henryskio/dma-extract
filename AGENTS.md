# AGENTS.md

Instructions for coding agents working on this repository. `README.md` is for
humans; this file is for you. Read it before editing anything.

## Project overview

`mmf-extract` is a Rust reverse-engineering tool for the proprietary `MMFW`
containers of \*\*\*\*\*\*'s Magic Artist Deluxe / Magico Artista: pictures
(`.MMP`), sounds (`.MMA`), scripts (`.MMS`) and films (`.MMF`). It turns each
resource into a modern lossless format (PNG / WAV / JSON+text), and can also
re-emit the game's original bytes exactly.

The formats are **not documented publicly**. Everything known about them is in
`docs/` (`PICTURES.md`, `SOUNDS.md`, `SCRIPTS.md`, `FILMS.md`, `SEQUEL.md`).
Read the relevant file before touching a decoder, and update it when you learn
something new — the docs are the project's memory, `STATUS.md` is its resume
notes.

**The engine is in-house, not Clickteam Fusion.** The `MMFW` signature is not
Multimedia Fusion; `DMA4.exe` (third game, container version 3) and `DMA.exe`
(second game, container version 2) are custom builds. Do not use MMF2 or
Clickteam documentation as authority; verify against the binaries (`FUN_*`
functions in `DMA4.exe`) and the shipped files.

## Setup

```sh
cd mmf-extract
cargo build --release            # binary: target/release/mmf-extract
```

No network access is needed to build. Dependencies are pinned in
`Cargo.lock`; add new ones only when a format genuinely needs them (image and
audio codecs, `sha2`, `anyhow`, `clap` are already there).

## Tests

```sh
cargo test --release                                   # full suite
DMA_ASSETS=/PathAssets cargo test --release       # point at the game's assets
DMA_ASSETS=/PathAssets cargo test --release --test colors   # one suite
```

* `DMA_ASSETS` — folder with the `.MMP/.MMA/.MMS/.MMF` files. Defaults to
  `/RawAssets`. If the folder is missing the asset-dependent tests **skip and
  pass**; they never fail for a missing asset folder.
* `MMF_PNG_DIR` — where the colour corpus scan looks for extracted PNGs.
  Defaults to `/workspace/extracted/Pictures`.
* `MMF_TEST_TMP` — scratch directory used by extraction round-trip tests.
* `--` separates cargo's own flags from the test binary's; anything after it is
  a test-name filter, not data.

The suite is green-only. If a test fails, fix the code or fix the test's
expectation — never weaken an assertion to make it pass, and never make a
colour/byte claim that the tests do not check.

## Conventions

* **Never edit `/RawAssets`** (read-only input) and never commit extracted
  assets or game data; `extracted/` is gitignored for exactly that reason.
* **Do not send images or audio to an AI or paste them in review.** Verify
  renders programmatically with the helper bins (`imgstat`, `imgcheck`,
  `grid`, `chan`) or by decoding PNGs back in tests.
  * **Exception — other-provider vision APIs.** It *is* allowed to send
    images to vision models hosted on other providers, so a render can be
    checked for "is this what the game shows". The reason this rule exists
    is that Claude Code once forwarded images back into the session's own
    model API, which errored with "too many images" and wedged the run;
    nothing about the game assets is at risk. The exception covers Groq
    (`qwen/qwen3.8-27b`, key in `$GROQ_API_KEY` / `$MSB_GROQ_API_KEY`) and
    the `thaitea/laya-vision-demo` Hugging Face space. Two constraints
    survive: **never send to NVIDIA NIM** (the session's own provider),
    and **never print an API key** — pass it through the environment only.
* **Payload length is ground truth.** The metadata `flags`/`attr`/`kind` bytes
  are hints only; when they disagree with the byte counts, the byte counts
  win. When a payload matches no known layout, report it as a failure — do not
  guess.
* **Every new decoder ships with a 1:1 `--original` test.** Mirror
  `tests/roundtrip.rs`: write the resource's original bytes and assert
  byte-equality against the container slice. Modern output must also decode
  back to the same pixels/samples.
* `src/container.rs` is the shared parser for every container kind. A new kind
  (or a new container version) is a branch there plus a module in `src/`;
  do not fork the parser per asset type.
* Failures are per-resource, not fatal: warn on stderr, count them in the
  report, keep extracting the rest.
* Surface format constants as named `const`s with the evidence in a doc
  comment (e.g. `PALETTE_BYTES`, `CHUNK_TAG`). Magic numbers inline in a
  decoder are a bug waiting to be filed.
* Rust style: `rustfmt` defaults, no `unwrap()`/`expect()` outside tests
  (use `bail!`/`with_context`), keep warnings at zero for new code.
* Keep the analysis scratch bins in `src/bin/` building (`cargo build
  --release` covers them). They are allowed to be rough, not broken.

## Before you finish a task

1. `cargo build --release` — no warnings for new code.
2. `cargo test --release` — green.
3. If you changed a format: update the relevant `docs/*.md` **and**
   `STATUS.md` (counts, open questions, what is measured vs inferred).
4. Commit with a message that says *what changed and how you know* (which
   function, which container, which measurement). Commit early and often.

## Repository layout

Expected content of `/workspace` is described in `docs/WORKSPACE.md` — read it
if a path in these instructions looks missing.
