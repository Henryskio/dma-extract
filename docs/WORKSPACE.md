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
│   │   ├── v1/                  # THE FIRST GAME ONLY (container version 1)
│   │   │   ├── mod.rs lzss.rs metadata.rs picture.rs
│   │   │   └──                  # its own header bases, metadata region, LZSS
│   │   │                        # payload step and palette bank
│   │   ├── picture.rs sound.rs script.rs film.rs # one decoder per asset type
│   │   ├── lz77.rs             # the game's LZ77 variant (round-trips)
│   │   └── bin/                # analysis helpers: imgstat, imgcheck, grid,
│   │                           # chan, cols, *probe, dump, v1pic — keep building
│   └── tests/                   # roundtrip.rs, colors.rs, sounds.rs,
│                               # scripts.rs, film_roundtrip.rs, game1.rs,
│                               # game1_payloads.rs, dma3d.rs
│
├── docs/                        # the reverse-engineering write-ups
│   ├── PICTURES.md              # .MMP (versions 2/3): header, metadata, pixels
│   ├── SOUNDS.md                # .MMA (versions 2/3): metadata, DPCM codec
│   ├── SCRIPTS.md               # .MMS (versions 2/3): serialized object database
│   ├── FILMS.md                 # .MMF (versions 2/3): inner .flm chunk stream
│   ├── SEQUEL.md                # the SECOND game (version 2): its deltas only
│   ├── GAME1.md                 # the FIRST game (version 1): its deltas only
│   ├── DMA3D.md                 # the 3D game: Models + Textures kinds
│   └── WORKSPACE.md             # this file
│
├── STATUS.md                    # resume notes: counts, tests, known gaps
├── FINDINGS.md                  # earlier working notes, kept as history
│
├── iso_range.py                 # list/carve files from a REMOTE ISO over HTTP range
├── extract_iso.py               # copy files out of a LOCAL ISO9660 image
├── oddparse_HD.txt              # negative decoder matrix for the undecoded pictures
│
├── RawAssets/                   # READ-ONLY game input, the THIRD game's (version 3)
│   ├── app.MMP HD.MMP stmp1.MMP stmp2.MMP stmp3.MMP   # pictures
│   ├── HD.MMA app.MMA           # sounds
│   ├── HD.MMS                   # scripts
│   ├── DMA.MMF                  # films
│   └── DMA4.exe                 # the third game's executable (the format authority)
│
├── PathAssets/                  # READ-ONLY game input, the FIRST game's (version 1)
│   └── Dati/                    # HD.PIC mpApp.PIC pack.PIC Topolino.CPK,
│                                # HD.SND mpApp.SND pack.SND, HD.mms pack.mms,
│                                # pack.BLB — see docs/GAME1.md §1
│
└── extracted/                   # extraction output, gitignored
    ├── Pictures/<container>/<index>_<name>.png
    ├── Raw/<container>/…        # untouched payloads (with --raw)
    ├── Sounds/<container>/<index>_<name>.flac
    ├── Scripts/HD/              # container.json, types.txt, strings.txt, objects/
    └── Films/DMA/00000_Intro/   # frame_NNN.png + manifest.json
