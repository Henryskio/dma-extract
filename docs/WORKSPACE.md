# What `/workspace` should contain

The project is developed inside a `/workspace` directory. This page is the
layout contract: what below is part of the deliverable, what is local-only
input, and what is scratch that may be deleted by anyone. If a path in
`AGENTS.md`, the tests or the documentation looks missing, this file explains
where it is supposed to live.

```text
/workspace/
├── LICENSE                      # AGPL v3 — covers the code AND the docs
├── AGENTS.md                    # instructions for coding agents (agents.md format)
├── README.md                     # short human front door; mmf-extract/README.md is the main one
│
├── mmf-extract/                 # THE DELIVERABLE: the Rust extractor
│   ├── README.md                # what it extracts, usage, notes
│   ├── Cargo.toml / Cargo.lock  # dependencies are pinned
│   ├── .gitignore               # ignores /target
│   ├── src/                     # library + CLI
│   │   ├── lib.rs container.rs cli.rs main.rs   # shared parser and driver
│   │   ├── picture.rs sound.rs script.rs film.rs # one decoder per asset type
│   │   ├── lz77.rs             # the game's LZ77 variant (round-trips)
│   │   └── bin/                # analysis helpers: imgstat, imgcheck, grid,
│   │                           # chan, cols, *probe, dump, … — keep them building
│   └── tests/                   # roundtrip.rs, colors.rs, sounds.rs,
│                               # scripts.rs, film_roundtrip.rs
│
├── docs/                        # the reverse-engineering write-ups
│   ├── PICTURES.md              # .MMP: header, metadata, pixel/palette byte order
│   ├── SOUNDS.md                # .MMA: metadata layout, format byte, DPCM codec
│   ├── SCRIPTS.md               # .MMS: the serialized object database
│   ├── FILMS.md                 # .MMF: inner .flm chunk stream and timelines
│   ├── SEQUEL.md                # the second game: version 2 + film differences
│   └── WORKSPACE.md             # this file
│
├── STATUS.md                    # resume notes: counts, tests, known gaps
├── FINDINGS.md                  # earlier working notes, kept as history
│
├── iso_range.py                 # list/carve files from a REMOTE ISO over HTTP range
├── extract_iso.py               # copy files out of a LOCAL ISO9660 image
├── oddparse_HD.txt              # negative decoder matrix for the undecoded pictures
│
├── RawAssets/                   # READ-ONLY game input, never modified
│   ├── app.MMP HD.MMP stmp1.MMP stmp2.MMP stmp3.MMP   # pictures (version 3)
│   ├── HD.MMA app.MMA           # sounds
│   ├── HD.MMS                   # scripts
│   ├── DMA.MMF                  # films
│   └── DMA4.exe                 # the third game's executable (the format authority)
│
└── extracted/                   # extraction output, gitignored
    ├── Pictures/<container>/<index>_<name>.png
    ├── Raw/<container>/…        # untouched payloads (with --raw)
    ├── Sounds/<container>/<index>_<name>.wav
    ├── Scripts/HD/              # container.json, types.txt, strings.txt, objects/
    └── Films/DMA/00000_Intro/   # frame_NNN.png + manifest.json
```

## Which of these are in git

**Versioned:** `mmf-extract/` (code, tests, `Cargo.toml`/`Cargo.lock`,
`.gitignore`), `docs/`, `STATUS.md`, `FINDINGS.md`, `iso_range.py`,
`extract_iso.py`, `oddparse_HD.txt`, `LICENSE`, `AGENTS.md`.

**Ignored by design:** `extracted/` (hundreds of MB of output — regenerable),
`mmf-*/target/` (build output), and the scratch items listed below. Nothing
under `RawAssets/` is ever copied into the repository.

## Local-only and scratch paths

These exist in a development workspace and are **not** part of the deliverable:

| Path | What it is |
|---|---|
| `sequel/` | `TOPOLINO2.ISO`, the second game's disc image, plus the files carved from it (`sequel/files/`) |
| `mmex-sound/`, `mmex-script/`, `mmex-film/` | throwaway per-asset working copies used while the formats were being reverse-engineered; their results are merged into `mmf-extract/` |
| `.tmp-channeltest/`, `check_hidden_files` | scratch output from earlier debugging |
| `.claude/`, `.git/`, `.gitignore` | agent/tool state and version control |
| `/tmp/…` | temp dirs (`MMF_TEST_TMP` test output, `sequel-extracted/` output) |

Any of the scratch entries can be deleted without affecting the build, the
tests (they skip when assets are absent) or the documentation. `RawAssets/`
must stay intact — everything else is reconstructed from it.

## Where the second game's assets go

The second game is **not** inside `RawAssets/` (that folder is the third
game's). Extract it somewhere outside the repository and point the CLI at it:

```sh
python3 extract_iso.py copy /path/TOPOLINO2.ISO /PathAssets2 '<regexes as in docs/SEQUEL.md>'
./target/release/mmf-extract /PathAssets2 -o out2 --types all
```
