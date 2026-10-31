# \*\*\*\*\*\*'s Magic Artist Deluxe / Magico Artista — MMFW Sound Format

Reverse-engineered notes for the `.MMA` (Sounds) containers of \*\*\*\*\*\*\*'s
Magic Artist Deluxe* (Italian release: \*\*\*\*\*\* Magico Artista, 2002).
Everything below was derived from the shipped files plus the game's own
executable (`DMA4.exe`), and is checked numerically by the extractor and its
tests.

> **Which game this covers.** Container versions 2 and 3 share this format, so
> this document describes both — the codec and the metadata layout below are
> version-independent, which is the rule `src/sound.rs` follows. The first
> game's sounds differ only in *where* the format byte is read: its metadata
> region ends in an `N`-byte array instead of the `attr`/`flags` pairs used
> here. Those deltas and the shipped counts live in **`GAME1.md`** §5.

> **Engine note.** Despite the `MMFW` container signature this is **not** a
> Clickteam Multimedia Fusion 2 application. `DMA4.exe` carries the PDB path
> `C:\Projects\DeluxeRevision\DMA Art Ware\Intermediate\Release\DMAAW.pdb`,
> was linked with MSVC 6 on 2002-06-15, imports only Win32/MFC/QuickTime-era
> libraries, and its loader errors are prefixed `MME_*`
> (`MME_PICTURE_RESOURCE_FILE_BAD_HEADER`, etc.). There is no `Clickteam`,
> `CNC`, or `MMF2` string in the binary. `MMFW` is the game's own container
> signature. The sound codec described here is an in-house, table-driven
> differential codec — not IMA ADPCM, Microsoft ADPCM, or a Clickteam format.

---

## 1. Container header

Both sound containers use the big-endian `"MM"` marker and version 3.

```text
0x00  16 bytes  "MMFW " + tag, NUL padded   (tag = "Sounds")
0x10   2 bytes  endian marker: "MM" (big-endian) or "II" (little-endian)
0x12   2 bytes  u16 version, must be 3
0x14   6 bytes  file id
0x1A   8 bytes  timestamp (f64)
0x22   2 bytes  u16 resource count N
0x24  4*(N+1)  u32 offsets; resource i spans [off[i], off[i+1])
       2 bytes  padding
      32*N      resource names, NUL-padded ASCII
```

Payloads begin at `off[0]`. Unlike picture resources, sound payloads are
**not LZ77-compressed**. Each payload is either stored PCM or the game's
differential stream; the metadata says which, and the stored length vs. the
size table confirms it.

The header parser is `FUN_004c25b7`, the names/timestamp reader is
`FUN_004c2953`, and the per-resource arrays are read by `FUN_004c61dc`.
The sound-file class vtable is at `0x4f3058`; its signature method
`FUN_004c5e93` returns the literal string `"MMFW Sounds"`.

## 2. Sound metadata region

Between the end of the names and the first payload sits **`14 * N + 2`**
bytes. Let `ne` = end of the names table =
`(0x24 + 4*(N+1) + 2) + 32*N`.

| Offset from `ne` | Size | Contents |
| --- | --- | --- |
| `0 * N` | `4*N` | u32 array: Macintosh timestamps (seconds since 1904-01-01) |
| `4 * N` | `2` | u16 word, `0x0008` in both shipped files; purpose unidentified |
| `4 * N + 2` | `N` | audio-format byte array (`C`) |
| `5 * N + 2` | `N` | flags/encoding byte array (`E`) |
| `6 * N + 2` | `4*N` | dimension table; all entries are zero for sounds |
| `10 * N + 2` | `4*N` | decoded size in bytes |
| `14 * N + 2` | — | first payload (`off[0]`) |

This exact layout differs from pictures: picture metadata is `15*N+4` bytes
and has no 2-byte word before its byte arrays. The discrepancy was the source
of an earlier misreading of `C` and `E`; the loader's read order in
`FUN_004c61dc` confirms the 2-byte word and the array positions above.

Verified byte-for-byte:

| Container | N | metadata bytes | `14*N+2` | first payload |
| --- | --- | --- | --- | --- |
| `HD.MMA` | 329 | 4608 | 4608 | `0x406e` |
| `app.MMA` | 354 | 4958 | 4958 | `0x4550` |

The timestamps are plausible asset dates: `HD.MMA` spans 1996-12-17 to
2001-07-12, and `app.MMA` spans 1996-11-12 to 2001-08-14.

### The format byte `C`