```

## Documentation layout

The docs mirror the code's layering, and for the same reason: the second and
third games share a container version, so they share one code chain *and* one
doc chain; the first game differs throughout, so it gets its own. An edit
inside one game's chain cannot silently change the other's, which is the whole
point — see `AGENTS.md` for the rule in its most direct form.

| covers | container version | documents | code |
|---|---|---|---|
| second **and** third games | 2 and 3 | `PICTURES.md`, `SOUNDS.md`, `SCRIPTS.md`, `FILMS.md` | `src/{container,picture,sound,script,film,lz77}.rs` |
| second game | 2 | `SEQUEL.md` | the same shared chain |
| first game | 1 | `GAME1.md` | `src/v1/` |
| the 3D game | 3, plus two kinds of its own | `DMA3D.md` | `src/dma3d.rs` |

Three rules keep the chains separate:

1. **A shared document describes no version-1 variant.** Versions 2 and 3 parse
   identically, so a shared decoder has no version branch to document; if a
   shared document ever needs one, that is a signal the difference belongs in a
   game file instead.
2. **A game's own document states only its deltas.** It says what that version
   changes and *points at* the shared document rather than restating it —
   `GAME1.md` §4.2 defers the run-length sub-format to `PICTURES.md` §7.3, and
   `SEQUEL.md` defers the picture formats to `PICTURES.md`.
3. **One place dispatches.** `src/container.rs` holds the common header and the
   dispatch by kind and version; `src/cli.rs`'s two `picture_*` functions are
   the only places that map a version to a decoder. The documents map the same
   way: a format change is written down in the chain it affects, never in two.

   The one shape a shared decoder may still make on `container.version` is
   **refusing a version it has never measured** — `src/film.rs` rejects version
   1 in `parse_film_metadata` because no Films container ships with the first
   game. It must never *choose* a layout for that version: applying version 3's
   film layout to a version-1 container would be a guess, and `AGENTS.md` makes
   a guess a failure. So a version check in a shared file is a guard, and a
   guard has to say in its own comment which versions it does cover.

`STATUS.md` is the exception by design — it is the resume notes, so it keeps a
section per game. Its §10 is the first game's, and it says so in its heading.

## Which of these are in git

**Versioned:** `mmf-extract/` (code, tests, `Cargo.toml`/`Cargo.lock`,
`.gitignore`), `docs/`, `STATUS.md`, `FINDINGS.md`, `iso_range.py`,
`extract_iso.py`, `oddparse_HD.txt`, `LICENSE`, `AGENTS.md`.

**Ignored by design:** `extracted/` (hundreds of MB of output — regenerable),
`mmf-*/target/` (build output), and the scratch items listed below.
`RawAssets/`, `PathAssets/` and `PathAssets2/` are read-only game input and are
never copied into the repository — that rule is why `extracted/` is gitignored.

## Local-only and scratch paths

These exist in a development workspace and are **not** part of the deliverable:

| Path | What it is |
|---|---|
| `PathAssets/Dati/` | the first game's ten containers, carved from `TOPOLINO.ISO` (`docs/GAME1.md` §1) |
| `sequel/`, `PathAssets2/` | the second game's disc image and the files carved from it (`docs/SEQUEL.md`) |
| `mmex-sound/`, `mmex-script/`, `mmex-film/` | throwaway per-asset working copies used while the formats were being reverse-engineered; their results are merged into `mmf-extract/` |
| `.tmp-channeltest/`, `check_hidden_files` | scratch output from earlier debugging |
| `.claude/`, `.git/`, `.gitignore` | agent/tool state and version control |
| `/tmp/…` | temp dirs (`MMF_TEST_TMP` test output, extraction round-trip output) |

Any of the scratch entries can be deleted without affecting the build, the
tests (they skip when assets are absent) or the documentation — the carve
tables in `docs/GAME1.md` §1 and `docs/SEQUEL.md` rebuild them from the disc
images. `RawAssets/` must stay intact: nothing else in this repository can
regenerate it.

## Where each game's assets go

Each game's assets live outside the repository, in its own folder.
`RawAssets/` is the **third** game's (`DMA4.exe`, container version 3) and is
what `DMA_ASSETS` defaults to. The other two have to be carved from their own
disc images, because neither ships inside `RawAssets/`.

### The first game (version 1)

`docs/GAME1.md` §1 lists the three public image URLs and the byte sizes of all
ten containers. Carve with range requests, which fetches only the sectors a
file occupies rather than the whole 574 MB image:

```sh
python3 iso_range.py save <url> /PathAssets/Dati/pack.BLB 29715 73074150
# ...one `save` per container; the LBAs are tabulated in docs/GAME1.md §1
```

Two cautions, both measured:

* `iso_range.py list` prints `# 0 files` on these images. The disc carries a
  Joliet volume descriptor, and the tool's directory walker rejects UCS-2
  names, so the ISO9660 tree has to be read by hand — the LBAs and sizes in
  `docs/GAME1.md` §1 are that hand-read table. `save` needs only those two
  numbers, so it is unaffected.
* `disney-magico-artista-pc-italy/Topolino.bin` is a raw `MODE1/2352` image,
  which `iso_range.py` cannot parse at all. Use the `TOPOLINO.ISO` uploads.

`/PathAssets/Dati` is the first game's default, so the version-1 tests find it
with no environment variable set; `GAME1_ASSETS` only has to be set to point
somewhere else.

### The second game (version 2)

Extract it into `/PathAssets2` and point the CLI at it:

```sh
python3 extract_iso.py copy /path/TOPOLINO2.ISO /PathAssets2 '<regexes as in docs/SEQUEL.md>'
./target/debug/mmf-extract /PathAssets2 -o out2 --types all
```

`docs/SEQUEL.md` has the same JPEG-friendly Joliet caveat as above: prefer the
`TOPOLINO2.ISO` upload, not a raw `MODE1/2352` image.
