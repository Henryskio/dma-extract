# \*\*\*\*\*\*'s Magic Artist Deluxe / Magico Artista — the documentation

This branch holds **only the project's documentation**. No source code, no tests, no
build files, no binaries. It is the read-only record of what the extractor does, why it
does it that way, what was measured, what was got wrong, and what is still open.

The code lives on the `lean-rewrite` branch (and its `main` ancestor). Everything here
describes that code; nothing here is needed to build it.

## Where to start

| If you want… | read |
| --- | --- |
| **the whole extraction story, current state** | [`STATUS.md`](STATUS.md) |
| **to understand the Lean implementation** | [`docs/LEAN-CORE.md`](docs/LEAN-CORE.md) |
| **to know what changed vs `main`** | [`docs/DIFF-vs-MAIN.md`](docs/DIFF-vs-MAIN.md) |
| **to know what went wrong, so it is not repeated** | [`docs/MISTAKES.md`](docs/MISTAKES.md) |
| **to pick up unfinished work** | [`docs/OPEN-QUESTIONS.md`](docs/OPEN-QUESTIONS.md) |
| **the repo's own working rules** | [`AGENTS.md`](AGENTS.md) |
| **the workspace layout and asset folders** | [`docs/WORKSPACE.md`](docs/WORKSPACE.md) |

## The format chain — one chain per game

The docs are layered the same way the code is, so an edit inside one game's chain
cannot change another's:

| document | covers |
| --- | --- |
| [`docs/PICTURES.md`](docs/PICTURES.md) | the shared picture format (versions 2 and 3) |
| [`docs/SOUNDS.md`](docs/SOUNDS.md) | the shared sound format |
| [`docs/SCRIPTS.md`](docs/SCRIPTS.md) | the shared script format |
| [`docs/FILMS.md`](docs/FILMS.md) | the shared film format |
| [`docs/SEQUEL.md`](docs/SEQUEL.md) | the **second game's deltas only** |
| [`docs/GAME1.md`](docs/GAME1.md) | the **first game's version-1 layout** |
| [`docs/DMA3D.md`](docs/DMA3D.md) | the **third game's 3D half** — containers, textures, models, and §§11.1–11.55's full measurement history |

A game's own file states that game's deltas and points at the shared document rather
than restating it.

## The port's own records

These live where the port put them, so the paths quoted in `docs/LEAN-CORE.md` and
`docs/DIFF-vs-MAIN.md` resolve:

| document | what it is |
| --- | --- |
| [`mmf-extract/lean/README.md`](mmf-extract/lean/README.md) | the Lean core's own README and module map |
| [`mmf-extract/lean/CORRECTIONS.md`](mmf-extract/lean/CORRECTIONS.md) | every place the Lean port chose differently from the Rust code or the docs, with its evidence |
| [`mmf-extract/lean/BINARY-AUDIT.md`](mmf-extract/lean/BINARY-AUDIT.md) | five rules re-derived from the shipped executables with REA/rizin rather than from the source |
| [`mmf-extract/lean/PORT-REST.md`](mmf-extract/lean/PORT-REST.md) | a byte-exact porting specification for the script/film/3D half |
| [`mmf-extract/lean/PORT-SOUND.md`](mmf-extract/lean/PORT-SOUND.md) | a byte-exact porting specification for the sound and v1 half |

`PORT-REST.md` and `PORT-SOUND.md` are **working documents from the port**, written so
the Lean modules could be implemented without reading the Rust. They are not format
documentation and they are not maintained as the code moves; the format facts are in
`docs/`.

## What is done and what is not

Done, as of the branch point:

* **Pictures** — 705 of 705 textures and both games' picture containers extract with
  zero failures.
* **Sounds** — 683 of 683, byte-exact `--original` plus a FLAC round trip per resource.
* **Scripts, films** — extracted, with the open questions recorded per doc.
* **3D models** — 425 of 460 payloads produce modern output (17 mesh glTFs, 213
  drawing-profile glTFs, 195 JSON+text documents); every payload is byte-exact in
  `--original`, and the remaining 35 are reported per resource rather than skipped.

Not done, and why: [`docs/OPEN-QUESTIONS.md`](docs/OPEN-QUESTIONS.md). The two largest
are the 35 unconverted model payloads (blocked on a runtime observation of the game's
own loader) and the 3D container's 130 non-name slots (blocked on the same reader).

## A note on the repo's own history

The root of this repository also carries `FINDINGS.md`, `OBJECTIVES.md`,
`ODD-FINDINGS.md` and `ODD-HYPOTHESES.md`. Those are working notes and hypothesis
ledgers from earlier stages and are **not** on this branch: the format facts they
recorded are in the `docs/` chain, and the mistakes they document are consolidated
(and superseded) in `docs/MISTAKES.md`. They remain available on `main` and
`lean-rewrite` for anyone who needs the raw history.