`FUN_004c5b32`, `FUN_004c5b46`, `FUN_004c5b5d`, and `FUN_004c5b74` read the
byte from object offset `0x278`:

| Bits | Meaning |
| --- | --- |
| 0–1 | sample rate: `0` = 11025, `1` = 22050, `2` or `3` = 44100 Hz |
| 2 | 16-bit samples |
| 3 | stereo (two channels); clear = mono |
| 4–7 | auxiliary; not needed to reconstruct the PCM |

The rate/bit-depth/channel interpretation is confirmed by
`FUN_004c5dd4(size, rate_code, is_16, is_stereo)`, which divides `size` by 2
for each of the 16-bit and stereo flags and then converts to milliseconds at
one of the three rates. `FUN_004c5e2e` performs the inverse. `FUN_004b20db`
builds a playback-format word from the same bits: rate index in bits 0–2,
16-bit in bit 3, stereo in bit 4.

Observed `C` values:

| Value | Rate | Bits | Channels | Encoding |
| --- | --- | --- | --- | --- |
| `0x01` | 22050 | 8 | 1 | PCM |
| `0x05` | 22050 | 16 | 1 | DPCM |
| `0x06` | 44100 | 16 | 1 | DPCM |
| `0x11` | 22050 | 8 | 1 | PCM |
| `0x15` | 22050 | 16 | 1 | DPCM |
| `0x1d` | 22050 | 16 | 2 | DPCM |
| `0x1e` | 44100 | 16 | 2 | DPCM |

All stored-PCM resources in this game are 22050 Hz mono 8-bit. The
stereo DPCM resources are `0x1d`/`0x1e`; a 30-second stereo file named
`Ball30sec.AIF` decodes to 28.799 seconds only with this interpretation.

### The flags byte `E`

`FUN_004c5c36` reads `E & 3` from object offset `0x284`. The loader
`FUN_004b214c` uses it as the codec selector:

| `E & 3` | Meaning |
| --- | --- |
| 0 | stored PCM; payload passed through unchanged (`E = 0x04` in this game) |
| nonzero | differential DPCM; payload expanded to 16-bit (`E = 0x09` in this game) |

The upper bits are not used by the extraction path. No other `E` values
occur in the shipped sound files.

### Decoded size table

The `4*N` table at `10*N+2` is the decoded byte count:

* stored PCM: `decoded_size == stored_size`;
* DPCM: `decoded_size == 2 * stored_size`.

This holds for every one of the 683 resources and is used as an integrity
check. A mismatch is reported and the resource is skipped rather than
guessed at.

---

## 3. Stored PCM

When `E & 3 == 0`, the payload is the decoded sample stream already. In the
shipped files it is **unsigned 8-bit mono PCM**: one byte per sample,
`0x80` is silence, `0x00` is the most negative and `0xff` the most positive.
There is no `RIFF`, `FORM`, `AIFF`, or `SSND` header; names ending in `.aif`
are historical only.

Example: `HD.MMA` resource 9 (`ISthumbDRAG.aif`, `C=0x11`) starts with the
smooth byte sequence `7f 7c 7b 7a 79 78 75 73 ...`, and resource 207
(`ITISBUB1.aif`) is DPCM instead. The first byte of a stored stream being
`0x7f`/`0x80` is why the earlier attempt to run the picture LZ77 decoder over
sound payloads failed.

## 4. Differential PCM

When `E & 3 != 0`, each stored byte expands to exactly one signed 16-bit
sample. The decoder is `FUN_004c6b80`; the 256-entry `i16` lookup table lives
at virtual address `0x532778` / file offset `0x132778` in `DMA4.exe`.

The decoder is a channel-interleaved predictor with one 16-bit state per
channel, reset to zero at the start of each resource:

```text
prev[0..channels) = 0
channel = 0
for each stored byte b:
    s = table[b]
    if (b & 0x7f) > 0x2f and b != 0xf8:
        s = (s + prev[channel]) as i16      // wrap at 16 bits
    prev[channel] = s
    output s as little-endian i16
    channel = (channel + 1) % channels
```

The `0xf8` code maps to zero and is excluded from the predictor update, so a
run of `0xf8` bytes is exact digital silence. This is why `silence.aif` in
`app.MMA` decodes to 44,100 zero samples (one second of 22050 Hz stereo).

Code classes in the table:

* `0x00`–`0x2f`: absolute positive steps (1 … 30826);
* `0x30`–`0x7f`: positive steps added to the previous sample;
* `0x80`–`0xaf`: absolute negative steps;
* `0xb0`–`0xff` except `0xf8`: negative steps added to the previous sample;
* `0xf8`: zero, silence.

