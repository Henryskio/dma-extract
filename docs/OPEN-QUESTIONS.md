# Open questions

Every unresolved question this project has, in one place, ordered so that whoever takes
one knows what it is blocked on. Each entry names the measurement that produced it, the
evidence it rests on, and what would settle it.

**Nothing here is a guess about what the answer is.** Where an answer was guessed and
later withdrawn, the withdrawal is in `docs/MISTAKES.md` §1 and the entry here says so.

---

## A. Third game — the 3D half

### A1. The 35 unconverted model payloads (the largest open item)

**State.** 425 of 460 payloads in `DMA3D.MMM` produce modern output: 17 mesh glTFs,
213 drawing-profile glTFs, and 195 JSON+text documents (48 record streams, 9 parameter
tables, 21 name tables, 112 `f32` value streams, 5 index tables). 35 are written
through as `.mod` and reported per resource.

**What the residue is, measured** (`docs/DMA3D.md` §11.53, §11.55):

| group | count | why no reader takes them |
| --- | --- | --- |
| `f32` runs of 8–15 values | 21 | below the **measured** minimum of 16 that all 112 accepted streams meet |
| no `f32` run at all | 10 | — |
| carrying half of the full byte range, no dominant zero parity | 4 | `r203 athCircle.mod` is 89% nonzero where the index reader needs 90%, and its bytes are full-range |

**Why these are not read by relaxing a threshold.** Every threshold in the seven readers
was calibrated against the population that reader accepts, and the 35 sit on the wrong
side of a boundary *that population* draws. Lowering `MIN_STREAM_VALUES` to 8 would
admit payloads whose runs are half the shortest run the corpus ever accepted; lowering
`MIN_STREAM_SPREAD` to 0.05 would admit spreads a tenth of the accepted minimum. Neither
number comes from the payloads; both come from wanting them to fit.

**What would settle it.** A schema from the game's own reader. The one static route left
is the relocation entry that would resolve the payload-consuming dispatch, and §11.46
withdrew the branch that was believed to be it. The route that does not need a
relocation is a PowerPC debugger stopping at the routine and reading the register that
holds the vtable pointer — which this workspace cannot do. **Blocked on: runtime
observation of the game's own loader.**

### A2. The 5 ambiguous textures

Five textures decode only under a palette/index split the payload does not pin:
`bike_frame.TGA`, `bike_seat.TGA`, `createModDefaultTex.TGA`, `orange.TGA`,
`RotateTGA`. They belong to a **260-byte class of 11 payloads, all flat colours**, and
the ones that decode differ measurably from the ones that do not: the decoders have low
index maxima in their tails (1, 1, 1, 58, 74, 75), the non-decoders high ones (74, 109,
175, 242, 255).

**Blocked on:** how a flat-colour texture declares its split. The rule *smallest
admissible split* is what is read now and counted in `ambiguous`; a payload with ≤256
index bytes and one value is indistinguishable from a 1×1 image without a declaration.

### A3. The 3D container's 130 non-name slots

The `Models` container's name table is 520 × 32-byte slots from `0x75a`, of which **390
hold a `.mod` name and 130 hold payload-shape bytes**. `src/container.rs` reads
`32 * n` bytes with `n = 460`, so it reads the first 460 slots.

**What was measured and why the obvious fix was not applied** (§11.47, §11.48): the name
runs are 32 bytes apart in 446 of 467 cases and **not** 32 apart in 20, with gaps of
310, 160, 43, 86, 176 bytes. A 310-byte gap is 9.7 slots. So the region is names
*interleaved* with non-name records and the slot→resource mapping is not sequential.
Assuming sequential names and empty gaps would shift every name from the first gap
onward and **silently rename 460 resources and change every output filename**.

**Blocked on:** the container's own reader, which decodes the interleaving. This is the
same blocker as A1.

### A4. The English 3D disc has never been carved

All 3D measurements are against the **Italian disc** (`.tmp/dma3d/assets`). The English
disc was never carved in this workspace. `docs/FILMS.md` §6 already records that the two
discs' film rects differ, so at least one measurement is disc-dependent.

**Blocked on:** an English disc image. Until then every 3D number carries the disc it
came from.

### A5. The `DMA3D.exe` dispatch routines

