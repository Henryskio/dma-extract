# AGENTS.md

Instructions for coding agents working on this repository. `README.md` is for
humans; this file is for you. Read it before editing anything.

## Project overview

`mmf-extract` is a Rust reverse-engineering tool for the proprietary `MMFW`
containers of \*\*\*\*\*\*'s Magic Artist Deluxe / Magico Artista: pictures
(`.MMP`), sounds (`.MMA`), scripts (`.MMS`) and films (`.MMF`). It turns each
resource into a modern lossless format (PNG / FLAC / JSON+text), and can also
re-emit the game's original bytes exactly.

**External dependencies.** `cargo build` needs only the pinned crates. The
*sound* output additionally needs the official `flac` tool
(`apt install flac` / `brew install flac`) because no Rust crate here
encodes FLAC and ffmpeg's encoder cannot do 8-bit — `docs/SOUNDS.md` §6.1
records the measurements. `src/sound.rs` fails loudly and once, naming the
install command, when it is missing, rather than silently writing WAV.

The formats are **not documented publicly**. Everything known about them is in
`docs/`, and the docs are layered the same way the code is — one chain of
documents per game, never one document describing two:

* **Shared (container versions 2 and 3).** `docs/PICTURES.md`,
  `docs/SOUNDS.md`, `docs/SCRIPTS.md` and `docs/FILMS.md` describe the format
  the second and third games share, whose containers differ only in the two
  bytes at `0x12`. `src/*.rs` mirrors them: one code chain, no version branches.
* **Second game (version 2).** `docs/SEQUEL.md` holds *only* its deltas.
* **First game (version 1).** `docs/GAME1.md` holds the 1997 build, whose
  header bases, metadata layout, palette placement and payload compression all
  differ. `src/v1/*.rs` mirrors it for the same reason.

The rule both sides follow: a game's own file states that game's deltas and
points at the shared document rather than restating it, so an edit inside one
game's chain cannot silently change the other's. Read the relevant file before
touching a decoder, and update it when you learn something new — the docs are
the project's memory, `STATUS.md` is its resume notes.

**The engine is in-house, not Clickteam Fusion.** The `MMFW` signature is not
Multimedia Fusion; `DMA4.exe` (third game, container version 3) and `DMA.exe`
(second game, container version 2) are custom builds. Do not use MMF2 or
Clickteam documentation as authority; verify against the binaries (`FUN_*`
functions) and the shipped files. There are three of them, one per game, and
each is the authority for its own container version — `DMA4.exe` for versions
2/3 (`docs/PICTURES.md` & co.), `DMA.exe` for version 2 (`docs/SEQUEL.md`), and
the first game's `DMA1.exe`, the disc's `Disney's Magic Artist.exe`, for
version 1 (`docs/GAME1.md` §1 has the carve table).

## Setup

```sh
cd mmf-extract
cargo build                      # binary: target/debug/mmf-extract
```

No network access is needed to build. Dependencies are pinned in
`Cargo.lock`; add new ones only when a format genuinely needs them (image and
audio codecs, `sha2`, `anyhow`, `clap` are already there).

## Tests

```sh
cargo test                                          # full suite
DMA_ASSETS=/PathAssets cargo test                   # point at the game's assets
DMA_ASSETS=/PathAssets cargo test --test colors     # one suite
GAME1_ASSETS=/PathAssets/Dati cargo test --test game1
```

* `DMA_ASSETS` — folder with the `.MMP/.MMA/.MMS/.MMF` files. Defaults to
  `/RawAssets`. If the folder is missing the asset-dependent tests **skip and
  pass**; they never fail for a missing asset folder.
* `GAME1_ASSETS` — the same for the first game's `.PIC/.SND/.BLB/.mms`
  containers, which live in a `Dati/` folder of their own (`docs/GAME1.md` §1
  has the URLs). Defaults to `/PathAssets/Dati`. It is consulted before
  `DMA_ASSETS`, because the first game's tests are version-1 specific.
* `MMF_PNG_DIR` — where the colour corpus scan looks for extracted PNGs.
  Defaults to `/workspace/extracted/Pictures`.
* `MMF_TEST_TMP` — scratch directory used by extraction round-trip tests.
* `--` separates cargo's own flags from the test binary's; anything after it is
  a test-name filter, not data.

The suite is green-only. If a test fails, fix the code or fix the test's
expectation — never weaken an assertion to make it pass, and never make a
colour/byte claim that the tests do not check.

## Conventions

* **Always build debug.** `cargo build` / `cargo test`, never `--release`.
  Debug builds are the default here: they compile in seconds instead of
  minutes, keep `debug_assert!` and the integer-overflow checks on, and turn
  panics into full backtraces instead of stripped frames — all of which matter
  more than execution speed on a project whose tests move 15,000 resources
  through decoders that were written by reading someone else's 1997 code.
  If a single run genuinely needs the speed (a timed extraction, a
  `--release`-only measurement), say so in the commit message and say why.
  The cost is real and worth knowing: a full debug extraction of the third
  game's pictures plus sounds took 822 s, where release does it in well under
  a minute. Tests are a few minutes either way.
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
* `src/container.rs` is the single parser for every container kind, and holds
  the common header plus the dispatch by **kind** and by **version**. Do not
  fork it per asset type, and do not add a version check inside a decoder: a
  container version whose layout differs is its own module — `src/v1/` is the
  worked example — reached through the two dispatchers in `src/cli.rs`
  (`picture_decoder`, `picture_payload`), which are the only places in the
  program that map a version to code. That is what keeps an edit to one game's
  path off the other's.
* Failures are per-resource, not fatal: warn on stderr, count them in the
  report, keep extracting the rest.
* Surface format constants as named `const`s with the evidence in a doc
  comment (e.g. `PALETTE_BYTES`, `CHUNK_TAG`). Magic numbers inline in a
  decoder are a bug waiting to be filed.
* Rust style: `rustfmt` defaults, no `unwrap()`/`expect()` outside tests
  (use `bail!`/`with_context`), keep warnings at zero for new code.
* Keep the analysis scratch bins in `src/bin/` building (`cargo build` covers
  them). They are allowed to be rough, not broken.

## Before you finish a task

1. `cargo build` — no warnings for new code.
2. `cargo test` — green.
3. If you changed a format: update the relevant `docs/*.md` **and**
   `STATUS.md` (counts, open questions, what is measured vs inferred). Which
   file follows the layering above — a change that affects all three games goes
   in the shared document, and a change to one game's deltas goes in that
   game's file, never both.
4. Commit with a message that says *what changed and how you know* (which
   function, which container, which measurement). Commit early and often.

## Repository layout

Expected content of `/workspace` is described in `docs/WORKSPACE.md` — read it
if a path in these instructions looks missing.