The full table, as extracted from the executable:

```text
    1, 6, 19, 41, 73, 120, 180, 258, 353, 468, 603, 761, 942, 1149, 1381, 1641,
    1929, 2247, 2596, 2977, 3391, 3839, 4323, 4844, 5401, 5998, 6633, 7310, 8028, 8788, 9593, 10441,
    11335, 12276, 13264, 14300, 15385, 16520, 17707, 18945, 20236, 21581, 22981, 24435, 25946, 27515, 29141, 30826,
    2048, 2304, 2560, 2816, 3072, 3328, 3584, 3840, 1024, 1152, 1280, 1408, 1536, 1664, 1792, 1920,
    512, 576, 640, 704, 768, 832, 896, 960, 256, 288, 320, 352, 384, 416, 448, 480,
    128, 144, 160, 176, 192, 208, 224, 240, 64, 72, 80, 88, 96, 104, 112, 120,
    32, 36, 40, 44, 48, 52, 56, 60, 16, 18, 20, 22, 24, 26, 28, 30,
    8, 9, 10, 11, 12, 13, 14, 15, 0, 1, 2, 3, 4, 5, 6, 7,
    -1, -6, -19, -41, -73, -120, -180, -258, -353, -468, -603, -761, -942, -1149, -1381, -1641,
    -1929, -2247, -2596, -2977, -3391, -3839, -4323, -4844, -5401, -5998, -6633, -7310, -8028, -8788, -9593, -10441,
    -11335, -12276, -13264, -14300, -15385, -16520, -17707, -18945, -20236, -21581, -22981, -24435, -25946, -27515, -29141, -30826,
    -2048, -2304, -2560, -2816, -3072, -3328, -3584, -3840, -1024, -1152, -1280, -1408, -1536, -1664, -1792, -1920,
    -512, -576, -640, -704, -768, -832, -896, -960, -256, -288, -320, -352, -384, -416, -448, -480,
    -128, -144, -160, -176, -192, -208, -224, -240, -64, -72, -80, -88, -96, -104, -112, -120,
    -32, -36, -40, -44, -48, -52, -56, -60, -16, -18, -20, -22, -24, -26, -28, -30,
    -8, -9, -10, -11, -12, -13, -14, -15, 0, -1, -2, -3, -4, -5, -6, -7,
```

`FUN_004b2935` constructs the DPCM reader and calls `FUN_004c6b68` with the
channel count. `FUN_004b214c` takes that count from the playback-format word's
stereo bit, i.e. from `C` bit 3. `FUN_004c0651` calls the decoder to produce
interleaved 16-bit output.

The codec is lossless with respect to the game's decoded samples, but it is
**not** a standard audio codec and the DPCM bytes are not a playable file on
their own.

---

## 5. Corpus measurements

Both containers:

| Container | N | metadata | stored PCM | DPCM | decoded bytes | audio duration |
| --- | --- | --- | --- | --- | --- | --- |
| `HD.MMA` | 329 | 4608 | 92 | 237 | 18,132,690 | 449.79 s |
| `app.MMA` | 354 | 4958 | 100 | 254 | 71,702,237 | 953.15 s |
| **total** | **683** | — | **192** | **491** | **89,834,927** | **1402.94 s** |

Rate/channel/bit-depth distribution:

| Format | `HD.MMA` | `app.MMA` | Total |
| --- | --- | --- | --- |
| 22050 Hz, mono, 8-bit PCM | 92 | 100 | 192 |
| 22050 Hz, mono, 16-bit DPCM | 232 | 222 | 454 |
| 44100 Hz, mono, 16-bit DPCM | 5 | 0 | 5 |
| 22050 Hz, stereo, 16-bit DPCM | 0 | 31 | 31 |
| 44100 Hz, stereo, 16-bit DPCM | 0 | 1 | 1 |

The shortest sound is 0.035 s; the longest are the 61.1 s `CreditsMusic.aif`
and a set of 30-second stereo loops whose names and decoded durations agree.
All 683 resources decode with zero failures.

## 6. Output

Modern mode writes every sound as a **FLAC** file:

* 8-bit stored PCM keeps its unsigned 8-bit width (`bits_per_raw_sample=8`);
* DPCM and any stored 16-bit PCM are signed little-endian 16-bit FLAC;
* stereo samples are interleaved exactly as the decoder emits them.