`MM2DFilmResourceFile` vtable slot 4 at `DMA3D.exe`'s `0x477940` is **unverified**: the
3D disc ships only a Mac PPC build, and the Windows `DMA3D.exe` is not on it. The
runtime-observed parts (table order, rect field order, the third game's chunk types)
were confirmed byte-for-byte on the shipped container instead.

**Blocked on:** a Windows build of the third game.

---

## B. Shared (games 2 and 3)

### B1. The `0x0008` metadata word in sound payloads

Consumed by the loader at `FUN_004a30fa`'s neighbourhood but not identified. Treated as
opaque metadata. `docs/SOUNDS.md` §7.

**Blocked on:** reading the consumer. All 683 shipped sounds extract without it.

### B2. `C` bits 4–7 and the `C >> 6` secondary-size branch

Not needed for any shipped file: every shipped `C` value has `C >> 6 == 0`. The upper
bits of `E` likewise remain uninterpreted beyond the low-two-bit codec selector.

**Blocked on:** a sound container that uses the branch. None ships.

### B3. The codec has no vendor name

It is a custom Delta/PCM-style table codec; calling it "DPCM" follows its behaviour, not
an official label. The 256-entry `i16` table at `0x532778` is verified against the
executable with **zero** differing indices (`lean/BINARY-AUDIT.md`), so what is verified
is the table and the decoder, not the name.

### B4. The 20-rectangle film table is advisory

Five of the eight per-film tables are preserved verbatim but not interpreted. The rect
table sometimes does not cover every RLE write, so it cannot replace the RLE stream.
The tick unit is treated as milliseconds because 83 ticks ≈ 12 fps; no explicit timebase
field has been found. `docs/FILMS.md` §11.

**Blocked on:** a container that contradicts the millisecond inference, or the loader's
tick consumer.

---

## C. First game (version 1)

### C1. The per-class record size in v1 script payloads

`STATUS.md` records it as unresolved: the loader's eight tables are read, but the
per-class record size is not derived. Records of the other 88 script classes are printed
as raw `u16`/`u32` fields.

**Blocked on:** the version-1 loader's class table, which is in the first game's
executable and has not been audited the way the shared one was.

### C2. The second game's `Hd.mms` (`MMFW 3 Script`, `II`, version 2)

Recorded as unresolved in `STATUS.md`. Needs the same class-table work as C1.

### C3. ~273 MB of the first game's disc is unread

The ISO9660 tree ends at sector 147,066 and the remainder is the Mac side of a Win/Mac
hybrid disc that no directory record references. The Windows side is byte-identical
between the two disc images and `pack.BLB` is 73,074,150 bytes on both.

`iso_range.py list` prints `# 0 files` on these images — the disc carries a Joliet
volume descriptor and the tool's walker rejects UCS-2 names, so the ISO9660 tree was
read by hand. That hand-read table (ready for `iso_range.py save <url> <out> <lba>
<size>`) is **in `docs/GAME1.md`**.

**Blocked on:** nothing technical — the table is recorded and the tool just needs the
Joliet path handled. This is the cheapest open item.

---

## D. Open questions about the work itself

### D1. The 3D modern formats are "recorded, not interpreted"

Six of the seven model readers write JSON+text documents whose `meaning` field says
what is **not** determined. That is honest and it is what `AGENTS.md` requires, but it
means half the 3D output is a measured dump rather than a decoded format. Whether that
counts as "extracted" is a question about the project's goal, not about the bytes, and
it is the reason the goal's second half is recorded as not met at 425 of 460.

### D2. The porting specifications are working documents

`lean/PORT-REST.md` (3,686 lines) and `lean/PORT-SOUND.md` (2,091 lines) were written so
the Lean modules could be implemented without reading the Rust. They are **not** format
documentation and they are not maintained as the code moves; the format facts live in
`docs/`. If the project is split they belong with the porting history.

### D3. `lz77.rs`'s 8 added lines are a documentation finding, not a code change

The `0x4000` base is read by the assembly in two places, which is why a zero offset with
bit 3 clear ends the stream. The Rust code was already correct and is unchanged; only
the prose now spells out what the base is. Worth knowing so that a future diff of
`src/lz77.rs` is not read as a behavioural change.

### D4. The parity harness is the only thing that says the port is correct

`tests/lean_parity.rs` (570 lines) plus `lean/test/Test.lean` (473 lines) are the two
halves' agreement checks. **A split that drops a decoder from Rust must also drop its
half of the parity harness**, or the harness will assert against a decoder that no
longer exists and the suite will fail for a reason unrelated to the format.