The sample rate, channel count and bit depth come from `C`, except where the
version-1 payload overrides the depth (§5.2) — and the depth that was settled
on is what the finished FLAC file reports about itself.

### 6.1 The encoder is the `flac` tool, and it is a dependency

`src/sound.rs` shells out to the official `flac` command-line tool
(<https://xiph.org/flac/documentation_tools.html>), installed as a package:
`apt install flac` / `brew install flac`. It is used both to write the output
and to read it back in `tests/sounds.rs`. If it is missing the extraction
fails loudly, once, naming the install command — it never falls back to WAV,
because silently changing the output format is the one thing this program
must not do.

Two alternatives were tried and rejected, both measured:

* **ffmpeg.** Its FLAC encoder accepts `s16` and `s32` only. An 8-bit source
  is silently promoted to 16-bit: feeding an 8-bit WAV to
  `ffmpeg -c:a flac` yields a file whose `sample_fmt` is `s16` — the samples
  survive, but the file then *reports* 16-bit audio where the container
  stored 8-bit. That is the one thing this extractor's output is supposed to
  state honestly, so ffmpeg was dropped as the encoder.
* **the Rust `flac` crate (0.5).** It was a dependency until it was found to
  be broken as a *decoder*: on `Ball30sec.AIF` it reads the first 633,600
  frames correctly and then produces garbage to the end — 2,828 samples,
  reading `-3017` where the file holds `10` — while ffmpeg and libsndfile
  agree with each other on the same file. A verifier that is itself wrong is
  worse than no verifier, so the crate was removed rather than kept as a
  false witness. No Rust crate here encodes FLAC at all (`claxon` is a
  decoder; `flac-bound` binds libFLAC and adds a C dependency).

WAV is not gone: `wav_bytes` renders the intermediate `WAVE_FORMAT_PCM` file
that is piped to the encoder's stdin, so the 8-bit unsigned convention is
written once and `hound` remains a dependency.

### 6.2 Measured

| | |
|---|---|
| resources extracted | 683 (329 `HD.MMA` + 354 `app.MMA`), zero failures |
| output size | 60,542,810 bytes FLAC |
| same audio as raw PCM/WAV | 89,834,927 bytes — a 32.6 % saving |
| wall time | 5 s for the two containers |
| round trip | all 683 files decode back byte-identical to the decoder's PCM |

The round trip is checked two ways. The samples come back through
`flac --decode --force-raw-format --endian=little --sign=…` (`unsigned` for
8-bit, `signed` otherwise) and are compared with the decoder's own buffer.
Separately, the `STREAMINFO` block is parsed straight out of each file and its
sample rate, channel count, bit depth and sample count are compared with the
decoder's — that is the check that would have caught ffmpeg, whose promoted
8-bit files round-trip perfectly and are only distinguishable by what they
claim about themselves.

`--original` writes the decoded payload verbatim to a `.pcm` file. For
stored-PCM resources that file is byte-identical to the container payload; for
DPCM it is the signed 16-bit little-endian sample stream produced by the
algorithm above. `.pcm` carries no header, so the accompanying rate/channel
information is the `C` byte in the container metadata (and is preserved in
the modern FLAC path).

Output names are `NNNNN_<name>.flac` / `.pcm`, with a trailing `.aif`,
`.aiff`, or `.wav` removed from the resource name. Failed resources are
counted in the extraction report, warned about on stderr, and skipped.

## 7. Verification and open questions

* `cargo test --release` covers the `14*N+2` layout, array positions, both
  codecs, the silence stream, predictor resets, byte-exact `--original`
  output, and — for every one of the 683 resources — a FLAC round trip plus
  the `STREAMINFO` rate/channel/depth/count check.
* A full `--types sounds` run produces 329 + 354 = 683 FLAC files, matching
  the resource counts and with no failures.
* `Ball30sec.AIF` decodes to 28.799 s at 22050 Hz stereo, confirming the
  rate-code and channel-bit interpretation; `silence.aif` decodes to exactly
  one second of interleaved zero samples.
* The `0x0008` word before the `C` array is consumed by the loader but its
  meaning is not identified. It is treated as opaque metadata.
* `C` bits 4–7 (and the `C >> 6` secondary-size branch) are not needed for
  these files; all shipped `C` values have `C >> 6 == 0`. The upper bits of
  `E` likewise remain uninterpreted beyond the low-two-bit codec selector.
* The codec has no vendor name in the executable. It is a custom
  Delta/PCM-style table codec; calling it "DPCM" here follows its behavior,
  not an official label.
