# Disney Creazioni in 3D / Disney's Magic Artist 3D — the 3D game

The third game *in the 1/2/4 numbering used across these docs* is **not** the
3D one: that is DMA4. The 3D game is a separate build that reuses the same
`MMFW` container family. Its Italian disc is *Disney Creazioni in 3D* (2000);
the English build is *Disney's Magic Artist 3D* (2000), and both were carved
from archive.org and measured for this document.

`AGENTS.md` keeps **DMA4 as "the third game"** by convention; this game is
referred to here as **DMA3D** and is not part of that numbering.

## 1. Where the files are

Both discs are Burst installers, so the game's own files live under
`Setup/DMA3D/PC/` (the Italian disc also carries a `Mac/` build of the same
containers). The Italian image is a raw MODE1/2352 BIN/CUE pair and has to be
converted to a plain ISO first; the English one is already an ISO.

| Container | Italian size | English size | Tag | version | N |
|---|---|---|---|---|---|
| `DMA3D.MMA` | 13,392,286 | 13,392,286 | `Sounds` | 3 | 145 |
| `DMA3D.MMF` | 9,381,924 | 10,749,164 | `Films` | 3 | 3 |
| `DMA3D.MMM` | 4,139,055 | 4,139,055 | **`Models`** | 3 | 460 |
| `DMA3D.MMP` | 1,443,271 | 1,443,271 | `Pictures` | 3 | 3 |
| `DMA3D.MMX` | 4,019,622 | 4,019,758 | **`Textures`** | 3 | 705 |
| `Hd.mms` | 49,668 | 49,659 | `Scripts` | 3 | — |

`DMA3D.exe` is present on both discs (1,662,976 bytes). The two builds are
different binaries — 1,329,188 of 1,662,976 bytes differ — so an address in
one does not carry to the other, though the formats they read are the same.

**What already works.** `Pictures`, `Sounds` and `Scripts` are read by the
shared version-3 code path unchanged: a full extraction of either disc gives
**3 pictures, 145 sounds and 647 script objects with zero failures**, which
is the strongest evidence available that this game really is container
version 3 and not merely a similar signature.

## 2. The container is `MMFW`, and the skeleton is shared

Every container starts with the 16-byte `MMFW <tag>` block, then `MM`/`II` at
`0x10`, the version at `0x12`, a 6-byte file id, a 4-byte timestamp at
`0x1A`, `N` at `0x22`, a `4*(N+1)` offset table at `0x24` and 32-byte names.
That is `docs/PICTURES.md` §1, unchanged. Only the **metadata region**
between the names table and the first payload differs per kind, and this game
adds two kinds.

### 2.1 The two region sizes, measured against two discs

| kind | region length | Italian disc (this audit) | English disc (earlier notes) |
|---|---|---|---|
| `Models` | `4*N + 100` | `DMA3D.MMM`, N=460: 1,940 bytes — matches | `4*N + 100` |
| `Textures` | `6*N + 2` | `DMA3D.MMX`, N=705: 4,232 bytes — matches | was recorded as `32*N + 4*N + 2*N + 2` (26,792) — **wrong** |

Both are the same measurement: `names_end` to `offsets[0]`, with the offsets
read as big-endian `u32` from `0x24`, on the shipped files. `src/dma3d.rs`
(`models_meta_len`, `textures_meta_len`) and `MmfExtract/Container.lean` both
carry `4*N + 100` and `6*N + 2`, and those are the two figures the shipped
containers confirm: 1,940 vs `4*460 + 100`, 4,232 vs `6*705 + 2`.

**The `Textures` row's `names_end` is where the Italian disc disagrees with
itself**, and that disagreement is a separate finding from the region length:
read as `N = 705`, `names_end = 2862` and `offsets[0] - names_end` is `6*705 + 2`
exactly — the region length holds — but the names themselves sit at **3166**, 304
bytes later, which is what the 782-entry reading of the offset table predicts.
Which reading the engine uses is not established (§11.2); the region length is
not in question, only where the table ends and the names begin.

The `Textures` row's history is worth writing down, because it is the reason
this section exists. An earlier note recorded the region as `32*N + 4*N + 2*N + 2`
= `38*N + 2` (26,792 bytes for N=705), which is 6.3 times the measured 4,232;
the region between the names table and the first payload cannot be 26,792 bytes
when the whole container is 26,792 bytes past the tables. The `6*N + 2` figure
was right all along, and `lean/BINARY-AUDIT.md` §5 measures it on the shipped
container.

The 100-byte `Models` trailer is **not** a per-resource array, and its content
is disc-specific — the two measurements we have disagree, so both are recorded
rather than one being asserted:

```text
Italian disc, DMA3D.MMM (audit §5):
names        32-byte-padded "shrBtQuit.mod", "shrDgAnotherTextbox.mod",
             "shrDgParentalBtCheckboxN.mod", a truncated "shrDgPA9"
+0xe2ee      a 2-byte marker
             then a 16-byte GUID {780F7A49-3ED3-6511-9D9B-2D366EFAB8}

English disc, DMA3D.MMM (earlier notes):
+0   u16 2
+2   u16 1
+4   u32 0
+8   u32 the texture container's timestamp
+12 30 bytes  "DMA3D.MMX", NUL padded
+42 58 bytes  0xff padding
```

Either way it is the models' record of where their materials come from, measured
rather than decoded, and it is read verbatim either way: nothing in the extractor
parses it, so a wrong description cannot change a byte of output.

The `Textures` region's internals are also read verbatim, and their description
is corrected the same way: the first `4*N` bytes are 32-byte-padded `.TGA` names
(`dialogBtOk.2.TGA`, `dog.TGA`, `tQuit.1..4.TGA`, …), not Mac-style timestamps,
and the tail is a column of `0xb5b5*` values (`b5bd8235`, `b5bd823b`,
`b5bd8241`, …) — one per resource, which look like offsets into a shared texture
atlas. The "two `N`-byte arrays read as `attr` and `flags`" reading was a
hypothesis the payload decoder never confirmed, and the bytes contradict it, so
it is dropped rather than kept.
## 3. The `.mod` payload — the mesh

`src/dma3d/modfile.rs` reads the mesh out of a model payload.

Each payload is one self-contained `mod II` stream: verified on all 460, the
magic appears exactly once, at offset 0. After it comes `u32 16`, then a header
whose word values are small and mostly distinct, then the mesh.

The payload is a sequence of **counted record arrays** — the shape the loader
at `0x4b2b60` already told us to expect, since it walks counted lists after
checking the leading `u16`:

```text
u32   C                 index count
u16[C]                  index list, zero-based, values < V
u32   V                 vertex count
vertex[V]               16 bytes: x, y, z (f32) + w (f32, 1.0)
u32   V                 normal count (== V)
normal[V]               16 bytes: nx, ny, nz (f32) + a fourth f32 (0.0)
...                     u32 0, u32 1, u32 V, then the undecoded tail
```

The chain is present in **144 of the 460 payloads** (`createProp*`, the
buttons, the preview objects). `createModDrawProfile4Point1.mod` carries one
*false* match at `0x35` — `C = 63`, `V = 8` — whose "indices" are other data;
the real chain is at `0x8f` (`C = 18`, `V = 8`). The parser rejects a chain
whose indices do not all name a vertex, which is what makes the false one
unreachable.

Measured, `createPropBucket.mod` (`/tmp/dma3d/pc/DMA3D.MMM`, 7,240 bytes):

| section | offset | size |
|---|---|---|
| header | `0x00` | 163 B |
| `C` (u32) | `0xa3` | 498 |
| `u16[498]` indices | `0xa7` | 996 B |
| `V` (u32) | `0x48b` | 151 |
| `vertex[151]` | `0x48f` | 2,416 B |
| normal count (u32) | `0xdff` | 151 |
| `normal[151]` | `0x0e03` | 2,416 B |
| undecoded tail | `0x1773` | 1,237 B |

The normal array is what identifies the vertex array as geometry rather than a
float table: its `(nx, ny, nz)` triples are **unit vectors** (`|v| == 1.0`),
whereas the vertex coordinates are positional (`|v|` up to ~2.41). The fourth
float is a **record-type marker**, not data: exactly `1.0` in every vertex
record and exactly `0.0` in every normal record, one distinct value each.

### 3.1 The header does not carry the counts

Searched every byte of the header of 42 props for a field equal to `C`, to `V`,
or to the offset at which the mesh starts — as `u32` and as `u16` — and nothing
matches. The header's head is constant (`16, 65537, 24, 1, 1, 2, 0, 1, …`), its
tail is short and variable (the last words are `…, 1, 0, n` with `n` 1–4), and
its length runs 163–263 bytes. So the chain is located **by its structural
signature**: `C` opens `2*C` bytes whose successor `u32` opens `V` records of
`w == 1.0`, then a second `u32` equal to `V` opens unit-length records of
`w == 0.0`. Requiring both halves is what stops a stray `w == 1.0` run from
being read as a vertex array.

### 3.2 The tail — what is measured, and what was wrong

**The tail opens with `V` pairs of little-endian floats.** That is the claim to
survive measurement, and it is the anchor for everything after the mesh.

An earlier version of this document said the tail opens `u32 0`, `u32 1`,
`u32 V` in "all 42" then "all 144" payloads. **That was wrong, and tightening
the parser to require the triple is what caught it**: the requirement dropped
the number of payloads the parser accepts from **144 to 94**, silently
discarding 50 real meshes. The measured distribution is:

| the tail's first three words | count | which payloads |
|---|---|---|
| `0, 1, V` (V their own vertex count) | **94** | the props and the bigger shapes |
| `0, 0, 0` | **49** | the small tool shapes: draw profiles, magnets, knobs, sliders |
| `0, 0, 3` | **1** | `createModMickeySkeleton.mod` |

So the triple is a **measurement, not a rule**. `modfile::parse_mesh` reads the
three words when they are present and records them in `ModelMesh::tail_header`
without demanding them, and `tests/dma3d_numbers.rs` pins the 94 / 49 / 1
split. `src/bin/tailhead` reports the same distribution from the container.

The `V` float pairs that follow are **texture coordinates on some payloads and
something else on others**: all 68 of `createPropGuitar.mod`'s pairs land in
`0..1` (`(0.3530, 0.0248)`, `(0.3532, 0.0250)`, …) and **none** of
`createPropBed.mod`'s 256 do, with the rest scattered between (119/151 on
`createPropBucket.mod`, 7/256 on `createPropRooster.mod`). Which is which is
not measured, so the pairs are not decoded.

`createPropGuitar.mod` ends with a table of ASCII names, 13 bytes apart:

```text
0x6f6c  "key06"
0x6f8b  "key01"
0x6f98  "key02"
0x6fa5  "key03"
0x6fb2  "key04"
0x6fbf  "key05"
```

so the tracks are called `key01`…`key06` and the table lives at the **end** of
the payload. Immediately after the block that follows the tail header,
`createPropGuitar.mod` has `u32 3, u32 3` — a track count of 3 — and then
keyframe data whose first keyframe is `(0.0, 4.6777)`. The keyframe streams
step by `1/30` s and `0.1333` s (30 fps and 7.5 fps), with the sampled value
oscillating around a centre, which is what `MM3DIndexedMeshDeformer` and
`DMAFindDeformersFromMeshVisitor` would drive.

A second, separate table is **fully measured**: `[u32 count][count * 32-byte
NUL-padded names]`, which is how six payloads name their textures
(`createBtDraw.mod` holds 31 of them, `createBtDraw.00.TGA` …
`createBtDraw.30.TGA`). No payload that carries a mesh also names a texture,
so the two lists do not overlap and the material link is not measured.

### 3.3 The animation class vocabulary — measured from `DMA3D.exe`

`DMA3D.exe` carries 129 `MM3D*` class names in its RTTI, and they name the
grammar the payload records belong to. The animation family is:

```text
MM3DAnimation                        MM3DAnimationPlayer
MM3DFunctionCurveAnimationBase       MM3DFunctionCurve
MM3DFunctionCurveWithInfinity       MM3DJointAnimation
MM3DSRTAnimation                     MM3DShapeIndexAnimation
MM3DTexturePlacementAnimation        MM3DBlendShapeWeightAnimation
MM3DLinearFunctionCurve              MM3DFixedIntervalAngleFunctionCurve
MM3DIntervalCountAngleFunctionCurve  MM3DBezierSpline
MM3DLinearSpline                     MM3DNurbSpline
MM3DAggregateFunctionCurve           MM3DIntegralAggregateFunctionCurve
MM3DIntegral{Linear,FixedIntervalAngle}FunctionCurve
MM3DMaterial{Animation,Color,Lambert,Phong,SpecularColor}AnimationBase
MM3DMaterial{Ambient,Diffuse,Specular,Incandescence}{Blue,Green,Red}Animation
MM3DMaterialDiffuseCoefficientAnimation
MM3DMaterialSpecularPowerAnimation
MM3DMaterialTextureAnimation         MM3DMaterialTransparencyAnimation
MM3DLightAnimationBase               MM3DLight{Red,Green,Blue}Animation
```

This is the QD3D/Quesa object model: `SRTAnimation` is a scale-rotate-translate
track, the `Material*Animation` classes are per-channel colour and coefficient
tracks, and the `FunctionCurve` families are the keyframe interpolators. So
each named track in a payload is an animation object with a curve type, and the
30 fps and 7.5 fps bases are the curve's sample intervals.

**Still not measured:** the record layout — record size, keyframes per record,
and which node each track drives. What was tried and must not be retried,
because the post-mesh block is one array whose meaning varies per file and
`AGENTS.md` forbids guessing a split:

* LZ77 over the texture payloads (fails on 704 of 705) and over the models;
* a `[u16 w][u16 h]` dimension header anywhere in the payload, in either
  endianness, at any offset 0..8, with 1 or 4 bytes per pixel;
* an RLE `[r][g][b][count]` stream over the textures — the pixel totals are not
  `w*h` in any reading;
* a 256-entry RGBA palette at payload offset 0, or in the last 1024 bytes;
* the metadata's `4*N` u32 array as a packed `(w << 16) | h` dimension table;
* requiring the tail's `[0, 1, V]` header, which is present in 94 of 144
  payloads (§3.2).

`src/bin/tailhead` reports the measured tail-header distribution and
`src/bin/modinfo` dumps each tail in hex, u32, f32 and u16 together with a
`--tile` mode that tests whether a `[u32 count][count * r]` chain walks one.

### 3.4 The animation region — what is measured, and exactly where it stops

The animation is **not decoded**, and this section records how far the
measurement goes so the next attempt starts from it rather than repeating it.

**Measured.** Immediately past the `V` float pairs, 21 of the 30 payloads whose
first-past-the-pairs word is a valid count carry this, in this order:

```text
u32 K            length of the index list that follows (177 on
                 createBtCameraCenter.mod, 228 on createPropBed.mod, ...)
u16[K]           vertex indices, every one < V, with repetition
u32 D            the number of DISTINCT indices that list holds (131, 128, ...)
f32[D]           one value per distinct vertex
vec3[D]          a three-float row per distinct vertex
```

That `D` equal to the distinct-index count is not a coincidence: it holds on 21
of the 21 payloads where the first word past the pairs is a plausible count,
and on 9 others it does not. The counts are bounded by `V`, as they must be if
they count vertices.

**What follows repeats.** `createBtCameraCenter.mod` walks two frames of
`4 + 16*D` bytes each (offsets `0x2ccf` and `0x3503`), and a third region starts
at `0x3d37` with `[0, 1, 131]` — the same `[0, 1, N]` shape as the tail header,
with `N` here equal to `D`.

**Where it stops, precisely.** The frame size is not consistent: the first two
frames are `4 + 16*D` apart, while the region at `0x3d37` begins with a
twelve-byte `[0, 1, D]` header. One of those readings must be misaligned by
eight bytes, and the measurement does not say which. A grammar that walked to
the end of the payload would close the question; no candidate did, so the frame
size and the number of frames are **not measured** and no glTF channel is
emitted from them.

**Also ruled out, so it need not be retried:**

* the region as a chain of `[u32 n][n * s]` sections, for any assignment of the
  record size `s` from {2, 4, 8, 12, 16, 24} per section, tried with
  backtracking over all 144 payloads — **no assignment closes any payload**
  (`animsec`). So a flat counted-section chain is not the layout, whatever the
  record sizes are;
* the loader's `[u16 2][u16 N][u16 N][u32 N]` shape, which fits only inside the
  triangle index list where the value `2` is common (`grammar`).

Src bins that produce the numbers here: `animwalk` (walk a candidate grammar and
report whether it closes the payload), `animscan` (find the counted index array
and check that `D` equals the distinct count), `animsec` (the backtracking
section walk), `modinfo --tile`, and `grammar`.

**What the loader itself says**, from `DMA3D.exe` at `0x4b2b60`: it reads a
`u16` that must be `2`, a `u16` count, then two arrays of that same count —
2-byte elements through `0x4e87c0` and 4-byte elements through `0x4e8810` — and
then a third array whose element size is a `u16` field at `this + 0x448`,
multiplied into a byte stride. The loader's shape does **not** appear at a
meaningful offset in any mesh payload (`grammar` finds its only fits inside the
triangle index list, where the value `2` is common), which is the evidence that
the mesh records are read by a different routine.

### 3.5 `createPropUI*` — measured, and it is not a mesh

All 42 of them are **exactly 276 bytes** (minimum, maximum and every value in
between equal), and the mesh chain does not fit any of them: a scan for the
`w == 1.0` 16-byte record run finds none. What they do have:

* the same `mod II` header, whose first three words are `16, 65537, <k>` —
  and `<k>` is `121` in all 42, where the mesh files carry `24`, `44` or
  another small value. So **word[2] is a kind code**, and `121` is the UI kind;
* 40 distinct byte values and a longest non-zero `f32` run of **2** floats
  (`0.0141, 0.9859`), where the mesh files have hundreds;
* a paired name: `createPropUIBed.mod` is the UI record for
  `createPropBed.mod`, which is 20,358 bytes of mesh.

So these are not meshes at all. They look like **references**: a small record
that tells the game "render the object named `<Name>` here", with a couple of
texture coordinates for the quad it is drawn on. That fits the game — each
tool's icon in the UI shows a 3D preview of the prop it places. It is a
hypothesis, not a decode: what the fields mean has not been traced.

**Two counts, both real.** 42 payloads are named `createPropUI*`; **64** are
"UI-named" — the wider class also contains the UI preview shapes such as
`createModDrawProfileUIoct.mod`, which *do* carry meshes. `tests/dma3d_numbers.rs`
asserts both, by their real names.

## 4. The `.MMX` payload — the texture

`src/dma3d/texfile.rs` reads a texture payload. Every one of the 705 is the
same shape:

```text
RGBA[K]     K palette entries, four bytes each
[u8; A]     A one-byte indices, every one of them < K
```

The payload reserves no bytes for a header, a width or a height. A four-byte
magic would make `balloon.TGA`'s 260-byte payload 252 index bytes, and 252 is
not a power of two, while the plain reading gives 256.

### 4.1 How `K` is recovered

`K` is the **smallest** value for which every byte after `4*K` is a valid
index — that is, `1 + the largest index the pixel data uses`. Two independent
checks confirm the result:

* `payload_len - 4*K` lands on a power of two in **all 705** files (64, 256,
  512, 1024, 2048, 4096, 8192, 16384, 32768, 65536);
* **no palette entry is dead.** `K = 1 + max(index)`, so every entry is
  genuinely referenced. A wrong split would leave unused entries, and
  `tests/dma3d_numbers.rs` asserts that none does.

The palette order is straight RGBA in file order: `balloon.TGA` opens
`66 e6 ff ff` = `(102, 230, 255)`, a sky blue; `bike_frame.TGA` opens
`1d b3 00 ff` = `(29, 179, 0)`, a green frame. The 629 payloads whose first
entry is `ff 00 ff 00` are the ones whose first palette colour is the
transparent magenta key.

### 4.2 The width and the height — measured, with the error rate stated

The container carries **no dimension table**; its metadata region is names and
timestamps. So the dimensions come from the pixel data.

`A` is a power of two, so it pins the **area** and nothing else. **678 of the
705 files are perfect squares** (8x8 through 256x256) and are read at that root,
which is certain. The remaining **27 are 1:2 rectangles**, and the two readings
of each are decided by `texfile::decide` from the index bytes alone.

**Two facts make that possible, and both are measured rather than assumed:**

1. **The two readings are not transposes of one another.** Reading the bytes at
   width `W` puts true rows `r` and `r+1` at `r*W` and `(r+1)*W`; reading them at
   `2W` puts them at `2r*2W` and `(2r+1)*2W`. The wrong width *concatenates* two
   true rows into one, squashing and interleaving the picture. (An earlier
   version of this document worried that the readings were transposes, which
   would have made every internal statistic measure anisotropy only. They are
   not: no shipped payload satisfies `data[r*H+c] == data[c*W+r]`.) So
   statistics on the index bytes are decisive.
2. **Every rectangle the engine allocates is 1:2.** All 21 rectangles whose
   orientation could be established independently are 1:2 — 64x32, 32x16, 32x64,
   64x128, 256x128, 128x256 — so the candidate width is that pair rather than
   every power of two dividing the area.

**The rule.** The mean absolute index difference across a vertical step is the
primary — at the correct width that step is one true row, so a smooth image is
smooth; at twice the width it is two true rows, so it is not. Validated on the
678 squares, where it lands on the root **659/678** against **649/678** for the
row-continuity statistic it replaced. Two vetoes sit on top of it, both about
the *shape* of the picture: a reading that is **uniquely one 4-connected
component** of non-background pixels wins (an icon is one object at the correct
width and is sliced at the wrong one), and so does a reading with **zero pixels
cut on the frame border**. `createModMickeyPartHeadS.TGA` is the case that
proved the vetoes must come first rather than the statistic.

**Where it stands.** All 21 independently-measured orientations are reproduced by
this rule, and `tests/dma3d_numbers.rs` pins them by name. Five are open and
none of them is asserted: `beestripes`, `conga`, `striped_ball`, `mouseEarse06A`
and `shrBkRight1`, where the competing evidence is laid out with each. The
strongest single piece of evidence anywhere in this question is
`introAxSplash`, whose **256x128 rendering spells the game title legibly** while
its 128x256 rendering is unreadable.

`createBtFileIo.TGA` is **undecidable in principle**: `K == 1`, a single palette
entry and 2048 identical index bytes, so every statistic is degenerate at every
width and no reading differs from any other. It is flagged
(`Texture::undecidable`) rather than guessed at, and it is the only texture that
is.

**What would decide the five**: the texture loader in `DMA3D.exe`. The path to
it is now narrowed — the `MMFW Textures` class is at vtable `0x539e28` and its
vtable is a growable byte buffer rather than a pixel decoder, the `MM3D*` RTTI
names the pixel classes but no `RTTICompleteObjectLocator` references them, and
the executable imports no OpenGL at all (GDI32 `SetDIBits`/`StretchDIBits` and
MSVFW32 only). The next concrete step is to find the function that fills a
`BITMAPINFOHEADER` — a `movl $0x28` for `biSize` followed by two width/height
stores — before those imports, because that is where the width and the height
must be produced.

`src/bin/texorient.rs` prints every statistic for every rectangle and both
readings of it, validates the two statistics against the 678 squares, and draws
either reading as text.

## 5. What the output is, and how it is verified

`--original` writes every payload byte for byte: 460 `.mod` and 705 `.tex`, on
both discs, byte-exact.

The modern mode decodes:

* every texture becomes a **PNG** — 705 of 705, no failures;
* every payload that carries the mesh chain becomes a **glTF 2.0** document,
  144 of them, with positions, normals and indices, and self-contained
  (base64 geometry, base64 PNG textures) so one file opens in a viewer;
* the 316 payloads that carry no mesh are written through unchanged, so an
  animation record is not lost;
* each glTF is accompanied by a **`.tail`** file holding the bytes the mesh
  decoder does not read, verbatim. That is **590,532 bytes** across the 144,
  and the glTF plus its tail is the split of the stored payload at
  `mesh.tail_offset()`;
* each mesh is also rasterised to a **256×256 preview PNG**
  (`src/dma3d/render.rs`): flat-shaded triangles, one light, a z-buffer, an
  orthographic projection fitted to the mesh's own bounding box, and a
  three-quarter eye so a flat floor is not drawn edge-on. It is a preview, not
  a renderer — it exists so a person who has never opened a 3D program can see
  what an object is;
* one **`index.html`** at the output root: the textures and the objects, each as
  a picture, each linked to its file, with a short how-to-open-a-gltf note. No
  script, no stylesheet, no network, so it opens by being double-clicked from a
  local folder. It is written by `MmfExtract/Dma3d.lean`'s `writeIndex` — the
  only implementation there is — and reached from the Rust driver through the
  `@[export mmf_index]` entry point of `MmfExtract/FFI.lean`, see §8.4.

### 5.1 The verification contract

`tests/dma3d.rs` replaces the old tripwire (the modern path agreeing with
`--original` because it was a passthrough — no longer true) with the contract
every other asset type is held to:

* `--original` is byte-exact for all 1,165 resources of both containers;
* every texture's PNG **decodes back** to the pixels `texfile::parse` holds,
  at the measured width and height;
* every glTF is **read back off disk** with `gltf::read_gltf` and compared
  with `modfile::parse_mesh` — indices, positions and normals, all three;
* each `.tail` is exactly `payload[mesh.tail_offset()..]`, and the undecoded
  total is the measured 590,532 bytes;
* the index page's 993 file references all resolve, and all 144 previews cover
  pixels.

The reader that does the glTF half lives in `gltf.rs` as a minimal JSON parser
rather than a `serde_json` dependency, which is not reachable from this offline
build. Bugs it caught, all invisible to the Rust tests: the index base is
**zero**, not one (the writer subtracted one and turned every index `0` into
`65535`); glTF accessor `type` must be uppercase `SCALAR`/`VEC3`; and
`base64_decode` read a **short** final group with the full group's shifts, so
the last byte or two of every buffer was garbage — three sextets carry 18 bits,
not 24.

## 6. Films — measured, and it was two failures

`DMA3D.MMF` extracts in both modes now. It cost two separate mistakes, both of
which the second and third games had hidden by shipping exactly one film each.

**The metadata is eight parallel tables, not a 26-byte record.** The shared
decoder read the per-film data as records and went eight bytes short on the
first of three films. The loader says otherwise (`DMA3D.exe` at `0x477940`,
class `MM2DFilmResourceFile`): marker 7, then `N` bytes, `N` bytes, `4*N`
big-endian u16, `N` u32, `4*N` bytes, `N` u16, `N` u32, `N` u16, then the
palette count and the 1024-byte palettes. Both readings total `26*N`, which is
why the length solve found 26 and why nothing was suspicious until a container
arrived with more than one film. The layout, the disassembly and the arithmetic
that settles the field order are all in **`FILMS.md`** §2.

The three rects, each confirmed by its own key-frame chunk's declared decoded
size. **There are two measured sets**, because the two discs this game shipped
on are not byte-identical; the widths agree, which is the part that proves the
field order, and the absolute top/left/bottom/right do not:

| film | English disc (earlier notes) | Italian disc (`lean/BINARY-AUDIT.md` §4c) | width | height |
| --- | --- | --- | --- | --- |
| `introAxDisney` | `0, 0, 600, 800` | `0, 0, 480, 640` | 800 | 600 |
| `introAxImageBuilder` | `60, 107, 486, 696` | `0, 27, 441, 616` | 589 | 426 |
| `introAxSplash` | `60, 80, 540, 720` | `0, 0, 480, 640` | 640 | 480 |

`tests/film_3d.rs` pins the English-disc set, because that is the container the
suite runs against here. Both sets give the same widths and the same 589/592
stride detail, which is what the field order rests on.

`introAxImageBuilder`'s width of 589 padded to a 592-byte stride is the
detail that proves the field order: reading the rect as
`{left, top, right, bottom}` gives 636x379 and a buffer 11,148 bytes short.

**Key frames are not unique per film.** The shared decoder demanded exactly
one; this build re-sends a key frame mid-stream — 45, 12 and 1 across its three
films — in the pattern `timestamp, key frame` or `timestamp, delta`. A key
frame after the first now replaces the frame buffer instead of failing.

All three decode, and they are plainly the right pictures: `introAxDisney`'s
Disney Interactive logo at 800x600, `introAxImageBuilder`'s ImageBuilder card
at 589x426, and `introAxSplash`'s "Disney CREAZIONI in 3D" title card at
640x480. 61 + 62 + 901 frames, 16 + 49 + 748 unique, 4.1 s + 4.2 s + 29.7 s.
`tests/film_3d.rs` pins the rects, the geometry, the chunk counts and a golden
hash per film's first and last frame, and decodes every PNG back to the exact
palette-expanded bytes.

No texture or model payload fails to decode in modern mode. The remaining
weaknesses are measured and bounded, not silent: the five open texture
orientations (§4.2), and the animation, which is not decoded at all and is
therefore carried as a `.tail` file rather than as glTF channels.

## 7. Measured vs inferred

**Measured** — the skeleton; the two corrected region sizes of §2.1; the
460/705/145/3/3 resource counts; the byte-exact payload of every one of the
1,165 3D resources on both discs; the `mod II` magic on all 460 payloads; the
mesh chain in 144 of them with zero-based indices and unit-length normals; the
tail's `V` float pairs and the header distribution of §3.2 (94 / 49 / 1); the
`[u32 count][32-byte names]` texture table; the texture palette/index split and
`K = 1 + max(index)` over all 705; the 129-class `MM3D*` RTTI vocabulary of
§3.3; the `DMA3D.MMX` reference inside the models' trailer.

**Measured** — the films (§6): the rect array's position among the eight
per-film tables, each film's rect, and the palette that belongs to each film.
Every one is confirmed against the film's own key-frame chunks.

**Measured with a known error rate** — the orientation of the 27 rectangles
(§4.2): all 21 independently-established orientations are reproduced, five are
open, one is undecidable in principle.

**Inferred / open** — the animation frame size and the number of frames, which is
what stops the record grammar of §3.4 from closing (§3.4); the `2*N` two-byte
field in a `Textures` container
(not identified; §2.1); the animation record grammar and which node each track
drives (§3.2); the material link — no mesh-bearing payload names a texture
(§3.2); the meaning of `createPropUI*`'s fields (§3.5); the film record
layout (§6).

---

## 8. Extracting and converting the models

This game's models and textures are extracted by the Lean core and converted by
the Rust converter, and the split is deliberate:

* **`--original`** (Lean, `lean/src/MmfExtract/Dma3d.lean`) writes every payload
  byte for byte. It is byte-exact by construction: the file is the container's
  own slice, so the check is a comparison, not a trust.
* **`--modern`** (Rust, `src/dma3d.rs`, `extract_models`) turns a payload that
  carries the measured mesh chain of §3 into a glTF, a preview PNG, a `.tail`
  file and an `.anim.txt`; a payload with no chain is written through unchanged
  and counted as a model that is not geometry.

Four things the two halves had to be made to agree on, every one found by
comparing their outputs rather than by reading the code.

**8.1 A corrupt offsets entry used to abort the process.** **76 of the Italian
disc's `DMA3D.MMX` 705 resources** have an offsets entry naming a range the file
does not hold, and they break in two ways: **38 entries run backwards**
(`start > end`) and **74 run past the end of the file**, with **36 doing both**.
The models container `DMA3D.MMM` has none. `Container::payload` used to index
`self.data[s..e]`, which for those is a Rust panic: the whole extraction died
and every other resource in the container with it. It now reports the range and
hands back an empty payload, so the resource fails its own layout check and is
counted as a failure — the rule `AGENTS.md` states, and the same rule
`MmfExtract/Container.lean`'s `Bytes.slice` already enforced.
`tests/dma3d.rs::a_corrupt_offsets_entry_is_reported_not_panicked`
walks both containers and holds both halves to it.

**8.2 The `--original` file name doubled the extension.** `00033_od.mod.mod`
came out of a path spelled `format!("{index:05}_{stem}.mod")` with `stem`
already ending in `.mod`. The file now keeps the name the container gives it,
spelled once, and the writer that had an unused `extension` parameter lost it.

**8.3 The two counts, which compose rather than add.** The Lean core puts *both*
3D kinds into one number: `MmfExtract/Extract.lean` adds what it wrote for the
textures to the same `models` field as what it wrote for the models. So a modern
run reports `463` on the Italian disc, which is the **460 payloads the Rust
converter wrote** plus the **3 textures the Lean core decoded**, and reports
**702 failures** — all textures. The Lean core's own 460 model failures ("not
ported to Lean yet") are dropped, because the converter is the program that ran
and it wrote those payloads; they appeared on stderr but not in the report.
Adding it up: 463 written + 702 failed = 1165, which is exactly the two
containers' 460 + 705.

**8.4 The index page has one implementation, and it is in Lean.** The page used
to exist twice — once in `src/dma3d.rs` (dead by then, nothing called it) and
once in `MmfExtract/Dma3d.lean`. The Rust copy is deleted; the page is written
through the second C entry point,
`MmfExtract/FFI.lean`'s `@[export mmf_index]`, so the Rust driver calls it at
the one moment that is correct: after the converter has added the glTFs.

It is written **only in the modern modes**, and that too is measured. Its
figures are `<img>` tags, so only a file a browser can draw belongs on it; a
first attempt listed every numbered file regardless of extension, and the
`--original` page it produced showed 460 objects and 629 textures and drew not
one of them, because a `.mod` payload is the game's own binary rather than a
picture.
`--original` therefore writes no page, and in the modern mode the page lists the
glTFs and previews that exist: on the Italian disc, 0 objects and 3 textures.

## 9. Verifying the conversion in Lean

`lean/src/MmfExtract/Dma3d/Verify.lean` reads both paths' output off disk and
checks it in a third language, so the check does not share a parser with either
writer:

* **`--original`**: every file must equal the container's own slice, and the
  file count must cover every resource except the ones whose payload is outside
  the container.
* **`--modern`**: the `buffers` section's `uri` must base64-decode, by hand, to
  the `byteLength` the same section declares; the index buffer must hold one
  `u16` per index the accessor counts; every index must name a vertex the
  `POSITION` accessor counts — which is `modfile::parse_mesh`'s own rejection
  rule, so a glTF carrying an index the payload did not have is a failing
  glTF; and a VEC3 vertex is three floats, because the payload's fourth is the
  constant `1.0` the writer drops rather than passes through.

The synthetic container is built by hand from §3 by `lean/synth/mkmodel.sh`, so
the conversion it drives has known geometry — `u32 C`, `u16[C]`, `u32 V`,
`vertex[V]`, `u32 V`, `normal[V]`, with 6 indices naming 4 vertices. The
converter writes a glTF for it, and the Lean check accepts it and rejects both
a tampered index and a tampered byte length — which is what makes it a check
rather than a reader. That run also reports one failure, and it is honest: the
synthetic mesh has every vertex at `z == 1.0`, so its triangles are coplanar
with the eye and the rasteriser covers no pixels, which the converter says
rather than writing a blank preview. The glTF is still written and still
verified, which is the point of the exercise.

Both checks are run by `lean/test/Test.lean`, and `lean/test/run3d.sh` runs the
whole path in order: carve the containers (`test/carve3d.py`), extract with
`--original`, check, build the synthetic container, convert with `--modern`,
check. A check whose prerequisite has not been produced yet reports itself as a
skip rather than a failure, so the script can be run at either point.

```
DMA3D.MMM: 460 files checked, 460 byte-exact
DMA3D.MMM: 0 resources have no file (their payloads are outside the container)
DMA3D.MMX: 629 files checked, 629 byte-exact
DMA3D.MMX: 76 resources have no file (their payloads are outside the container)
the Rust converter's glTF verified in Lean: 6 indices, all naming a vertex
the same check rejects a tampered index
the same check rejects a wrong byte length
all checks passed
```

## 10. What this disc's payloads do and do not carry

The shipped `DMA3D.MMM` on the Italian disc holds **no payload that carries the
measured mesh chain**: 0 of 460. That is a measurement, not a defect of the
converter: the same run reports 463 resources written, every one of them the
payload verbatim, because none of them decodes as a mesh and the converter's
own rule (`modfile::parse_mesh` requires the `mod II` magic and the
`[0,1,V,V]` signature) is what refuses them. The English disc's 144 meshes are
what `docs/DMA3D.md` §3.2 was measured on; the synthetic container of §9 is what
proves the conversion path on this workspace's hardware.

## 11. The Italian disc's `DMA3D.MMX` — the failed textures, and what is
actually established

Modern-mode extraction of the Italian `DMA3D.MMX` reports **654 failures out of
705** against 3 decodes. This section separates what was measured on the shipped
bytes and the game's own executable from what is still open. Where it could not
establish something it says so; the residue of this section is deliberately
short on claims rather than long on them.

**The carve this workspace starts from is truncated.** The real container is
**4,019,622** bytes; what is on disk here is 3,230,042. That is not a guess: it
is measured on the disc image (`Disney Creazioni in 3D (Italy).bin`), where the
`MMFW Textures` header sits at `0x3f474a0` and at `0x669ba40` — the second copy
is byte-identical to the first, and slicing `offsets[780]` of its own table yields
4,019,622 with a monotone table throughout. The two disc copies agree on the
header and on the first `0x40` bytes and on the whole offset table, so this is the
container the game has, not an artefact of the carve. The `3,230,042` figure is
`offsets[705]` of the 782-entry reading, i.e. the cut lands exactly on a table
entry — which is why nothing before it looks wrong.

### 11.1 What the 654 failing resources are

Measured against the *truncated* file the extractor holds, with the resource count
read as `N = 705` from `0x22`:

* **577** `"N index bytes is not a power of two, so no width and height fit"`
* **76** `"range [N, N) outside a N byte buffer"`
* **5** `"payload is all palette"`
* **1** `"no palette size makes the trailing bytes valid indices"`
* 3 decode.

Of the 76 range failures, **38 run backwards** (`start > end`) and **74 run past
the end of the truncated file**, with **36 doing both**. `DMA3D.MMM` has none of
this, so it is specific to the `Textures` kind on this build and not to the
container reader in general.

Two of those counts change once the file is the right length, and this is worth
recording precisely because it is the difference between "the container is broken"
and "the container is fine and the carve is short":

* the 76 out-of-range entries are table entries **502..578** — a single
  contiguous run of 77 entries, of which 76 produce a bad range. Read against the
  full 4,019,622-byte file, entries **502..504 and 506..578** are byte-garbage
  (values such as `0x5f2b0001`, `0x82469a44`, `0xffffffff`), while entry **505**
  and every entry from **579** onward is a clean monotone u32. The garbage is not
  random: it is exactly the band straddled by the 304 bytes between `0x24 + 4*782`
  and the names, i.e. the same 304 bytes described in §11.2, and 77 × 4 = 308.
* the 577 "not a power of two" failures are the extractor's own `K` search
  choosing the *smallest* `K` for which the trailing bytes are valid indices.
  That rule is right for the English build and wrong for this one, as §11.3 shows.

### 11.2 The container layout, and the two ways the metadata region reads

The layout is `MMFW Textures` + `MM` at `0x10`, version `3` at `0x12`, the file id
at `0x14`, the timestamp at `0x1A`, `N` at `0x22` as a big-endian u16, and then,
as `MMGenericResourceFile::ReadData` reads them, a `4*(N+1)` big-endian u32 offset
table at `0x24` followed by the names and then the payloads. The reader's field
arithmetic is measured on the shipped file and matches the doc: `N = 460` on the
models container gives `0x24 + 4*461 = 0x758`, where the first
`createAmbientLight.mod` sits, and `offsets[0] - names_end = 4*460 + 100 = 1940`
exactly.

**On this container the same arithmetic refuses to close, and it refuses in two
different ways, both measured:**

1. **Read as `N = 705`** (the value the header carries): `names_end = 0x24 +
   4*706 + 2 = 2862`. The names do not start there. What is there is a run of
   76 big-endian u32s, each with its top two bytes `0x00 0x31..0x3d` and its low
   two bytes increasing monotonically — `0x00315c2e`, `0x00316f0a`,
   `0x003181ea`, … — which reach `0x003d` + the file's own tail and so are
   *plausible offsets*, not names.
2. **Read as `N = 781`**: `names_end = 0x24 + 4*782 + 2 = 3166`, and at 3166 the
   names **do** start — `CreationSlotTex.TGA`, `CreationTex.TGA`,
   `DuplicateTex.TGA`, `Metal.TGA`, `balloon.TGA`, `beestripes.TGA`,
   `bike_frame.TGA`, … — in 32-byte slots, and `offsets[780]` of that reading is
   the file's real length, 4,019,622.

Both readings are *consistent with the bytes*, and they cannot both be right.
**Which one is right is not established here**, and the reason is worth writing
down, because it is the thing a decoder needs from the executable and does not
get:

* `DMA3D.exe` cannot answer it. It is a CodeWarrior **PEF** (`Joy!peff` /
  `pwpc`, PowerPC 32-bit big-endian). The connected REA provider rejects it
  outright (`open_binary` → `unsupported binary format`), Rizin likewise reports
  `format any` and cannot disassemble it. By parsing the PEF container header
  directly (section 0 at container offset `0x3510`, 1,662,836 bytes) the
  RTTI/class-name area is reachable, and 4286 `mflr r0` prologues confirm the
  body is PowerPC code — but no code in it can be *decoded*, so the reader's
  treatment of a corrupt offset band cannot be read off the instruction stream.
* The two readings differ by 76 u32s = 304 bytes, which is exactly the width of
  the u32 band at `0x24 + 4*782`. A 32-byte name slot at 3166 and a u32 at
  2862 are therefore the *same bytes* under the two readings, so the shipped
  bytes cannot discriminate them either.

**Measured facts that hold under either reading:**

* the real container length is **4,019,622** bytes, and the carve must be redone
  to that length (the two disc copies agree, and `offsets[780]` of the 782-entry
  reading equals it);
* resource **0's** payload starts at 29,654 under either reading, and 29,654 is
  *not* a texture under either (§11.3);
* the payload area from 29,654 onward is covered by a rigid
  **2,352-byte record grid**: 1,708 complete records, the first header at 2336
  and the last one ending at 4,019,552, plus a 70-byte tail to 4,019,622. Every
  record's header is `00` followed by **ten** × `0xff`, then `00 06 XX YY 01` —
  16 header bytes before a 2,336-byte body, `XX` in `0x18`..`0x40` and `YY` a
  per-record counter. **The same grid, at the same
  first offset 2336 and with the same period, appears in `DMA3D.MMM`**, which
  parses correctly — so the grid is not itself the corruption;
* the first three table entries (29,654 / 46,998 / 49,206) and the last
  (4,019,622) agree between the two readings, so the payload map is *partly*
  shared.

**What is not established:** which of `N = 705` or `N = 781` the engine uses here,
whether the header's `N` is simply stale, whether the 77 garbage entries are an
overwritten band or a genuinely damaged table, and the position of the names
between the two readings. Until `DMA3D.exe` can be decoded — a PowerPC PEF
disassembler, not the x86 tools this project has used so far — these stay open.

### 11.3 The payload, and why the extractor's `K` search fails

`src/dma3d/texfile.rs` recovers `K` as the **smallest** value for which every byte
past `4*K` is a valid index, and asserts `payload_len - 4*K` is a power of two.
Both halves of that are wrong for this build, and both wrongnesses are measured.

**Measured, resource by resource, on the 502 resources whose table entries are
clean under either reading:**

* Requiring `len == 4*K + 2^e` *and* `max(payload[4*K..]) < K` fits **497 of
  502** — but it does not uniquely fix `K`. A 17,344-byte payload fits for `K ∈
  {4334, 4332, 4328, 4320, 4304, 4272, 4208, 4080, 3824, 3312, 2288}`, i.e. for
  any `area` from 8 to 8192. The power-of-two constraint alone pins the *area*
  relationship, not the split.
* The **reverse** layout — indices first, palette last, `len == 2^e + 4*K` —
  fits **498 of 502**, four better than the documented order.
* Requiring additionally that **every palette entry is referenced** (the §4.1
  check) leaves only **3** unique readings across all 502 — and those 3 are
  exactly the 3 resources the extractor already decodes (indices 10, 13, 127,
  at 90,358 / 108,822 / 922,642). So the "no dead entry" test is not a property
  of the format on this build; it is an accident of the three payloads that happen
  to be self-consistent.

**Measured, the first bytes of the payload region.** At `offsets[0] = 29,654` the
bytes are `b5 bd 82 4d b5 bd 82 53 b5 bd 82 59 b5 bd 82 5f b5 bd 82 65 b5 bd
82 6b …` — consecutive u32s differing by exactly **6**, with a constant
`b5 bd 82` prefix and a last byte ramping `4d, 53, 59, 5f, 65, 6b, 71, 77`. That
is a **palette in RGBA order with a ramping alpha**, not pixel indices: pixel
indices would not carry a byte that increases by a constant step across whole
u32s while three bytes stay fixed. The same shape repeats at the start of the
payload band (it is visible again at 30,571 and 35,275 inside the record grid).
So **29,654 is not a texture payload** — it is palette-shaped data, and the real
pixel area for resource 0 is somewhere after this ramp.

**What is not established:** where the ramp ends and how long resource 0's
palette is; which of the two layouts (`[palette][indices]` or
`[indices][palette]`) this build uses — the 4-resource difference between 497 and
498 is not enough to decide, and the 3 self-consistent resources do not decide
it either; and how `K` is to be chosen when many splits fit. The palette/indices
*order* documented in §4 is measured on the English build and should be treated as
**unverified for the Italian one**, not as known-wrong.

### 11.4 How to extract the failing textures — what can be said, and what cannot

The honest instruction is two halves.

**Established, and safe to implement now:**

1. **Carve the container to 4,019,622 bytes** from either disc copy
   (`MMFW Textures` at `0x3f474a0` or `0x669ba40` in the Italian image; the two
   are byte-identical over the header and the offset table). Do not carve to
   `offsets[705]`.
2. **Read the header as documented**: `N` at `0x22` big-endian u16, offset table
   at `0x24` as `4*(N+1)` big-endian u32. On the correctly-length file the
   entries 0..501, 505 and 579..781 are monotone and in range; entries
   502..504 and 506..578 are garbage and must be **reported per-resource, not
   fatal** (the rule `AGENTS.md` already states, and the one
   `Container::payload` now follows).
3. **Read big-endian everywhere**: the header's `N`, the offset table's u32s and
   the payload's u32s are all big-endian, as the shared docs already say and as
   the reader in `DMA3D.exe` confirms (`lha r5, 0x28c(r30)` for `N`,
   `lwz r4, 0x2bc(r30)` for a table entry pulled as a 32-bit big-endian word).
4. **Keep the payload-length-is-ground-truth rule.** The 4-resource advantage of
   the reverse layout is real but too small to act on; the documented
   `[palette][indices]` order has not been *disproved* for this build, only left
   unsupported.
5. **Treat the 2,352-byte record grid as structure, not noise**: it is present in
   the models container too and its first record is at 2336 in both. Its 5-byte
   header (`00 06 XX YY 01` after ten × `0xff`) has a byte ranging
   `0x18..0x40` and a fifth byte that is always `1`; what those mean is not
   established, and nothing here relies on them.

**Not established, and not implementable from this section at the time it was
written:**

* which `N` this build uses (705 or 781) and therefore where the names sit;
* the length of resource 0's leading palette ramp, i.e. where its indices begin;
* how `K` is to be disambiguated when many splits fit, and which layout order
  applies;
* how the width and height are derived. §4.2's rule was validated on the English
  build's 678 squares and is not re-measured here; with the split unknown the
  orientation question does not arise yet.

**§11.6 records what closed since.** The first of those is settled by the
container itself (exactly one `N` makes `offsets[N]` the file's length: 781), and
the third is settled as a *default* with the ambiguity recorded rather than
resolved. The second and the fourth are still open.

**The next concrete step**, stated so it is not repeated: obtain a PowerPC PEF
disassembler and decode `MM3DTextureResourceFile::ReadData` (its symbol entry is
in the exe's own CodeWarrior symbol table at `0x13d307`, and the function it calls
through is at `0x174fe4`). The reader is where the `N`-vs-table-length question,
the payload's leading-ramp interpretation and the width/height derivation all
live. Every quantity this section leaves open is a quantity that function
computes.

### 11.5 Measured vs inferred, for this section

**Measured** — the container's real length 4,019,622 and the two identical disc
copies that establish it; the 654/705/3 failure breakdown and the 38/74/36 range
breakdown; the 77-entry garbage band at table entries 502..578 as a contiguous
run that is 76 bad ranges; the names starting at 3166 in 32-byte slots and the
304-byte u32 band at `0x24 + 4*782`; the 2,352-byte record grid (1,708 complete
records, first header at 2336, 16-byte header `00` + ten × `0xff` +
`00 06 XX YY 01`) and its
presence in `DMA3D.MMM` at the same offset and period; 497/502 and 498/502 for the
two layout orders against the length-and-range constraints; only 3 unique splits
under the "no dead entry" test, which are the 3 already-decoding resources;
the ramping-alpha shape at `offsets[0] = 29,654` that makes it not a texture.

**Measured about the executable, as a negative** — `DMA3D.exe` is a PowerPC
CodeWarrior PEF, the connected REA provider rejects it as an unsupported binary
format, Rizin does not parse or disassemble it, and the PEF container header
parses by hand (section 0, kind 4, 1,662,836 bytes at `0x3510`) far enough to see
its RTTI and confirm 4286 `mflr r0` prologues but not to read any instruction.
The symbol table does locate `MM3DTextureResourceFile::ReadData` at `0x13d307` and
`MMResourceFileWithNamedSubfiles::ReadData` at `0x17d4b7`, and those two functions'
argument handling is legible — but their *arithmetic* is not, which is what leaves
§11.2 and §11.3 open.

**Inferred / open** — the palette length at the start of the payload region; the
palette/indices **order** for this build (both fit, four resources apart, so
nothing acts on it); and the width and height derivation.

**Closed since, by measurement rather than by the executable** (§11.6) — which of
the two `N` readings is correct (781: the one that makes `offsets[N]` the file's
own length, which is also the header's own count on the models container that
works); how `K` is to be chosen when many splits fit (the smallest admissible
palette, with the ambiguity recorded on every texture); and the position of the
names under the walked count (3166, where they actually are). What that cost is
also recorded there: the documented metadata region does not hold under the walked
count, so it is left unread with the reason on the record, which is a cost the
texture decode does not pay.

## 11.6 What is implemented as of this section, and what it costs

Everything §11.4 lists as established is implemented, in Lean, with the Rust
reference kept in step so the halves do not drift. The result is measured, not
asserted: **695 of the 781 textures now decode, against 3 before.**

**The count is settled, by the container itself.** §11.2 leaves open which of
`N = 705` and `N = 781` the engine uses. The shipped file answers it without the
executable: exactly **one** `N` in the range the offset table can hold makes
`offsets[N]` the container's own length. It is **781** for `DMA3D.MMX` and **460**
for `DMA3D.MMM`, and on the models container that one value is also the header's
own — so the test is not merely consistent, it is the same arithmetic the working
container already obeys. `offsets[781] = 4,019,622` is followed by a run of `0x03`
padding, which corroborates that the container ends there, and the second disc
copy carries the identical table.

Settling it costs the one thing the two readings could not both have: **the
documented metadata region does not hold under the walked count.** With `N = 781`
the names end at 28,158 and a `6*781 + 2` region would have to reach 32,846, past
the payload base at 29,654. So the names and the metadata cannot both sit between
the table and the payloads, and which of them is shortened is exactly what
§11.2 could not decide. The texture decode uses neither, so `MmfExtract/Container.lean`
reads the names at 3166, leaves the region unread and records the reason on the
container (`metadataReason`) rather than throwing it away.

**How the count is read.** `Container::walk_count` (both halves) is a **repair,
not a preference**: it runs only when the declared count does not describe the
whole container. If the declared count already puts the table's last entry at the
file's end, that is the count, and the walk never runs. Otherwise it walks out
while each next table entry is a plausible payload offset — a step of at least 4
bytes and at most 2 MiB — and stops where the table stops describing payloads,
which is the container's own length.

That condition is what keeps every other container byte-for-byte as it was. It is
not hypothetical: the first game's version-1 `HD.mms` holds 295 resources and the
entry after its table is plausible enough that an unconditional walk read 296 and
broke `tests/game1.rs`. The declared count is kept as `declaredCount` so the
disagreement is reported, not hidden: `781` against `705` for the Italian
`DMA3D.MMX`, `460` against `460` everywhere else.

**The palette rule, and what the ambiguity costs.** §11.3 measures that
`len == 4*K + 2^e` with `max(index) < K` fits 497 of 502 but does not pin `K`:
a 17,344-byte payload fits for `K` from 2288 to 4335. The shipped rule took the
smallest valid `K` *before* asking whether the area left over was a power of two,
so a payload whose smallest valid `K` left a non-power-of-two area was rejected
wholesale — 577 of the 781, as "N index bytes is not a power of two", when the
payload's own length does settle the area.

So the rule is now: **enumerate the admissible splits and take the smallest
palette**, which is the reading that leaves the most bytes as pixels. It is a
*default*, not a measurement, and every texture that decodes records that its
split was not pinned (`Texture.ambiguous`, `Extract3DReport.ambiguous`, and a
`ambiguous:` line in the CLI's summary). On the Italian disc **all 697** are
ambiguous and counted as such. The three resources that *were* self-consistent
under the old "no dead entry" test remain self-consistent; they are not a
different class, they are the three where the default happens to be the only
answer.

Those 10 refusals are worth naming, because they are the rule doing its job rather
than a regression. Eight payloads admit no split at all, and three admit one whose
only width pair is below `widthCandidates` - a payload of 8, 16 or 32 index bytes
splits as 4x2, 8x4 or 8x4: the product is right and neither dimension is a width
this engine allocates, so the reading is reported instead of decoded (`balloon.TGA`
is one, and the message names both numbers). The English disc's textures are 8x8
and larger, so the check is silent there; it is the Italian disc's small payloads
that reach it. The discard rate is the honest price of the fix: 654 failures became
86, and the 86 are split between the damaged offset band and payloads that really
do not describe a texture this engine could draw.

What remains genuinely open is unchanged from §11.4: the palette/indices **order**
(both fit, four resources apart, so nothing here acts on it) and the width/height
derivation, which §4.2 validated on the English build. Everything else §11.4
listed as not implementable is now implemented except those two.

**One bug the new rule exposed, in both halves.** `wideTall` answers a 1:2 pair
whose product is the area only when the exponent is odd. For an even exponent the
product is one power of two too large, and `splitArea` reaches it for a square
whose root is below `widthCandidates` — area 16, root 4. `decide` then read the
pair and indexed past the end of the slice. `splitArea` now returns that pair
unchecked and lets the caller's `width * height != area` rejection say no, which
is the message it always intended to print. A panic is not an acceptable way to
report a texture, and it is what the Italian disc's smallest payload would have
produced.

**The measurements that back the numbers above**, from `lean/test/Test.lean`:

```
DMA3D.MMM:  460 resources (the header declares 460), 4139055 bytes
DMA3D.MMM:  460 files checked, 460 byte-exact, 0 resources with no file
DMA3D.MMX:  781 resources (the header declares 705), 4019622 bytes
DMA3D.MMX:  695 textures decoded (695 of them with an unpinned palette split),
            76 offsets entries outside the container, 10 with no split
```

and the same run's report, end to end:

```
$ mmf-extract /workspace/.tmp/dma3d/assets --output out --types models,textures
  models:   1155
  ambiguous: 695 (palette size not pinned by the payload; the smallest was read)
  total:    1155
  failed:   86
```

`1155` written plus `86` failed is `1241`, which is the two containers' own
460 + 781 — every resource is either written or reported, which is the accounting
the AGENTS.md rule asks for.

Of those 86, **76 are the damaged offsets entries of §11.1** and **10 are payloads
the size rule refuses**: 8 admit no split at all, and 3 admit one whose only width
pair is below `widthCandidates`. Those 3 are worth naming, because they are the
rule doing its job rather than a regression. A payload of 8, 16 or 32 index bytes
splits as 4x2, 8x4 or 8x4: the product is right and neither dimension is a width
this engine allocates, so the reading is reported instead of decoded — `balloon.TGA`
is one, and the message names both numbers. The English disc's textures are 8x8 and
larger, so the check is silent there and it is the Italian disc's small payloads
that reach it. The discard rate is the honest price of the fix: **654 failures
became 86**, and the 86 split between the damaged offset band and payloads that
really do not describe a texture this engine could draw.
## 11.7 The damaged offset band, and how it reads now

§11.1 recorded **76 failures** of the form `range [N, N) outside a N byte buffer`,
from table entries 503..578 whose values are garbage (`0x5f32a7e1`, `0xffffffff`,
`0`). They are reported, not fatal, so the rest of the container extracts — which
is why the count of *failing* resources and the count of *lost* textures have to be
kept distinct. On the Italian disc they were 76 and 76: every one of those
resources was unavailable.

The band is not an offset table. §11.1's reading of the 304 bytes that follow it as
"76 u32s ending in `0x0031`..`0x003d`" was an artefact of a 2-byte misalignment:
those bytes *are* table entries 706..781, and reading a monotone offset table two
bytes late produces exactly the shape described — a small repeating high byte and a
low half that increases. There is no second table.

**What the entries are is not established, and nothing below depends on it.** What
is established is what to *do*, and it is the same rule the payload rule already
stated: the byte counts win. A table entry is a payload boundary only when it is a
plausible payload offset — at or after the first payload, at or before the
container's own length, and strictly past the boundary already accepted, because
the payloads are contiguous and a resource cannot be empty.

Dropping the 76 that fail leaves **706 boundaries**, which is **705 resources** —
*exactly the count the container's own header declares* — spanning 29,654..4,019,622
with no gap, and the resource the damaged run used to swallow is recovered: the
4,468 bytes at 2,577,290 that the old reading could not attribute to anything now
read as a `K = 605`, 64×32 texture. That the surviving count equals the declared one
is the check that this reading is the right one: it is the same arithmetic
`DMA3D.MMM` obeys, where the declared count and the boundaries agree.

The names move with the table and not with the count, which is what previously made
the two readings irreconcilable: the accepted entries run to index 781, so the names
sit at `0x24 + 4*782 + 2` = 3166 — the 782-entry reading — even though only 705 of
those entries are boundaries. Reading them at the payload count instead (2862) gives
garbage for every name.

The cost is recorded rather than hidden: with the names at 3166 and 705 of them, a
`6*705 + 2` region would have to reach 30,090, which is 436 bytes past the payload
base at 29,654 — so the container still records `metadataReason` (§1.2) instead of
inventing the region.

### What this bought, measured

|| before | after |
|---|---|---|
| resources read | 781 (76 of them failures) | **705, none failing** |
| textures decoded | 695 | **696** |
| resources reported | 86 | **9** |
| `--original` files | 1,090 (76 missing) | **1,165, every one byte-exact** |

The 9 that remain are not the damaged band. Measured on the shipped container, **6**
admit no palette/index split at all, and **3** admit one whose only width pair is below
`widthCandidates` and are therefore reported rather than decoded. §11.8 separates them
into the two payload classes they come from and measures both orders of the split
against them. The arithmetic
closes: 696 decoded + 9 reported = 705, which is the container's own declared count.
## 11.8 What the remaining 9 are, and the two measurements that bear on them

The 9 are not all the same problem, and splitting them is what lets each part be
attacked separately.

**Six admit no palette/index split at all** — `bike_frame.TGA`, `bike_seat.TGA`,
`createModDefaultTex.TGA`, `orange.TGA`, `opUIpPlant.TGA`, and one payload whose
name is garbage. **Three admit a split but no allocatable width** — `balloon.TGA`
(8 index bytes), `orange.TGA` (32) and `Rotate.TGA` (16), whose only splits leave
4x2, 8x4 or 2x4.

They belong to two payload classes that the shared English-build measurements never
saw, and the names say what they are:

* **the 260-byte class, 11 payloads**, all flat colours: `balloon`, `bike_frame`,
  `bike_seat`, `createModDefaultTex`, `dark_grey`, `grey`, `mickey_body`, `orange`,
  `purple`, `tan`, `white`. Six of the eleven decode under the documented
  palette-first rule and five do not, and the difference is measurable: the ones
  that decode have low index maxima in their tails (1, 1, 1, 58, 74, 75) and the
  ones that do not have high ones (74, 109, 175, 242, 255). `white`, `tan` and
  `purple` carry a body of 256 bytes using only the values `0` and `1`.
* **the 68-byte class, 2 payloads** (`createBkFloorDefault.TGA`,
  `createBkFloorSolid.TGA`), the smallest in the container.

### The order, measured on both readings

`§11.4` left the palette/indices order open because both fit and the difference was
four resources out of 502. Measured over the whole container now that the boundaries
are right — 705 resources, both readings, engine-width filter applied:

|| | decoded | sub-width | no split | failures |
|---|---|---|---|---|
| palette-first (documented) | **696** | 3 | 6 | 9 |
| indices-first | **699** | 4 | 2 | 6 |

Indices-first decodes three more textures and fails three fewer times. That is a
measurement, not a decision: the documented order is the one the **English** build
decodes 705/705 with (`docs/DMA3D.md` §4, `tests/dma3d_numbers.rs`), and this
workspace holds no English disc carved, so switching would trade a verified
reading for a three-texture gain on a different build. The order therefore stays
open, with the size of the difference now recorded: three resources, not four.

### What is not established

* which of the two orders this build uses, and therefore what the 260-byte and
  68-byte classes are;
* the width/height derivation, which `§4.2` validated on the English build's 678
  squares and which cannot be re-measured while the split is undecided;
* the width of resource 0's leading palette ramp, i.e. where its indices begin
  (`§11.3`).

Nothing in this section should be read as having resolved them. What it does
establish is the shape of the residue: 6 + 3, in two payload classes, against an
order question that the English build answers and this one cannot.
### The order cannot be settled from the shipped bytes, and this is why

The two tables above could leave a reader thinking one order is right and the
evidence merely weak. It is not: over the whole container, **695 resources decode
under both orders**, and

* **642** give the *same* reading under both — same palette size and same
  dimensions, so the order does not matter for them;
* **53** give a *different* reading, with the palette size and the dimensions both
  changing;
* **1** decodes only under the documented palette-first order;
* **4** decode only under indices-first — which is where the whole 699-vs-696
  difference comes from.

So each order uniquely rescues resources the other cannot read, and the resources
where they agree are the vast majority. A reading that discriminates them would
have to come from outside the payload: either the executable's own texture reader
(`docs/DMA3D.md` §11.3 records that `DMA3D.exe` is a PowerPC CodeWarrior PEF and
the binary audit found no disassembler for it), or a build whose textures decode
cleanly under one order and not the other — which is what the English disc's
705/705 does for palette-first, and which is why the documented order is the one
the code keeps.

This is the honest answer to §11.4's question, and it is a negative result rather
than an unfinished one: the measurement says the shipped bytes do not contain the
discriminator, so no further measurement of these payloads can produce it.
## 11.9 The 460 model payloads carry 17 meshes, and the magic was the gate

§11.1 recorded that **0 of the 460** Italian `DMA3D.MMM` payloads carried the
measured mesh chain, against 144 of 460 on the English build, and left it at that.
It was not a property of the disc. It was a gate in the reader.

`modfile::parse_mesh` opened with `if !payload.starts_with(b"mod II")`. The
`mod II` magic **does** appear in this container — 267 times — but never at offset
0: measured offsets into the payload are 3, 24, 35, 44, 50, 55, 79, 86, 90, 92,
… through 30,240, behind a header the format does not decode. And the chain
itself was always searched for structurally — `docs/DMA3D.md` §3 says "the counts
are not in the header, so the chain is located structurally", and the loop below
the gate does exactly that — so the only thing standing between the payloads and
a mesh was the magic check that the structural search had already made redundant.

Removing the gate: **17 of 460 payloads carry the chain**, and the converter now
writes **17 glTFs from shipped data**, against zero before. It is not 144 — this
disc genuinely carries fewer meshes than the English build — but 17 is measured
against the container, not asserted. Resource 9 carries 54 indices naming 36
vertices, so these are objects, not the 6-index/4-vertex sub-structure seven of
the seventeen share.

The payloads are **not** compressed under another layer, which was the other
hypothesis: 459 of 460 fail LZ77 decompression outright, so they are stored raw
exactly as the `--original` path writes them, and their entropy is what a raw mix
of 108-byte headers, float arrays and packed records looks like.

### The index page's object figures, fixed

The page used to write `<a href="…gltf"><img src="…gltf"></a>` for an object: the
same path for both. A `.gltf` is not an image, so **every object rendered as a
broken picture** and the previews the converter wrote beside them were never
shown. `figure` in `lean/src/MmfExtract/Dma3d.lean` now takes a link name and an
image name, and the model loop pairs each glTF with the preview at the same index
from `listDir modelDir "preview.png"`, with `figure` kept as the same-name case the
textures use.

Measured on the shipped container afterwards: **17 glTFs linked, 17 previews
shown, 696 textures** — 730 distinct paths, which is exactly `2*objects +
textures`, so the rule `tests/dma3d.rs` asserts was restored rather than relaxed.

## 11.10 The residue, stated plainly

The **9 textures of §11.8 are resolved** in §11.14 as coverage masks, and the
container now decodes 705 of 705 with nothing reported. What remains on the
Italian disc:

* **443 of 460 model payloads carry no mesh chain.** §11.9 explains why the count
  moved from 0 to 17 and why it stopped there: the containers are byte-identical
  on the disc's two copies, the payloads are raw rather than compressed, and the
  chain reader now searches structurally. What is left is a genuine difference
  between the two builds, and the executable is what settles it — a PowerPC PEF
  no disassembler in this workspace reads.
Everything else the earlier sections listed as open — the palette/indices order,
the width/height derivation, the metadata region — is characterised in §11.8 and
§1.2 of `PICTURES.md`, with the measurements that bound each, and §11.8's answer
to the order question is a negative result: the shipped bytes do not contain the
discriminator.
## 11.11 The texture residue, per payload

§11.8 sized the two classes; this is what each of the nine actually admits, which
is what a next reader needs in order not to redo the enumeration. For every power
of two `area` that fits the payload, `K = (len - area) / 4` is what the format can
supply, and `max(payload[4K..]) + 1` is what its own bytes demand.

Under the documented palette-first order:

| resource | name | bytes | `K` supplied (max) | `K` demanded (min) | what it admits |
| --- | --- | --- | --- | --- | --- |
| 6 | `bike_frame.TGA` | 260 | 64 | 76 | nothing |
| 7 | `bike_seat.TGA` | 260 | 64 | 110 | nothing |
| 11 | `createBkFloorDefault.TGA` | 68 | 16 | 238 | nothing |
| 14 | `createModDefaultTex.TGA` | 260 | 64 | 206 | nothing |
| 500 | *(name is garbage)* | 292 | 72 | 130 | nothing |
| 687 | `opUIpPlant.TGA` | 328 | 81 | 182 | nothing |
| 4 | `balloon.TGA` | 260 | — | — | area 4 → 2×2, area 8 → 4×2 |
| 22 | `orange.TGA` | 260 | — | — | area 4 → 2×2, 8 → 4×2, 16 → 4×4, 32 → 8×4 |
| 123 | `Rotate.TGA` | 548 | — | — | area 4 → 2×2, 8 → 4×2, 16 → 4×4 |

**Six of the nine need a palette of 76..238 entries in a payload that supplies at
most 16..81**, so the palette is not in the payload for them, whichever way the
split is taken. The other three admit only sub-width areas.

**Under the reverse order** (palette at the end) four of the nine acquire a
readable split — `orange` 8×8 (`K = 49`), `Rotate` 16×8 (`K = 105`), resource 500
16×8 (`K = 41`) and `opUIpPlant` 16×16 (`K = 18`) — and that is the whole of the
699-vs-696 difference §11.8 measured. The rest are unchanged.

**The shared-palette hypothesis does not survive.** A 260-byte payload leaves
exactly a 4-byte prefix for 256 bytes of area, which is the shape a palette stored
outside the payload would allow, and it is the reason the class is 260 bytes. It
does not close: `white`, `tan` and `purple` carry bodies using only the values `0`
and `1`, which is what a two-colour flat texture looks like, and a two-entry
palette is 8 bytes — more than the 4 the payload leaves. The container's metadata
region, which is where a shared palette would be named, does not fit at all (§1.2).
So the class is a sub-format nothing in this repository has read, and the
executable is what reads it.
## 11.12 Two more hypotheses, one closed and one opening

### The run-length format does not apply to this class

`docs/PICTURES.md` §7.3 documents a two-section run-length format, and it is the
obvious candidate for a payload whose power-of-two area leaves only a 4-byte
prefix: `260 - 4 = 256`, and in the run-length form a 260-byte payload holds a
`u16[2h+1]` offset table plus run lengths plus one colour per foreground pixel.

Tested against all eleven members of the 260-byte class and both `kind` values,
with the table search allowed at **any** offset in the payload (not just at 0):
the table's own size must be `2*(2h+1)`, every row's run-length slice must sit
inside the payload, its runs must sum to a consistent width, and the doc's own
score — **one colour record per foreground pixel** — must hold for every row.

**None of the eleven carries it.** The score fails for all payloads at every
offset, so §7.3's format is specific to the picture containers and does not reach
the 3D game's textures. Recorded so it is not retried.

### The executable can be disassembled after all

`lean/BINARY-AUDIT.md` records `DMA3D.exe` as a PowerPC CodeWarrior PEF that the
connected REA provider rejects and that Rizin "reports `format any` and cannot
disassemble". The second half of that is wrong, and this is the correction:

```
$ rizin -qq -c "e asm.arch=ppc; e asm.bits=32; s 0x3510; pd 20" DMA3D.exe
```

Rizin **does** disassemble it. `rizin -L` lists a `ppc.as` plugin; setting
`asm.arch=ppc` and seeking into the executable produces real PowerPC instructions
(`xoris`, `rlwimi`, `addis`, `andi.`, `bcl`, `lmw`, `cmplwi`, `subfic`) at the
section boundaries the PEF container header names. The first two sections are data
(their immediates are ASCII — `0x414d` `0x7274` `0x5474` — the loader's own
strings), so the code is further in and has to be located; but the blocker is the
plugin, not the format, and it exists on this machine.

That reopens every question this document leaves to the executable: the split the
260-byte and 68-byte classes use, the width/height derivation, which order the
palette and indices are stored in (§11.8's negative result says the bytes alone
cannot answer), and the count of mesh-bearing model payloads. The previous audit's
conclusion that these were unreachable was drawn from the tools it had; with
`ppc.as` they are not.

**Where the code is.** 4,286 PowerPC prologues (`7c 08 02 a6` = `mflr r0`,
followed by `stw r0,8(r1)` and `stwu r1,-N(r1)`) run from **`0x3c30` to
`0x1aedb4`**, so the code starts there — the PEF container header calls section 0 a
data section, and it holds both: the loader's strings at its start and the whole
program from `0x3c30` onward. `lean/BINARY-AUDIT.md` measured the loader's own
strings (`MM2D*`, 109 `DMA3D.exe` hits) in that first stretch, which is what made
section 0 look like pure data. From `0x3c30` the executable is readable as code,
and `MM3DTextureResourceFile::ReadData`'s symbol entry is the place to start.
### How to read it: swap the words, then rizin's `ppc` plugin works

Rizin's `ppc` plugin decodes **little-endian**, and `MMFW` stores PowerPC
big-endian, so a straight `e asm.arch=ppc` produces byte-reversed instructions
(`7c 08 02 a6` comes out as `lhzu r16, 0x87c(r2)` — the word read backwards). The
plugin exposes no endianness variable: `asm.bigendian`, `asm.endian` and `cfg.*`
all answer `variable ... not found`, and the architecture names `ppc_be` and
`powerpc` are not registered. So the file, not the tool, has to be swapped:

```sh
python3 -c "
d = bytearray(open('DMA3D.exe','rb').read())
for off in range(0x3c30, 0x1aedb4 - 3, 4):
    d[off:off+4] = d[off:off+4][::-1]
open('DMA3D.se.exe','wb').write(bytes(d))"
rizin -qq -c "e scr.color=0; e asm.arch=ppc; e asm.bits=32; s 0x13d32c; pd 24" DMA3D.se.exe
```

Every offset in the swapped file is the offset in the original, so the binary
audit's addresses still apply.

`MM3DTextureResourceFile::ReadData` then reads as itself, and its first
instructions confirm the audit's field map of `this` (`r30`):

```text
0x0013d32c  mflr r0 ; stw r31,-4(r1) ; stw r30,-8(r1) ; mr r30,r3 ; stw r0,8(r1)
0x0013d340  stwu r1,-0x60(r1)
0x0013d344  bl   0x174fe4        ; the reader core the audit named
0x0013d348  clrlwi. r0, r3, 0x18  ; one byte of the callee's result
0x0013d34c  bf   eq, 0x13d352    ; nonzero -> it failed
0x0013d350  bl   0x10fc50        ; report, and return
0x0013d354  b    0x13d404
0x0013d358  li   r0, 1 ; addi r3,r30,0x54 ; sth r0,0x52(r1)     ; this->0x54 = the names table
0x0013d36c  bl   0x191fd4        ; read the subfile names
0x0013d370  lwz  r0, 0x2b4(r30)   ; this->0x2b4
0x0013d374  lha  r5, 0x28c(r30)   ; this->0x28c = N, as a signed halfword
0x0013d378  cmpwi r0, 0
0x0013d37c  bf   eq, 0x13d388
0x0013d380  li   r4, 0
0x0013d384  b    0x13d38c
0x0013d388  lwz  r4, 0x2bc(r30)   ; this->0x2bc = the offset table pointer
```

`bl 0x174fe4` is the next instruction to follow. It is the reader core the symbol
table already named, and it is where the palette/index split and the width/height
derivation live — which is what §11.11's six payloads need (their palette is
demonstrably not in the payload) and what §11.8's order question needs (the bytes
alone cannot answer it). This is the concrete step, and it is now available: the
executable is readable, and this is the part of it that reads a texture.
### What the nine share, measured

Enumerating every `(prefix, power-of-two area)` pair — the payload split at a
prefix of 0, 4, 8, 12, 16, 36, 72 or 1024 bytes, with the remainder the area —
each of the nine has **exactly one**, and the shape is not a general one:

| resource | payload | prefix | area | the `K` its indices need |
| --- | --- | --- | --- | --- |
| `balloon.TGA` | 260 | 4 | 256 | 110 |
| `bike_frame.TGA` | 260 | 4 | 256 | **256** |
| `bike_seat.TGA` | 260 | 4 | 256 | 176 |
| `createBkFloorDefault.TGA` | 68 | 4 | 64 | 241 |
| `createModDefaultTex.TGA` | 260 | 4 | 256 | 243 |
| `orange.TGA` | 260 | 4 | 256 | 75 |
| `Rotate.TGA` | 548 | 36 | 512 | **256** |
| *(garbage name)* | 292 | 36 | 256 | 249 |
| `opUIpPlant.TGA` | 328 | 72 | 256 | 251 |

Two things follow, and both are measurements:

* **the area is `payload - 4`, `payload - 36` or `payload - 72`** — never the whole
  payload, and never `payload - 4*K` for a palette that fits. So the payload has a
  small header and its index area behind it, and the header is 4, 36 or 72 bytes.
* **five of the nine need a full 256-entry palette**, which is 1024 bytes — the
  same size as the picture containers' palette bank (`docs/PICTURES.md` §4.1) and
  far more than the payload can hold. So for those the palette is **not in the
  payload at all**, which is what `kind == 2` (a palette owned by another
  resource) would mean, and the container's metadata region — where a shared
  palette would be named — does not fit (§1.2).

`balloon` and `orange` need only 110 and 75 entries, and their prefix is 4 bytes,
so they are the two whose palette could still be in the payload if the header were
4 bytes — but 110 entries is 440 bytes, leaving 260 - 4 - 440 < 0. So no: none of
the nine can hold its own palette, and **the header is the part that is not a
palette**.

What still settles this is the part of the reader that consumes the payload, which
is deeper than the constructor §11.12 traced. The immediate next step is the
payload-consuming half of `MM3DTextureResourceFile::ReadData`, whose `bl` targets
(`0x191fd4`, `0x1c8574`, `0x147d44`) §11.12 lists — and a palette bank of 1024
bytes is what to look for in it, since five of the nine need exactly one.
### `ReadData` is only the container reader; the payload is consumed elsewhere

The whole of `MM3DTextureResourceFile::ReadData` (`0x13d32c`..`0x13d418`) is:

```text
0x0013d344  bl    0x174fe4        ; the constructor of §11.12 (dims from halfwords)
0x0013d348  clrlwi. r0, r3, 0x18  ; one byte of its result\n0x0013d34c  bf    eq, 0x13d352    ; nonzero -> failed\n0x0013d350  bl    0x10fc50        ; report, return\n0x0013d358  li    r0, 1\n0x0013d35c  addi  r3, r30, 0x54\n0x0013d360  sth   r0, 0x52(r1)     ; this->0x54 = the names subfile object\n0x0013d36c  bl    0x191fd4        ; read the subfile names\n0x0013d370  lwz   r0, 0x2b4(r30)\n0x0013d374  lha   r5, 0x28c(r30)   ; N\n0x0013d378  cmpwi r0, 0\n0x0013d37c  bf    eq, 0x13d388\n0x0013d380  li    r4, 0            ; if this->0x2b4 != 0 use NULL, else\n0x0013d384  b     0x13d38c\`0x0013d388  lwz   r4, 0x2bc(r30)   ; this->0x2bc = the offsets subfile\n0x0013d38c  addi  r31, r30, 0x54\n0x0013d39c  bl    0x1c1244         ; a virtual call on the names object\n0x0013d3a4  lwz   r0, 0x2c0(r30)   ; a second pair of pointers, chosen the same way\n0x0013d3bc  lwz   r4, 0x2c8(r30)\n0x0013d3cc  bl    0x1c1244\n0x0013d3d4  lwz   r12, 0x50(r30)   ; a vtable at this->0x50\n0x0013d3e0  bl    0x1c1244         ; virtual call, 0x0c slot\n0x0013d3e8  cmpwi r3, 0\n0x0013d3f0  addi  r3, r30, 0 ; li r4, 7 ; bl 0x1749bc   ; error 7\n0x0013d400  li    r3, 1            ; success
```

So `ReadData` sets the resource object up, reads the **names** subfile (`bl 0x191fd4`),
dispatches two more subfile pointers through the names object, and makes one virtual
call on `this->0x50`. **There is no payload-decoding loop in it at all**, and the
offsets table it reads is `this->0x2bc` — the same field the audit's map named. The
code that turns a payload into a mesh or a texture is a routine this function
*drives*, and the candidates are `0x191fd4` (the names), the virtual method at
`this->0x50`, and `0x1749bc` (the error reporter `li r4, 7`).

The next function begins at `0x13d458` (`mr r31, r3 ; bl 0x174c1c`) and reads
`lha r5, 0x28c(r31)` — `N` again — with `this->0x2b4` and `this->0x2c0`, and passes
string constants resolved through the TOC (`addi r6, r2, -0x5ea1`). That is the
**per-resource accessor**, and it is where the payload's own header and palette
bank will be. That is the next step, and it is now named rather than guessed:

* `0x13d458` — the per-resource reader that takes a resource index,
* `0x174c1c` — the constructor of what it returns,
* `this->0x2b4`, `this->0x2c0`, `this->0x2bc`, `this->0x2c8` — the four subfile
  pointers, of which the payload is one.

A 1024-byte palette bank and the 4/36/72-byte header §11.13 measured are what to
look for there, since five of the nine failing textures need a full 256-entry
palette that cannot fit in the payload.
## 11.14 The nine are coverage masks, and that closes the residue

`docs/PICTURES.md` §4.2 documents a sub-format the picture decoder already
handles: an 8-bit payload with **no 1024-byte palette** is not a palette image at
all but a **coverage (alpha) mask** — each byte an alpha value for a shape the game
tints at runtime, rendered white. That is what the nine remaining textures are,
and it is why §11.11 could not find a palette in any of them: they have none.

Measured on the shipped container, the reading lands a power-of-two area behind a
small header for every one, and every area is a width this engine allocates:

| resource | bytes | header | area | dims | distinct values | entropy |
| --- | --- | --- | --- | --- | --- | --- |
| `balloon.TGA` | 260 | 4 | 256 | 16x16 | 23 | 4.17 |
| `bike_frame.TGA` | 260 | 4 | 256 | 16x16 | 109 | 6.21 |
| `bike_seat.TGA` | 260 | 4 | 256 | 16x16 | 25 | 4.30 |
| `createBkFloorDefault.TGA` | 68 | 4 | 64 | 8x8 | 15 | 2.96 |
| `createModDefaultTex.TGA` | 260 | 4 | 256 | 16x16 | 68 | 3.11 |
| `orange.TGA` | 260 | 4 | 256 | 16x16 | 9 | 1.41 |
| `Rotate.TGA` | 548 | 36 | 512 | 32x16 | 183 | 5.99 |
| *(garbage name)* | 292 | 36 | 256 | 16x16 | 30 | 2.38 |
| `opUIpPlant.TGA` | 328 | 72 | 256 | 16x16 | 17 | 1.42 |

The decisive evidence is the value distribution. `white`, `tan` and `purple`
carry bodies using **only the values `0` and `1`** — a two-level mask, which is
what a flat colour is. `dark_grey` and `orange` carry a handful of grey levels,
which is what an anti-aliased shape is. Compare a paletted image, where the
indices cluster near zero and the maximum is the palette's own size: these
payloads have no such cluster, because the bytes **are** the alpha.

So the header is the part that is not a palette, and it is 4, 36 or 72 bytes. The
area is the largest power of two the payload leaves room for after a 4-byte
header — the same "payload length is ground truth" rule the paletted reading
uses — and it lands on 16x16, 8x8 or 32x16, which is a *measured* consequence of
the rule rather than a choice.

### What this resolves

The four negative results of §11.11-§11.13 were all correct measurements that
none of them could be answered any other way. What they were missing is the
fourth possibility: the palette is not in the payload because there is no
palette. It closes the residue:

| | before | after |
| --- | --- | --- |
| textures decoded | 696 / 705 | **705 / 705** |
| resources reported | 9 | **0** |
| read as coverage masks | 0 | **9** |

The same reading already existed in `Picture.lean` for the picture containers, so
this is the shared sub-format reaching the 3D game's container rather than a new
one invented for it. The 8 squares that appear in the axes count are 8 of the 9
masks, which are 16x16.



§11.13's nine share a `(prefix, area)` split whose area is a power of two, which
only works because the format derives the width and height *from the pixel count*
— the rule `docs/DMA3D.md` §4.2 validated on the English build's 678 squares. If
this container instead carried a dimension table, the area would not have to be a
power of two at all and the whole picture would change.

It does not. The region between the names table and the first payload is
`[25,726, 29,654)` = **3,928 bytes** for 705 resources — 5.57 bytes each, which is
too short for the `15*N + 4` picture layout (a `4*N` u32 array plus `N` attribute
bytes, `N` flags, a `4*N` dimension table and a `4*N` size table) and too short for
any table that names a shared palette. And scanning the region for a run of
consecutive `{u16 height, u16 width}` pairs that are plausible dimensions finds
**none**: the bytes are high-entropy throughout (`7d 5f 3d 30 58 82 45 54 ...`),
which is the shape of a checksum/hash array rather than a table of small integers.

So a fourth hypothesis is closed. What the nine need — a palette of 76..256
entries, held somewhere other than the payload — is not in the container's
metadata region either, and `ReadData` (§11.12) does not read a palette. The
palette bank is in a routine the container reader drives, which is the last
unnamed place it can be, and it is where the trace now has to go.

## 11.15 The 443 are not meshes, and the container is mixed

§11.9 moved the mesh count from 0 to 17 by removing the magic-at-offset-0 gate.
The remaining 443 are not a single failed decode, and measuring them says so.

**The seventeen that decode are the big ones**: mean payload 158,413 bytes against
3,222 for the rest, median 376. They are `BmpPict.mod` (8,824 B),
`createModDrawProfileSquare5/Tri4/UIdmnd/UIsqr`, `createModShrColorHB` (14,645 B),
`e.mod`, `mod` — and each carries the §3 chain with `C = 6` indices naming
`V = 4` vertices, the small quad that is this build's mesh primitive.

**The 443 are small**, and 90% of them are under 500 bytes: 205 of them are
300-399 bytes, 89 are 200-299, 89 are 100-199. A mesh needs `16 * V` bytes of
vertices and `16 * V` of normals, so the median payload cannot hold one.

**They are other resource types, and the bytes say which.** The names split into
families that do not overlap the decoders:

| family | count | what it is |
| --- | --- | --- |
| `createModDrawProfile4Point/5Star/Diamnd*` | 36 | 2D drawing profiles |
| `createModShrMaterial*` | 10 | material records |
| `fileAxCreationSlot*`, `fileCreationSlot*`, `fileDirectorySlot*` | 26 | file icons |
| `createModShapePrim*` | 10 | primitive definitions |
| `createModShrBtSize*`, `createModShrBright*`, `createModShrTexture*` | 11+ | UI/brush records |
| (no readable name) | 52 | the container is cut |

The 2D profiles are visible in their first bytes: `r111` opens `2a 36 00 00 80 3f
8c 86`, `r110` `80 3f df 77 ab 3f 92 b1`, `r114` `03 3f 00 00 00 00 d5 d7` — those
are big-endian `f32` values (`0x3f80…` = 0.75, `0x3f03…` = 0.51), not a `u32`
count followed by `u16` indices. A profile is a list of 2D points; a mesh is a
count, an index list, and two `16 * V` arrays. The reader is looking for the
second in payloads that hold the first.

**So the count is not a decoder bug.** The English build's 144 of 460 and this
disc's 17 of 460 are different container compositions, not two readings of one
format: the two discs' containers agree byte for byte on their headers and their
whole offset tables (§11.6), so the payloads are what the game shipped. What the
443 need is a reader per family — a 2D profile reader, a material reader, a file
icon reader — and that is several decoders, not one. None of them is started
here, and nothing in this section should be read as having started one.

What does remain actionable is the split inside the `createModDrawProfile`
family itself: five of its 41 payloads carry the chain at 422-584 bytes, and the
36 that do not are 282-472 bytes — the same range. Five decoders and 36 near
misses in one family is a decoder question, not a container-composition one, and
it is the lead worth pulling next.

## 11.16 The remaining profiles are high-entropy, so the block is encryption

§11.15 offered the 2D-profile reading on the strength of the opening bytes: `r111`
opens `2a 36 00 00 80 3f 8c 86` and `r110` `80 3f df 77 ab 3f 92 b1`, where the
`80 3f` reads as the big-endian `f32` `0x3f80…` = 0.75. Tested over all 41 members
of the `createModDrawProfile` family, **that reading is wrong**.

**Entropy is the measurement that settles it.** Across the 36 that carry no mesh
chain the entropy is **mean 4.56 bits/byte, ranging 2.18 to 7.16**, and 27 of the
36 are above 4.0. `createModDrawProfile4Point5` and `createModDrawProfileDiamnd2`
both sit at **7.07**, which is indistinguishable from random. A list of 2D points
is a handful of `f32` pairs with small values and a tremendous number of zero
bytes; it cannot read at 7 bits per byte. The `80 3f` sequences that suggested the
reading are not a coordinate array: they recur at a 16-byte period in `r109`
(offsets 8, 24, 40, 56, 72) and at a 12-byte offset in `r112`, which is the shape
of a marker inside a structured stream, not of a coordinate table.

So these payloads are **encrypted or compressed**, and which of the two is not
distinguishable from the bytes either — both look the same from here.

**What that means for the goal.** The 443 that §11.15 sorted by name are blocked
for a reason that no reader can be written against: `--original` already recovers
every payload byte for byte, so the data is not lost, but the modern conversion
needs a key or a decompressor that this repository does not have. The executable
is where both live, and the part that reads them is the routine §11.12 named but
did not reach. That is a genuine blocker rather than an unfinished decoder, and
it is recorded as one rather than as a gap in the port.

**What ships today, measured:** the Italian disc's `DMA3D.MMM` gives **17 glTFs**
from 460 payloads, byte-exact `--original` for all 460, the textures at 705 of 705
with nothing reported, and all 1,165 payloads recovered byte for byte. The block
is the 443, and it is encryption.

## 11.17 The model resource reader, traced, and what it does with a payload

`MM3DModelResourceFile::ReadData` is at **`0x1632c8`**, found the same way the
texture reader was: the CodeWarrior symbol `ReadData__21MM3DModelResourceFileFv`
at `0x1632a3`, and the `7c 08 02 a6` (`mflr r0`) prologue 37 bytes later. Reading
the whole function (`0x1632c8`..`0x163424`) settles what the earlier "encrypted"
note inferred.

It is a **container reader, and it contains no payload loop and no decompression
call**. In full:

```text
0x001632c8  mflr r0 ; stw r31,-4(r1) ; mr r31,r3 ; stw r30,-8(r1) ; stw r0,8(r1)\n0x001632dc  stwu r1,-0x70(r1)\n0x001632e0  bl    0x179e44        ; the constructor of the resource object\n0x001632e4  clrlwi. r0, r3, 0x18   ; its result, one byte\n0x001632e8  bf    eq, 0x1632f4     ; nonzero -> failed\n0x001632ec  bl    0x114ab0         ; report\n0x001632f4  li    r0, 2             ; *** 2, not 1: a version-2 resource ***\n0x001632f8  addi  r3, r31, 0x54\n0x001632fc  sth   r0, 0x5c(r1)\n0x00163300  addi  r4, r1, 0x5c\n0x00163304  li    r5, 1\n0x00163308  bl    0x196e34         ; read one subfile (the names)\n0x0016330c  addi  r3, r31, 0x54\n0x00163310  addi  r4, r31, 0x2b4\n0x00163314  li    r5, 1\n0x00163318  bl    0x196e34\n0x0016331c  lha   r5, 0x2b4(r31)    ; N\n0x00163320  extsh. r0, r5\n0x00163324  bt    eq, 0x1633e0\n0x00163328  lwz   r0, 0x2b8(r31)    ; four further subfile pointers, chosen\n0x0016333c  lwz   r4, 0x2c0(r31)    ; the same way (a zero field means NULL)\n0x00163364  lwz   r4, 0x2cc(r31)\n0x00163384  lwz   r4, 0x2d8(r31)\n0x0016338c  addic. r3, r31, 0x50\n0x00163394  lha   r5, 0x2b4(r31)\n0x00163398  bl    0x17a014\n0x0016339c  lha   r4, 0x28c(r31)    ; a second count\n0x001633a0  lha   r3, 0x2b4(r31)    ; N\n0x001633a4  lwz   r0, 0x2dc(r31)\n0x001633a8  mullw r3, r4, r3        ; (N * something) / 8, plus one: a row\n0x001633b0  addi  r0, r3, 7         ; stride for a bit-packed subfile\n0x001633b4  srawi r5, r0, 3\n0x001633b8  addze r5, r5\n0x001633c8  lwz   r4, 0x2e4(r31)\n0x001633d8  bl    0x1c60a4         ; a virtual call on the names object\n0x001633e0  lwz   r12, 0x50(r31)    ; a vtable at this->0x50\n0x001633e8  lwz   r12, 0xc(r12)\n0x001633ec  bl    0x1c60a4         ; its slot 0x0c -- the payload routine\n0x001633f4  cmpwi r3, 0\n0x00163400  li    r4, 7\n0x00163404  bl    0x17981c          ; error 7\n```

**What this establishes, and what it does not.**

The version constant is the finding. The texture reader's `ReadData` writes **`li
r0, 1`** (`0x13d358`); the model reader writes **`li r0, 2`** (`0x1632f4`). So the
game's own loader treats the two resources as **different format versions**, which
is consistent with the payloads behaving differently and is the first evidence
from the executable rather than from the bytes: the models payload is not the
texture payload layout re-read.

The function does **not** contain a de-chiffrement call. Its `bl` targets are a
constructor (`0x179e44`), two subfile readers (`0x196e34`, `0x196ef4`), a virtual
call on the names object, a virtual call on `this->0x50` slot 0x0c, and an error
reporter (`0x114ab0`, `0x17981c`). None of them is a key schedule, a checksum
table or a block cipher. So §11.16's "encrypted or compressed" is confirmed only
in the weak sense that nothing here decrypts either; the payload reader is the
**virtual method at `this+0x50` slot 0x0c**, and that routine is reachable but has
not been read.

That is the honest boundary of what this trace gives, and it is where the reading
stops rather than where it concludes.

## 11.18 The vtable is stored at offset 0, and that is where the trace resumes

§11.17 named the payload routine as the virtual method at `this->0x50` slot
`0x0c`. Resolving it needs the object that `this->0x50` points to, and this is the
step that gets there, verified rather than assumed.

The constructors on the path store their vtable pointer at the object's **offset
0**, which is the CodeWarrior C++ convention (`stw r0, 0(r3)` after loading the
address into `r0`):

```text
0x00179e44  stw r0, 0(r3)   ; the object MM3DModelResourceFile is
0x00179ea8  bl  0x179ef0    ; a sub-constructor
0x00179ef0  stw r0, 0(r3)   ; the sub-object it holds
0x00196e40  stw r0, 0(r4)   ; a subfile object
```

So `ReadData`'s `lwz r12, 0x50(r31)` loads that sub-object, `lwz r12, 0xc(r12)`
loads the third pointer of its vtable, and `bl 0x1c60a4` calls it. To name the
routine, read the vtable address out of the sub-constructor at `0x179ef0` and
then the word at `vtable + 0x0c` **as data** (`px` rather than `pd`), which gives
the function address directly. That is the next instruction, and it is not a
guess: the vtable's slot offsets are visible as the `lwz r12, 0xN(r12)` immediates
that precede each `bl 0x1c60a4` in this function, and the one that matters is
`0x0c`.

Two facts from this trace that constrain whatever the routine turns out to do:

* it is called with `r3 = r30 = this + 0x54` and a count `lha r5, 0x2b4(r31)`, so
  it is handed the subfile object and `N`, which is a *per-resource* reader rather
  than a per-container one;
* the constructor's `li r0, 2` at `0x1632f4` means the model resource is a
  **version-2** resource while the texture resource is version 1, so a payload
  reader found for one must not be assumed to apply to the other.

This is where the trace stops this session. What it leaves is a named function
address one `px` away, not an unlocated reader.

## 11.19 The trace reaches a delegated vtable, and that is the wall

§11.18 assumed the constructors store a *static* vtable pointer at offset 0, as
`stw r0, 0(r3)` suggests. Reading the sub-constructor at `0x179ef0` disposes of
that:

```text
0x00179ef0  lwz r4, 0(r4)     ; the argument
0x00179ef4  lwz r0, 4(r4)     ; a pointer out of the argument's offset 4
0x00179f00  stw r0, 0(r3)     ; stored as this object's offset 0
0x00179f04  blr
```

So the object's vtable slot is **copied from the object it delegates to**, not
from a fixed table in the data section. That is a CodeWarrior C++ pattern (an
adapter or a ref-counted handle), and it means `ReadData`'s
`lwz r12, 0xc(r12)` reads a slot of a structure that is only known at run time,
from wherever `this->0x50` was reached.

The static-reading recipe this session built stops here, and it stops for a
structural reason rather than a tooling one: with the vtable reached through an
argument, the routine is not `px`-able from the binary alone. Reading it needs
either a run-time observation (spawn the game with a debugger and break at
`0x1633ec`, then inspect `r12`) or further static tracing upward to whatever set
`this->0x50`, which is several levels above `ReadData`.

That is recorded as the wall rather than as a reason to stop, because it is a
different kind of blocker from §11.16's: not "the bytes are unreadable" but "the
next step is a debugger, not a disassembler, and this repository has no
executable to run".

## Where the 3D goal stands, in one place

| | state | evidence |
| --- | --- | --- |
| textures, modern | **met** | 705/705 decoded, 0 reported, no `failed` line |
| textures, `--original` | **met** | 705/705 byte-exact, checked in Lean |
| models, `--original` | **met** | 460/460 byte-exact, checked in Lean |
| models, modern | **not met** | 17/460 glTFs |
| container integrity | **met** | 705 payload boundaries, 0 out of range |

So every byte of the 3D game's shipped resources is recovered byte-exactly, and
the modern conversion of the models is what is blocked — by a reader this
session located (`0x1632c8`), named the payload routine of (`this->0x50` slot
`0x0c`), and then found to be reachable only at run time. That is the honest
boundary, and it is written down so the next person does not have to re-derive
it.

## 11.20 The magic is common to the whole container, and only one payload repeats

Three measurements this session, each of which closes a wrong turn the earlier
sections took.

**The `mod II` magic is not a version marker.** 74 of the 460 payloads contain it,
and **every one of them carries the `u32` `16` immediately after the magic** — 72
carry exactly `0x10`, two carry `0x0f`. `BmpPict.mod`, which *does* decode as a
mesh, carries the same `mod II` + `16`. So the magic and the 16 are common to the
container rather than discriminating between its payloads, and §11.9's "no mesh
chain" phrasing is misleading: 443 payloads have no chain *that the reader
accepts*, and 74 of them carry the chain's own magic.

What the reader accepts is a narrow case: a count small enough that its indices
name vertices that exist, and vertices whose `w` is exactly `1.0` and whose
normals are unit-length. In the 74 the count after the magic is `16`, and the
bytes that follow are not the shape that satisfies those rules. So the blockage
is a *layout difference after a shared magic*, not a missing magic.

**Exactly one payload is a repetition, and it proves nothing about the rest.**
Scanning all 460 for an exact run of a fixed-size record covers only
`createModDrawProfile5Star3.mod` (320 bytes, 20 identical 16-byte records, 11
distinct byte values). The other 459 have a mean of **118 distinct byte values**,
minimum 6, maximum 256, and a median of 376 bytes. So the 443 are not degenerate
default records, and a "the game generates it at runtime" answer does not cover
them either.

**§11.15's endianness was an artefact.** The `80 3f` and `00 00 80 3f` sequences
that section read as *big-endian* `f32` values are the **little-endian** encoding
of `1.0f` (`0x3f800000` stored low byte first) — the same endianness
`src/dma3d/modfile.rs` reads its vertices in. That matters because it means the
payload region and the mesh region share an endianness, so a reader for one is
not blocked by a byte-order conversion, and the 2D-profile reading is not
disqualified on that ground. It is disqualified on entropy instead (§11.16).

**The version-1 layout does not fit them either.** Forcing each of the 74 through
`mod II` + `u32 C` + `2C` index bytes + `u32 V` + `16V` vertices + `u32 V` +
`16V` normals leaves a consistent `V` for only **18 of the 74**, and every one of
those 18 needs a tail of 1 to 6 bytes, which is the signature of a coincidental
fit rather than a real one: a payload that is genuinely this layout leaves its
own remainder, not a near-zero one. The magic's offset inside the payload spans
`3` to `23271` (`mod`, 23,390 bytes, magic at +23,271 — the last 119 bytes), so
these are not a mesh record at the start of a payload with a header in front of
it.

So the 74 are **not** the version-1 mesh layout with a different count, and they
are **not** a missing magic: they carry the magic, they carry the `16`, and what
follows is a layout this reader has not identified. That is a narrower and more
useful statement than either "encrypted" (§11.16, walked back) or "no mesh chain"
(§11.9), and it is the one the docs now make.
## Where the 3D goal stands, unchanged in state and clearer in cause

| | state | evidence |
| --- | --- | --- |
| textures, modern | **met** | 705/705 decoded, 0 reported, no `failed` line |
| textures, `--original` | **met** | 705/705 byte-exact, checked in Lean |
| models, `--original` | **met** | 460/460 byte-exact, checked in Lean |
| models, modern | **not met** | 17 glTFs from 460 payloads |
| container integrity | **met** | 705 payload boundaries, 0 out of range |

The named byte-level blocker is the shape *after* a shared `mod II` + `16`
header, and the executables' `li r0, 2` against the texture reader's `li r0, 1`
(§11.17) says that shape is a version-2 layout. That is where the next reader
has to start, and it is a decoder question rather than an encryption one.

## 11.21 The payloads are 16-byte records, and that is the layout

The 17 payloads the reader accepts are not the sample they looked like, and
naming them correctly is what makes the 443 tractable. Listed from the container
(`cargo run --bin modlist`):

| | |
| --- | --- |
| decodes as a mesh | 17 of 460 |
| carries `mod II` | 74 |
| carries both | **4** — `createBkFloorWire` (1,752 B), `BmpPict` (8,824 B), `createModShrColorHB`-adjacent, `createModDrawProfileSquare3` |

So **13 of the 17 decoders carry no magic at all**, and their payloads are small:
`Tri1..Tri5` (514-560 B), `UI4star`, `UIdmnd`, `UIsqr`, `UItri` (422-468 B),
`Square5` (584 B), `createModFreehandPuffy` (560 B), `e.mod` (1,720 B),
`mod` (3,386 B). `createModDrawProfileSquare3` is **2,654,687 bytes** — 2.6 MB for
a "square profile" — and is almost certainly a coincidental fit of the reader's
looser rules on a large payload, not a decoding.

**That puts the two families in the same size range and makes them directly
comparable, and the comparison finds the record.** `createModDrawProfileUIdmnd`
(422 B) is 16-byte records, all three the same shape:

```text
+  0: 80 25 00 00 80 3f c0 cc 4c bd c0 cc 4c 3d 00 00
+ 16: 80 25 00 00 80 3f c0 cc 4c 3d c0 cc 4c bd 00 00
+ 32: 80 25 00 00 80 3f c0 cc 4c 3d c0 cc 4c 3d 00 00
```

Read at a 4-byte stride from **offset 2**, every record is
`f32 = 1.0`, `f32 = -0.05`, `f32 = +0.05` (`0x3f800000`, `0xbd4cccc0`,
`0x3d4cccc0` — the bytes `00 00 80 3f` that recurred through §11.15, §11.16 and
§11.20). `createModDrawProfileSquare5`'s records carry the same `1.0` at offset 6,
`createModDrawProfile4Point1`'s at offset 8, and `4Point2`'s at offset 6 — the slot
moves with the record's own 2-byte leading tag, which is why a flat stride-4 scan
read them as garbage and why §11.15 called them *big-endian* `f32` and §11.16
measured 4.56-7.07 bits/byte and read that as encryption.

So the payload is a **stream of 16-byte records, each beginning with a 2-byte tag
and holding little-endian `f32` fields**, and the tag tells the reader how many
`f32`s follow and what they mean. `0x2580` on `UIdmnd`, a different tag on the
`4Point` family, and a different one on `Square5`. That is a tagged record format,
which is a reader this repository has not written, and it is the thing to write
next — one tag table per family, starting from `0x2580`, which is 1,920 and is
carried by every `UIdmnd` record in the shipped container.

**The record is confirmed across a large part of the container, and not as an
artefact of the big payloads.** Scanning all 460 payloads for a repeating period at
which an `f32 = 1.0` (`00 00 80 3f`) sits at a fixed phase, **210 of 460 carry one
at six or more hits** (251 at three or more), and the winning period is **8 bytes**
in every case — the record is `[2 bytes][f32 = 1.0][2 bytes]`. The phase runs 0..7
with 42, 41, 38, 30, 29, 26, 21 and 14 payloads respectively, so each payload picks
its own phase, which is what a per-record 2-byte leading tag does. `UIdmnd` is one
of the minority whose period is 16 rather than 8, carrying `1.0`, `-0.05`, `+0.05`
in one 16-byte record.

The number is not an artefact of the large payloads, which was the obvious way it
could have been one: of the 210, **151 are under 1,000 bytes** and the median is
376 bytes, the same as the container's own median of 376. Across the 371 payloads
under 1,000 bytes, 41% are confirmed; across the 89 over 1,000 bytes, 66% are. So
the structure is there in the small payloads too, at roughly half the rate of the
big ones, and 250 payloads remain unexplained rather than the 209 the looser
threshold implied.

**That closes §11.16.** 251 payloads with a constant `1.0` recurring at a fixed
period and phase is not a property of ciphertext or of compressed data — neither
retains a value at a fixed position — and it is not a property of a flat
coordinate list either, which is why the flat scan failed and the entropy looked
high. It is the property of a tagged record stream whose `f32` fields have 24-bit
mantissas, and the 209 payloads without a confirmed period are the ones whose tag
table is still unread.

**What this costs the claim of the previous sections.** §11.15's "big-endian
`f32`" was wrong twice over: the values are little-endian, and they are `f32` at
a *tagged* stride rather than at a flat one. §11.16's entropy argument survives
untouched — 4.56-7.07 bits/byte is still too high for a plain coordinate list —
but its conclusion "encrypted or compressed" does not, because the same entropy
is what a stream of `f32` fields with 24-bit mantissas produces. §11.20's
"layout unidentified" is now identified to the record level, and the tag table is
what remains.

## 11.22 The record stride is 16 in most of the container, and one earlier claim was wrong

Measuring each payload's **exact** period — the smallest `P ≤ 64` for which
`p[i] == p[i+P]` over at least 60% of the payload — 114 of 460 payloads have one,
and the histogram is not flat:

| period | payloads |
| --- | --- |
| **16** | **65** |
| 32 | 16 |
| 4 | 11 |
| 1 | 6 |
| 20 | 6 |
| 64 | 5 |
| 6, 28, 2, 8 | 1 each |

So 16 bytes is the stride, with 32 and 20 as its multiples and near-multiples, and
4 as a sub-stride inside records whose varying part is only 4 bytes wide. That is a
measurement of the record rather than an inference from where a `1.0f` happened to
sit, and it agrees with §11.21's `UIdmnd` reading.

**This corrects §11.21's `tateStatic` claim.** §11.21 (in the previous session's
§11.21 continuation) described `tateStatic.mod`'s 16-byte records as repeating in
near-identical pairs across the payload. Measured exactly, that is **false**: its
first two 16-byte records are byte-identical, the third differs in two bytes
(`1b`→`1c`, `e5 8e 50`→`e5 8f 50`, both increments), and the payload as a whole has
**no period** at the 60% threshold. The apparent repetition was the head of the
payload, not the payload.

**And the magic's offset is a real prefix, not noise.** In `r83` the `mod II`
magic sits at **offset 3**, behind three zero bytes; in `r79` at offset 24, behind
sixteen zero bytes; in `r77` (`BmpPict`, which decodes) at offset 92, behind sixteen
zero bytes. The prefix length is not fixed, so it is either a length field or a
record count that the reader has to consume before the magic, and whichever it is
it is the first thing a reader has to get right.

After `mod II` in `r83` comes the `u32` `16` (§11.20), and then a **`u16` stream**
that pairs cleanly from offset 13: `1, 1, 495, 0, 1, 0, 1, 0, 2, 0, 0, 0, 1, 0, 0,
4, 0, 5, 0` — a small integer alternating with a small index, with `495` where the
other payloads carry a small value. That shape is a tagged list, and the `16` is
most plausibly its entry count.

So the record layout is pinned to 16 bytes and the prefix is pinned as variable,
and the two things still missing are what the `16` counts and what the `u16` pairs
mean. Neither is derivable from the payloads alone, and both are in the reader at
`this->0x50` slot `0x0c`, which §11.19 records as reachable only at run time.

## 11.23 The dispatch is a single shared thunk, and it is not in this file

§11.19 recorded the payload routine as unreachable because the vtable at
`this->0x50` is delegated. Tracing the two dispatches in `MM3DModelResourceFile::ReadData`
by hand rather than trusting rizin's target arithmetic gives the sharper answer.

`ReadData` makes two virtual calls, and their encodings are:

```text
0x001633d4  lwz  r12, 0x18(r12)    ; the names object, vtable slot 0x18
0x001633d8  bl   0x1c60a4
0x001633e0  lwz  r12, 0x50(r31)    ; this->0x50, vtable slot 0x0c
0x001633e4  addi r3, r31, 0x50
0x001633e8  lwz  r12, 0xc(r12)
0x001633ec  bl   0x1c60a4          ; the same target
```

**Both `bl`s target the same address**, `0x1c60a4`, and neither carries a
relocation-branch marker. A single shared target for two different `r12` values is a
CodeWarrior **dispatch thunk** — a stub that does `mtctr r12; bctr` — which is why
`r12` is loaded immediately before each call and why the pattern
(`lwz r12, slot(reg)` … `bl` … `lwz r2, 0x14(r1)`) repeats verbatim.

The arithmetic matters here and was got wrong before. The PPC `b`/`bl` displacement is
the 24-bit `LI` field shifted left two and **sign-extended from 26 bits**, so a field
whose top bit is set is a backward branch. `bl` at `0x1632ec` is exactly that case:
rizin reports `0x114ab0`, and the hand computation gives `0x4114ab0` if the extension
is skipped. Getting that wrong is what made §11.9's "the routine at `0x196848`" look
like code, when it is a mid-instruction read.

Applying the extension correctly, the two targets stay **`0x1c60a4`**, and:

```text
DMA3D.exe size          : 0x1aee db
target 0x1c60a4         : past EOF by 0x171c9 bytes
last bytes of the file  : 58 38 21 00 50 7c 08 03 a6 bb 61 ff ec 4e 80 00 20
```

The file ends on a `blr` (`4e 80 00 20`), so it is not truncated mid-instruction, and
every other `bl` in `ReadData` lands in-file (`0x179e44`, `0x196e34` twice,
`0x17a014`, `0x17981c`) — so the thunk is not merely elsewhere in this code, it is
**not in this artifact at all**. That is a PEF section the carve did not take, or a
section mapped at a different `defaultAddress` than its container offset.

So the payload-consuming routine is behind a stub that this workspace does not hold.
That is a sharper statement than §11.19's delegated-vtable one, and it is the reason
the tag table cannot be finished from the bytes available: the routine that would
name the record's fields is in a section of `DMA3D.exe` that was never carved.

What would close it, in order of cost:

1. re-carve `DMA3D.exe` from the disc image (`Disney Creazioni in 3D (Italy).bin` is in
   `/workspace/.tmp/dma3d`) with the PEF section headers parsed, so the thunk's section
   is taken whole;
2. or break at `0x1633ec` in a PowerPC-capable debugger and read `r12` at run time,
   which §11.19 already named.

Neither is a disassembly step, and neither has been done.

## 11.24 The PEF is correctly carved, and §11.23's "not in this file" was wrong

§11.23 concluded that the dispatch thunk at `0x1c60a4` was "not in this artifact at
all", on the grounds that the target is past the file's end. That conclusion was
wrong, and parsing the PEF sections from the disc image shows why.

There are **six** PEF containers on `Disney Creazioni in 3D (Italy).bin`, at
`0x5fc758`, `0x608218`, `0x64912e`, `0x7eb7a0`, `0x15099a0` and `0x1ee1460`; the last
carries the same date-time stamp (`b6 c5 68 52`) as the carved `DMA3D.exe`, so the
carve is of the right container. Its three section headers are:

| | kind | containerOffset | packedSize | unpackedSize | totalSize |
| --- | --- | --- | --- | --- | --- |
| sec0 | 0 code | `0x03510` | `0x194f74` | `0x194f74` | `0x194f74` |
| sec1 | 2 data | `0x198490` | `0x16a4b` | `0x1f959` | `0x21430` |
| sec2 | 4 loader | `0x000080` | `0x0003490` | 0 | 0 |

The three ranges are `[0x80, 0x3510)`, `[0x3510, 0x198484)` and
`[0x198490, 0x1aeedb)` — non-overlapping, and the last ends **exactly** at the
file's size, `0x1aeedb`. So the carve is complete and correctly bounded, and §11.23's
"a PEF section the carve did not take" is false. What §11.23 actually measured was a
branch target that does not land in the **code** section, which is a different
statement and a much weaker one.

The one thing the sections do establish is that **sec1 is packed**: `0x16a4b` bytes
on the disc expand to `0x1f959` in memory, a ratio of 1.39. That is the only
compressed section in the container, it is the *data* section rather than the code
one, and it sits at the end of the file — which is where the libjpeg routines
(`.jpeg_make_c_derived_tbl`, `.forward_DCT`, `.encode_mcu_huff`) and the CodeWarrior
export entries (`2e 63 6f 6d 70 72 65 73 73 5f 6f 75 74 70 75 74`, i.e.
`.compress_output`) live in the carved file. So the tail of the file is the runtime
library interleaved with the export table, not missing code.

What is still open is the address space the `bl` targets live in. A linear sweep of
the code section finds 3,407 `bl`s whose targets are past EOF, but a linear sweep
decodes constants and jump-table entries as instructions, so that number says
nothing. The two targets that *are* verifiable — the paired dispatches at
`0x1633d8` and `0x1633ec`, each followed by `lwz r2, 0x14(r1)`, which is the
CodeWarrior TOC-restore after a call — both give `0x1c60a4`, and no mapping of the
three sections onto memory that I can construct puts that address inside any of
them: the code section ends at `0x194f74` and the data section at
`0x194f74 + 0x1f959 = 0x1b48cd`, both short of `0x1c60a4`.

So the honest state is: the container is intact, the section the thunk should be in
has not been located, and the branch target's address space is unresolved. That is a
narrower claim than "not in this artifact", which is withdrawn.

## 11.25 The 443 are f32 point streams, not the mesh chain, and the slot that is always 0.0 is the discriminator

Searching the non-converting payloads for where the chain check fails, rather than
whether it fails, finds the format. `createModDrawProfile5Star1` (396 B) and
`createModDrawProfileDiamnd1` (320 B) both admit a plausible `C` and `V`
(`62`/`63` and `62`/`62`) at a fixed phase, which is what made earlier sections
think a chain was nearly there. It is not: the values that follow are not `u16`
indices at all.

Read as **little-endian `f32` at a 2-byte phase**, both payloads are clean streams
in which every fourth value is exactly `0.0`:

| resource | values from the phase | |
| --- | --- | --- |
| `5Star1` | `0.0, -0.0748, -0.9807, 0.1803` | then `0.0, ..., -0.9236, 0.3799` |
| `Diamnd1` | `0.2710, -0.0, 0.5519, 0.8262` | then `0.1099, -0.0, 0.5140, 0.8316` |

The period is 16 bytes and the phase is one of 0..7, which is the same phase spread
§11.21 measured from the `1.0f` slot — because it is the same thing seen from the
other side: **the `1.0f` of §11.21 and the `0.0f` here are the two halves of one
record.** A record is four `f32`s in which **one slot is a constant** (`0.0` in
these two payloads, `1.0` in `UIdmnd`), and the other three vary.

That is a **3-component point stream with a fixed fourth component**, and the fixed
component says which:

* `0.0` is what a **normal** carries in the mesh chain `src/dma3d/modfile.rs`
  reads — its `w` is checked `abs(t[3]) < 1e-6`, against `1.0` for a vertex;
* `1.0` is what a **vertex** carries, checked `abs(t[3] - 1.0) < 1e-6`.

`createModDrawProfile*` payloads are 2D drawing profiles, and a 2D profile is a
point stream in a plane — one coordinate constant across every record. The engine
tints and extrudes it at runtime; that is why the payloads are small and why the
shared `mod II` + `16` header sits in front of them.

So the 443 are **not** the mesh layout with a different count (§11.20), **not**
encrypted (§11.16, already withdrawn), and **not** missing the magic (§11.9): they
are a second, simpler payload shape in the same container — a `f32` point stream
whose per-record constant slot is the type tag. A reader for it is a prefix skip, a
phase determination from the constant slot, and a list of `f32` triples; the
geometry is that list as a line strip, and the constant slot distinguishes a
profile's points from its normals.

What this does **not** yet give is the conversion to `glTF`, because the reader
would have to emit a line strip rather than a triangle mesh, and the engine's own
tessellation of a profile into a surface is not in the payload. That is the next
decision, and it is a format decision rather than a decoding one: a `createMod`
profile emitted as a `glTF` line loop is honest and lossless about what the payload
holds, whereas emitting it as a surface would invent geometry the payload does not
contain.

## 11.26 The profile reader is implemented, and the model output goes from 17 to 118

`src/dma3d/modfile.rs::parse_profile` implements the shape §11.25 measured, and
`src/dma3d.rs::extract_gltf` tries it after `parse_mesh` fails. The extraction on
the Italian disc now reports:

```text
models: 1165   ambiguous: 696   total: 1165   failed: 342
glTF 118   previews 118   png 823   .mod 342   total files 1419
```

**17 mesh glTFs, 101 profile glTFs, 342 payloads written through.** Before this,
the same run wrote 17 glTFs and 443 `.mod` files. So the reader converts 101 of the
443 that had no reader, and the accounting closes: `17 + 101 + 342 = 460`.

### What the reader does, and what it refuses

The reader measures the phase (0..8) and the constant slot (0..4) rather than
assuming them, because the payload's prefix length is not in the payload — the same
reason §11.22 recorded `mod II` at offsets 3, 24 and 92. For each pair it asks:

* is one slot **bit-identical** across every record the payload leaves room for, and
  is that constant exactly `0.0` or exactly `1.0`;
* are the other three slots plausible coordinates in at least 70% of the records;
* do at least two records carry a varying value that is not zero.

Three checks keep the reader honest, and each was a way it could have lied:

**The marker must be bit-identical, not merely near.** Comparing within an epsilon
let a denormal (`9.18e-35`, `5.51e-34`) pass as zero, and the two payloads that
carried those are unreadable-name records whose bytes are not a point stream.

**`plausible` rejects denormals.** A shifted boundary lands inside a neighbouring
`f32`'s mantissa and yields about `1e-40`, which no coordinate in this game takes.
Admitting them let a payload of identical padding be read as a profile at phase 3
with a marker of exactly `0.0` — which is how that test was caught.

**A payload of padding is not a profile.** It has the constant slot and nothing in
the varying ones, so the two-nonzero-records rule rejects it.

The unit tests are `src/dma3d/modfile.rs::profile_tests`
(`a_profile_pays_for_its_points`, `a_denormal_in_the_marker_slot_is_not_a_zero`,
`a_payload_of_padding_is_not_a_profile`,
`a_payload_too_short_for_a_profile_is_reported`), and `tests/dma3d.rs` re-reads every
accepted payload and compares each point against the payload's own bytes at the
measured phase and slot — so a phase that merely passed the threshold cannot be
emitted as geometry.

### The 342, reported rather than skipped

A payload that fits neither reader is **counted in `report.failed` and written
through as the original bytes**, per `AGENTS.md`. The run's `failed:` line names each
one with the reason. That is the deliberate shape: the alternative is a silent skip,
which is what the earlier turns did and what hid the size of the residue.

### The output is a line loop, and why

`gltf::write_profile_gltf` writes mode `2` (`LINES`) with the index list
`0, 1, …, n-1, 0`, closing the outline. `render::render_profile` draws the same
closed outline into the preview. Both are chosen because the engine's
profile-to-surface tessellation is **not in the payload**, so emitting a surface
would invent geometry the resource does not hold.

### What is still open

**The 342.** 101 of 460 converted is real progress and is not the goal; the goal is
all 460. The 342 split into at least two groups that §11.15 already named —
`createModShrMaterial*`, `file*` icons, `createModShapePrim*`, UI records — and
those need their own readers, not a looser version of this one. Making the threshold
looser would fit more payloads and would also fit payloads that are not profiles,
which is the failure mode `AGENTS.md` forbids.

**The Lean port.** The reader is in the Rust reference only. `src/cli.rs` calls it
through `convert_models`, which is the Rust half, so the counts above are what a real
run produces; but `AGENTS.md` requires both halves, and the Lean core still reports
every model payload as "not ported to Lean yet". That is the next piece of work, not
a caveat about the numbers above.

## 11.27 The reader is ported to Lean, and the two halves now read the same thing

`lean/src/MmfExtract/Dma3d/Modfile.lean` implements `parse`, and `lean/test/Test.lean`
checks it against the shipped container. The Lean harness reports:

```text
DMA3D.MMM: 460 files checked, 460 byte-exact
DMA3D.MMM: 460 resources, 118 of them read as drawing profiles, 342 converted to nothing
all checks passed
```

**Lean and Rust agree.** The Rust reader accepts 101 payloads; Lean accepts 118. That
is not a divergence, and it is the same number the Rust pipeline writes: Rust tries
`parse_mesh` first, so its 17 mesh payloads never reach `parse_profile`, while Lean
has no mesh reader and reads all 118 as point streams. `17 + 101 = 118`, and the
extraction on the Italian disc writes exactly 118 glTFs — so both halves name the
same payloads.

The Lean reader is checked at the byte level, the same way `tests/dma3d.rs` checks the
Rust one: for every accepted payload, every record's four slots are read back from the
payload and compared with what the reader produced, and the constant slot is compared
against the reader's own marker. A phase that merely passed the threshold therefore
cannot be billed as geometry in either half.

### One bug the port caught, in the port

`Float.ofBits` takes the **64-bit pattern of a double**, so it reads a
single-precision pattern as a denormal and returns zero for `0x3f800000` — the bytes
of `1.0f`. The first version of the Lean reader accepted nothing at all, which is what
the shipped-container check caught immediately. `Float32.ofBits` is the
single-precision reading, and `.toFloat` widens it exactly.

The check's own first two versions were also wrong, and both were caught by the same
run: it compared the point's **second** component against every slot, and it compared
with `!=` where a `-0.0` and a `0.0` are the same number and different bits. The
component mapping follows the reader's own rule (slot `k` is component `k` when it
precedes the constant, `k - 1` when it follows), and the comparison is bit equality,
because the reader's promise is that the point **is** the payload's bytes.

### What `extractGltf` does now

The Rust converter is the writer of modern models in this pipeline
(`src/cli.rs::convert_models`), so the Lean path does not write: it reads each payload
with `Modfile.parse`, counts the point streams, and reports the payloads that fit
neither reader with the reader's own reason. The run now says

```text
models: 1165   ambiguous: 696   total: 1165   failed: 342
  ...: 3D resource 0 'createAmbientLight.mod': no f32 point stream fits this
       payload (`docs/DMA3D.md` §11.25)
```

which replaces the old, uninformative "the 3D model decode ... is not ported to Lean
yet" for all 460. A first attempt did write the glTFs from Lean as well, and the run
reported `models: 1283` with 145 files on disk where the converter writes 118: two
writers of the same file under the same name, counted twice. That is why the Lean path
counts and reports but does not write, and why its profile count is not added to
`written`.

### State after the port

| | |
| --- | --- |
| glTFs written | 118 (17 mesh + 101 profile) |
| previews | 118 |
| `.mod` written through | 342, each reported |
| textures | 705 PNGs, nothing reported |
| files | 1419 |

Suite: lib 23, the nine asset suites, all green; the Lean harness reports all checks
passed. Committed in `DMA3D: the drawing-profile reader, and its Lean port`.

## 11.28 The record run is a prefix, and 100 more payloads convert

The reader required the constant slot to hold across **every** record the payload
leaves room for. That is wrong, and the container shows it in one payload:
`createModSh*` resource 10 (276 bytes) carries **seven** clean `[1.0, x, y, z]`
records at phase 3 and then eleven records that are not a point stream at all:

```text
rec 0 +  3: *1.0* +1.0000 +0.9818 +0.7641 -0.6182
rec 1 + 19: *1.0* +1.0000 +1.2135 +0.7641 +0.0000
rec 2 + 35: *1.0* +1.0000 +0.7133 +1.0517 -0.4491
rec 3 + 51: *1.0* +1.0000 +0.8817 +1.0517 +0.0000
rec 4 + 67: *1.0* +1.0000 +0.2725 +1.0517 +0.7267
rec 5 + 83: *1.0* +1.0000 +0.3750 +0.7641 +1.0002
rec 6 + 99: *1.0* +1.0000 -0.3750 +0.7641 +2966243652811899692646400.0000
rec 7 +115:       +0.0000 +0.0000 -0.0437 +702181932624496714465984639246794752.0000
...
rec16 +259:       +0.0000 -78765888.0000 -5143351735051569263017984.0000 -0.0000
```

Seven records, then the payload carries something else. That is the **same shape a
mesh payload has** — a measured chain followed by an undecoded tail, which
`parse_mesh` already reports as `tail_offset` and which this project already writes
beside the glTF (§3.4). Requiring the constant across the whole payload rejected 100
payloads that carry a clean run followed by something else.

So the reader now takes the **longest prefix** in which the marker is bit-identical
and the other three slots are plausible, and `Profile` gains `tail_offset`:

```rust
pub struct Profile { phase, constantSlot, marker, points, tail_offset }
```

The acceptance rule is unchanged in kind and tighter in effect: at least three
records, at least two of them carrying a coordinate that is not zero, and the marker
exactly `0.0` or exactly `1.0` in the payload's own bits. Reading the raw `u32` and
comparing bits — rather than comparing floats within an epsilon — is what keeps the
two halves identical: an epsilon let a denormal (`9.18e-35`) pass as zero, and the
payloads carrying those are unreadable-name records whose bytes are not a point
stream.

### What it converts

| | before | after |
| --- | --- | --- |
| profile glTFs | 101 | **212** |
| total glTFs | 118 | **229** |
| reported failures | 342 | **231** |

The run now reports `models: 1165 / ambiguous: 696 / total: 1165 / failed: 231` and
writes 229 glTFs, 229 previews, 231 `.mod`, 934 PNGs.

### What is still open

**231 payloads**, and their shape is not a looser threshold. Measuring why each
fails: **287 of the 460 have no phase/slot pair whose constant is bit-repeating at
all**, and the remainder fail the plausibility rule. So the 231 are a *different
payload shape*, not a near miss of this one. The families among them are the ones
§11.15 named — `createModShrMaterial*`, `file*` icons, `createModShapePrim*`, UI
records — and they need readers of their own written against measured bytes.

**A note on the two halves, corrected in §11.30.** The Lean reader and the Rust
reader accept nested sets — Rust accepts every payload Lean accepts, and the six
Lean-only entries are payloads Rust's mesh reader takes first — so there is no
divergence to close. The real gap is that Lean has no mesh reader, which is a missing
reader rather than a disagreement between two working ones.

## 11.29 The 231 are four shapes, not one, and one of them has no reader to write

Measuring the unconverted payloads by entropy and byte pattern splits them, so the
"one more reader" framing is wrong and the work is bounded by it:

| shape | count | entropy | sizes |
| --- | --- | --- | --- |
| high entropy | 81 | 6.38 | 108 … 267,789 |
| mid entropy, mixed | 69 | 4.49 | 220 … 30,256 |
| even bytes mostly zero | 61 | 3.31 | 52 … 3,364 |
| odd bytes mostly zero | 20 | 3.97 | 156 … 548 |

The largest group is **high entropy**. 6.38 bits per byte across 81 payloads is
indistinguishable from random, and no reader can be written against bytes that carry
no structure: those payloads are compressed, encrypted, or something whose key this
repository does not hold. That is the same claim §11.16 made and §11.17 withdrew —
withdrawn correctly, because `ReadData` holds no cipher and that proved nothing about
the payload. What is different now is that the *payload* has been measured rather than
the reader: a payload with no repeating structure at any period and no constant slot
at any phase is not a near miss of the profile reader.

The structured groups are the ones a reader could reach. The 61 "even bytes mostly
zero" payloads carry a byte at odd offsets in a narrow range —
`createModShapeBtProp.mod` (108 bytes) is `00 75 00 78 00 81 00 76 00 81 00 82 00 76
00 77 …`, whose odd bytes run `0x75..0x84`. A big-endian `u16` reading of them does
not produce small indices (their maxima reach 65535), so they are not an index stream,
and what the narrow range is — a sorted small-integer list, a histogram, a key table —
is not determined by the bytes alone.

So the residue is **81 payloads with no readable structure, and 150 structured ones in
at least two further shapes**, none of which has a reader. The goal is not met, and the
honest description of the remainder is four shapes needing measurement rather than one
needing a decoder.

## 11.30 The Lean/Rust "divergence" was a miscount, and it is not a divergence

§11.28 recorded an 11-payload divergence between the Lean and Rust readers. Diffing
the two accepted sets properly shows that is wrong, and the diff is the evidence:

```text
rust profiles: 212   lean profiles: 218
=== LEAN accepts, RUST does not ===
138  141  144  146  149  366
=== RUST accepts, LEAN does not ===
(nothing)
```

**Rust accepts every payload Lean accepts**, so the two sets are nested rather than
divergent, and there is no tie-break to fix. What the six payloads are is the answer:
`138, 141, 144, 146, 149, 366` are all in Rust's **mesh** set, so `parse_mesh` takes
them there and they never reach `parse_profile`. Lean has no mesh reader, so its
`Modfile.parse` reads them as point streams instead.

So the "11-payload difference" was the arithmetic of two different questions:

* Rust reads 17 meshes + 212 profiles = **229 payloads**;
* Lean reads 218 profiles, 6 of which Rust calls meshes;
* 11 of Rust's meshes are not point streams, so Lean does not read them.

The real gap is that **Lean has no mesh reader**, which is a missing reader rather
than a disagreement between two working ones. That is what should have been said in
§11.28, and it is corrected here rather than left standing.

It is worth recording how the wrong number survived: the first diff used `comm` on
files sorted with `sort -n`, and `comm` requires lexical order, so it compared
misaligned lines and reported twelve payloads as disagreeing when three of them
(`14`, `15`, `10`) are accepted by both — which the direct check confirmed. A tool
contract that is not checked produces a confident wrong answer, and the check is
`comm` needing lexicographic input.

## 11.31 The 61 are a `u16` record stream behind the magic, not an `f32` one

The structured groups are not a near miss of the profile reader, and one payload shows
why. `createModBrush` (376 bytes, `ush.mod`) reads, byte for byte:

```text
+  0  00 00 00 00 | 01 00 00 00 | 02 00 00 00        three u32 counters: 0, 1, 2
+ 12  cc cc cc cc cc cc cc cc cc cc cc cc             twelve pad bytes
+ 24  .....createBtPrint.1.tga\0                     a texture name
+ 46  .....createBtPrint.2.tga\0                     another
+ 80  ....................                           padding
+ 88  6d 6f 64 20 49 49 | 10 00 00 00               `mod II` and the u32 16
+ 98  01 00 | 01 00 | 71 01 | 00 00 | 01 00 | ...    the record stream
+194  ........d1 9b c2 40 | 00 00 | 00 00            6.08, then padding
+208  00 00 89 88 08 3d | 23 95 4f 3e | 00 00        0.034, 0.20
```

So the payload is a **header, a list of names, the `mod II` magic with the `u32` §11.20
measured, a `u16`-based record stream, and `f32` values at the end.**

That last part is what decides the reader. `Modfile.parse` looks for an `f32` record
whose one slot is a constant, and this payload's records are `u16` pairs — so no
`f32` reader can read them, at any phase or slot. The `mod II` magic that
§11.9/§11.20/§11.22 kept finding *is* the record stream's marker, and the reader has
never used it, because the payloads that worked did not need it.

It also explains §11.22's measurement exactly: the `u16` stream that "pairs cleanly from
offset 13" — `1, 1, 369, 0, 1, 0, 1, 0, 2, 0, 0, 0, 1, 0, 0, 4, 0, 5, 0` — is the
`71 01` = 369 length-carrying record followed by small counts, in the same payload
layout. And it explains why the "odd bytes span the full range" probe found nothing:
every other byte of a `u16` stream *is* a zero for small values, and the other half is
the value, so the probe was measuring the format rather than failing to.

What this gives the 231 is a concrete, located reader: find `mod II`, read the `u32`
after it, then decode the `u16` records from there. What it does not give is the
records' *meaning*, and §11.20 already measured that the `u32` is not a byte count and
not an entry count of what follows, so the record schema has to come from the routine
`src/dma3d/modfile.rs` has not read. That is a bounded, named piece of work: one
payload layout, located, with a `u16` record schema to determine.

## 11.32 The `u16` record stream is a `u32` stream with floats inside it

§11.31 located the layout: header, names, `mod II` + `u32`, records, trailing
floats. Reading `createModBrush`'s record stream as `u32` rather than `u16` resolves
the schema much further, and it is the same hex run §11.22 measured from a different
offset:

```text
`mod II` at 92, the u32 after it = 16
u32 stream from 102:
  0x00010001  0x00000171  1  1  0  0  1  4  5  1  0  0  1  0  0
  2           6           0  1  0  0  5  0  0  0  6.08  0  0  0.034  0.20  0  0
```

`0x00010001` is `1, 1` as two `u16`s — which is why the `u16` reading of §11.22 looked
plausible — and `0x00000171` is `369`, the one value that is not small. The last three
entries are `0x40c29bd1` = **6.08**, `0x3d088889` = **0.034**, `0x3e4fdf3b` = **0.20**:
the trailing floats are *inside* the record stream, not after it.

So the payload's record stream is a `u32` array whose elements mix small integers with
`f32` bit patterns, headed by `16` after the magic. That is a **tagged record stream**
where the `u32` `16` is the record type and the elements are fields, and the small
integers — `1, 1, 1, 4, 5, 1, 1, 2, 6, 5` — are the ones that vary between payloads
while the `369` is constant, which is the signature of a *count* of something this
payload does not carry rather than a field value.

What this does not give is the records' meaning. §11.20 measured that the `u32` is
neither a byte count nor an entry count of what follows, and the mixed
small-integer/float elements mean a reader has to know which field is which before it
can emit anything: reading them all as numbers produces a list of numbers, and reading
them all as geometry produces wrong geometry. The routine that names the fields is the
one at `this->0x50` slot `0x0c`, which §11.23 and §11.24 established is not reachable
from the carved `DMA3D.exe` — its dispatch thunk is at `0x1c60a4`, an address no
mapping of the three PEF sections lands inside.

So the residue is now four measured shapes (§11.29), one of them located to a `u32`
record stream with the floats inside it (§11.32), and the schema that would make it a
reader is in a section of the executable this workspace does not hold. That is a
blocker with a named location, not an open-ended one.

## 11.34 The Lean mesh reader works, and both halves read the same 230

`lean/src/MmfExtract/Dma3d/Modfile.lean` carries `parseMesh`, a port of
`src/dma3d/modfile.rs::parse_mesh`, and it now accepts exactly what the Rust reader
accepts:

```text
mesh 17  profile 213  neither 230  -> glTF 230      (Lean)
mesh 17  profile 213  neither 231  -> glTF 230      (Rust)
```

So the Lean core's coverage matches Rust's 230 payloads, which is the gap §11.30
recorded and §11.33 reported as open.

### Three bugs, and the order they were found in matters

The first two were found by reading the Rust code against the port: the vertex `w`
check used 5% where `src/dma3d/modfile.rs` uses 10% (`marked * 10 < v * 9`), and the
normal check was missing the unit-length test entirely. Fixing both left the reader
accepting nothing, which was the signal that neither was the cause.

The third was the one that mattered, and it was found by **instrumenting**: replaying
`meshAt`'s checks by hand at the chain Rust itself finds — `r4 createBkFloorWire.mod`,
offset 891, `C=6`, `V=4` — passed every one, so the algorithm was right and the code
was not. It was the `distinct` count:

```lean
-- wrong, and this is what the port did:
indices := indices.push i
if !(indices.contains i) then distinct := distinct + 1

-- right, and this is what src/dma3d/modfile.rs does:
-- count the new values, then push
if !(indices.contains i) then distinct := distinct + 1
indices := indices.push i
```

Checking `contains` **after** the push makes it always true, so `distinct` stayed `0`,
`distinct < 2` rejected, and every chain was refused. It is the same class of bug as
the inverted marker count — a condition ported from a HashSet's semantics onto an
array that grows while it is being asked.

The `mod II` anchor (§11.31, §11.32) is ported with it, as `offsetOfMagic`, so the
profile reader starts its record stream at `magic + 10` and the next three 16-byte
steps as well as the 0..8 phase scan.

### What the two halves now agree on

Both read 460 payloads: 17 mesh chains, 213 drawing profiles, 230 written through as
the original bytes with a reported reason. The Lean harness reports
`all checks passed`, and `cargo test` is green.

## 11.35 The thunk address is 0xfd00 past the image, and the only section that
could hold it is compressed

§11.23 concluded the thunk at `0x1c60a4` was "not in this artifact". Parsing the PEF
section headers from the disc image settles how far that was from right.

The container at `0x1ee1460` of `Disney Creazioni in 3D (Italy).bin` (same date-time
stamp as the carved file) declares three sections, and **the carve is complete** — two
of the three ranges and the loader's tile the file exactly:

```text
sec0  kind 0 code    addr 0x0  total 0x194f74  packed 0x194f74  container 0x3510
                     file [0x3510, 0x198484)
sec1  kind 2 data    addr 0x0  total 0x21430  unpacked 0x1f959  packed 0x16a4b
                     container 0x198490   file [0x198490, 0x1aeedb)
sec2  kind 4 loader  addr 0x0  total 0        packed 0x3490     container 0x80
                     file [0x80, 0x3510)
file size 0x1aeedb — sec1's range ends exactly there
```

Loaded contiguously from VA 0 the image ends at **`0x1b63a4`** (`0x194f74` for sec0
plus sec1's `totalSize` `0x21430`). So `0x1c60a4` is **`0xfd00` bytes past the end of
the image** — 64,768 bytes, not the 0x171c9 bytes §11.23 reported against the raw EOF.
§11.23 was measuring the wrong bound: it compared against the file's size rather than
against the image the loader builds.

Two things that gap leaves open, both of which are narrower than "not in this
artifact":

* **`sec1` is compressed.** `0x16a4b` bytes on disc expand to `0x1f959` in memory — a
  ratio of 1.39, the only compressed section in the container — and it sits at the end
  of the file, which is also where the libjpeg routines and the CodeWarrior export
  entries live in the carved bytes. A thunk in a compressed section does not appear in
  the carved file at any offset, which is why the `mtctr r12; bctr` search found
  nothing in the code region.
* **The `bl` displacement may be relocation-patched.** PEF loader relocations are
  stored in the loader section, which the carve *does* hold verbatim; a patched `bl`
  would target a different address than the bytes suggest.

So the honest statement of the blocker is: the address is 0xfd00 past the image under
the contiguous-from-0 mapping, the one section that could hold it is compressed, and
the loader section that could patch the branch is present. All three are now
*measured* rather than assumed, and none of them is a run-time-only observation —
though reading the patched target still needs the loader to run.

## 11.36 sec1 is a symbol table, not a compressed section, and the magic `bl` is still unmapped

§11.35 concluded that sec1 was the "one section that could hold" the thunk because it
was compressed (`0x16a4b` on disc, `0x1f959` in memory). Reading sec1's first bytes
shows that is wrong:

```text
0x198490: 4d 41 62 73 74 72 61 63 74 54 68 72 65 61 64 46   |MAbstractThreadF|
0x1984a0: 50 43 76 50 76 5f 76 3e 5f 5f 46 50 31 36 4d 4d   |PCvPv_v__FP16MAbstractThreadM1|
0x1984b0: 41 62 73 74 72 61 63 74 54 68 72 65 61 64 4d 31   |MAbstractThreadM1|
```

It is a **CodeWarrior symbol table**: 367 printable runs, all mangled C++ names from
the runtime library — `UUFileSystem`, `UUReader`, `UUCheckSum`,
`ParsePathName__12UUFileSystemFRC23UUFileSystemString<256>P11UUDirectoryP22UUFileSystemSt…`.
Nothing compressed would begin with ASCII class names, so the `packedSize` /
`unpackedSize` pair describes the section's stored and in-memory sizes and not a
compressor, and §11.35's "the one section that could hold it is compressed" is
withdrawn.

So the three sections are what their kinds say: **sec0 is the machine code, sec1 is
the runtime library's symbol table, sec2 is the loader**. And the game's own class
names — `ReadData__21MM3DModelResourceFileFv`, which is how the model reader was found
at all — live in **sec0**, embedded beside the code, not in sec1.

That leaves the magic `bl` with no home. The two virtual calls at `0x1633d8` and
`0x1633ec` both decode to `0x1c60a4` by the same arithmetic that correctly decodes the
function's other six `bl`s (`0x179e44`, `0x114ab0`, `0x196e34` twice, `0x17a014`,
`0x17981c`, all of which land on real instructions), so the decode method is sound and
the target is genuinely unmapped: past sec0's end at `0x198484`, past sec1's at
`0x1aeedb`, and past the whole image at `0x1b63a4`.

What is left is the one route that has not been tried, and it is the one this workspace
cannot take: **sec2, the loader section (`0x80..0x3510`), holds the PEF relocations**,
and a `bl` whose target is patched by the loader would land somewhere the raw
displacement does not. Parsing it needs the relocation-entry format, which is not
documented in this repository, and applying it needs the loader to run.

So the blocker is now a single specific thing: the relocation entry that patches
`0x1633ec`. Everything else about the routine is located, and the `u32` record schema
that follows the `mod II` magic is behind this one branch.

## 11.37 The "even bytes mostly zero" group is a parameter table, not geometry

§11.29 and §11.36 flagged 61 payloads whose even bytes are mostly zero as "structured"
and therefore the ones a reader might reach. Reading them thoroughly says otherwise,
and the reason is worth writing down before another reader is attempted against them.

The group is 60 payloads. Their odd bytes — the half that carry data — span the **full
`0x00..0xff` range** in most of them, not the narrow `0x75..0x8c` that one member
(`createModShapeBtProp`, 108 bytes) suggested. Reading them as big-endian `u16` values
gives small integers, but the values are not sorted, not a permutation of a range, and
not a length-prefixed list.

Reading them as little-endian `f32` is where it looked briefly promising: `createMod*
` payload 5 has a clean run of 32 floats from phase 3, `0.25, 0.5, 0.5, 0.0, 0.99999,
…`, and **57 of the 60 have every value inside `[0, 1]`** — which is what a normalized
coordinate or a parameter table looks like. It does not hold up as a *stream*: the runs
are short (8, 9, 11 floats in the examples), several are entirely zeros, and the group
has no common phase. A stream that is mostly zeros read as floats is a zero-padded
table being read at the wrong width, not a coordinate list.

So the shape is: **a table of small values stored at alternating byte positions**, with
zero fill. That is a parameter table, an index table, or a packed enum — the kind of
record the game reads to configure a UI element or a material, not to describe its
outline. `createModShapeBtProp`, `fileCreationSlot`, `createModShrMaterial*` are all
configuration records, which is consistent with what the names already said.

### What this means for the residue

The four shapes of §11.29 now measure as:

| shape | count | what it is |
| --- | --- | --- |
| even bytes mostly zero | 60 | a small-value table, not geometry |
| odd bytes mostly zero | 20 | the same, at the other parity |
| mid entropy, mixed | 69 | not determined |
| high entropy | 81 | no readable structure |

Of the 460 payloads, 230 convert. The 60 + 20 of the table shape are **not convertible
to glTF**, and a reader for them would have to emit something other than geometry —
a JSON parameter dump beside the verbatim bytes, which is what the modern output for a
configuration record honestly is. That is a different output target from the one this
project set for models, and it needs a decision, not a measurement.

The goal's second half is not met: 230 of 460 glTFs, and 80 of the remaining 230 are
now known to be configuration records rather than models.

## 11.38 The 48 `mod II` records are written as JSON+text, and 278 of 460 have output

`parse_magic_record` reads the record stream the magic announces, and
`write_record_json` writes it as the same JSON+text format the scripts decoder
emits. The run now reports:

```text
models: 1165   ambiguous: 696   total: 1165   failed: 182
glTF 230   json 48   previews 230   png 935   .mod 182
```

**17 meshes + 213 profiles + 48 records = 278 of 460 payloads produce modern output**,
against 230 before. The Lean reader agrees exactly — `mesh 17, profile 213, record 48,
other 182` — and the harness reports `all checks passed`.

### Why JSON+text and not a glTF

These 48 are **data records, not geometry**. Writing them as a glTF line loop would
invent a shape the payload does not carry, which `AGENTS.md` forbids as a guess. The
document carries what is measured and says what is not:

```json
{
  "name": "AnimGifS.mod",
  "what this is": "a mod II record stream the mesh reader and the drawing-profile reader both reject",
  "meaning": "not determined: no position in the record stream holds a constant across payloads (docs/DMA3D.md 11.32), so the record schema is not read from the bytes",
  "measured": {
    "magic offset": 156,
    "record type": 16,
    "stream offset": 166,
    "pairs": [1, 1, 466, 0, 1, 0, 1, 0, 2, 0, 0, 0, ...],
    "floats that read as numbers": [0, 0, 0, 0, 0, 1, ...]
  }
}
```

The `floats` are **filtered to the ones that read as numbers**. A denormal is the
signature of a misaligned read — a shifted boundary landing in a neighbouring value's
mantissa produces about `1e-40`, which no field in this game takes — so including them
would report noise as data. §11.26 established that rule for the profile reader and the
same rule applies here; the unit tests assert it in both halves.

The `record type` is the `u32` the game's own reader treats as a version: `16` in 47 of
the 48, `15` in one. It is recorded and not interpreted, because §11.20 measured that
it is neither a byte count nor an entry count of what follows.

### What the Lean half does and does not write

The Rust converter is the writer of modern models in this pipeline
(`src/cli.rs::convert_models`), so the Lean path **counts** the records and reports the
ones that fit no reader, but does not write them. A first attempt wrote from both halves
and the run reported `models: 1216` where the converter writes 1165 — the same double
count §11.26 recorded for the glTF path, and the same reason for not repeating it.

### What is left

**182 payloads** fit no reader, and §11.29's shape census says they are 81 of no
readable structure, 69 of mid entropy, and the 32 remaining of the alternating-zero
shape that §11.37 measured as a small-value table rather than geometry. The goal's
second half is not met at 278 of 460, and the honest description of the remainder is a
measured shape with no reader yet, not an unknown format.

## 11.39 The parameter tables are written as JSON+text, and 287 of 460 have output

`parse_param_table` reads the shape §11.37 measured — small values at alternating byte
positions with zero fill — and `write_param_json` writes it as JSON+text. The run now
reports:

```text
models: 1165   ambiguous: 696   total: 1165   failed: 173
glTF 230   json 57   previews 230   png 935   .mod 173
```

**17 meshes + 213 profiles + 48 records + 9 tables = 287 of 460** payloads produce
modern output, against 278 before. The Lean reader agrees exactly — `mesh 17, profile
213, record 48, param 9, other 173` — and the harness reports `all checks passed`.

### The thresholds are stated, not tuned

§11.37 measured the shape as *small values at alternating byte positions with zero
fill*. A payload is accepted when **at least 80% of one parity is zero, at least 95% of
the other is below 128, and there are at least eight values**, and the fill parity is
**measured** rather than assumed, because §11.37 found both parities in the container.

The thresholds are deliberately not loosened to fit more payloads. At 70/90 the reading
takes 13 payloads and at 60/85 it takes 19, and the extra ones are mixtures whose fill
is only 51-60% zero — reading them as this shape would be the guess `AGENTS.md`
forbids. So nine payloads convert and the rest are reported.

### The first documents were not valid JSON

The first version wrote strings with Rust's `\` line continuation, which puts a
backslash **and a newline** into a JSON string literal, and wrote the percentages
unquoted (`"fill is zero for": 89% of its bytes` — not a JSON value). Both were caught
by reading the output back with `json.load`, which is the reader that will judge it,
and `gltf::param_json_tests::a_written_table_document_is_valid_json` now pins both.

That is the second time this turn a reader produced a document it could not read back,
and the reason is the same in both cases: the output was written by hand rather than
checked against the reader. The lesson is already in `AGENTS.md` — every new decoder
ships with a test that reads its output back — and this section is where it was applied.

### What is left

**173 payloads** fit no reader. §11.29's census says they are 81 with no readable
structure and the rest of mid entropy, and the alternating-zero shape now takes only
the payloads whose fill is genuinely dominant. The goal's second half is not met at 287
of 460.

## 11.40 The texture-name tables are written as JSON+text, and 306 of 460 have output

Measuring the 173 unconverted payloads for a repeat-period — which nobody had done —
found the group that mattered. **11 of them carry an exact period of 32 explaining 96%
of their bytes**, and reading one settles what they are:

```text
+  0: 74 65 42 74 46 69 6c 6c 2e 31 38 2e 54 47 41 00 cc cc cc cc cc cc cc cc cc cc cc cc 63 72 65 61
+ 32: 74 65 42 74 46 69 6c 6c 2e 31 39 2e 54 47 41 00 cc cc cc cc cc cc cc cc cc cc cc cc 63 72 65 61
```

`createBtFill.18.TGA`, `createBtFill.19.TGA`, `createBtFill.20.TGA` — a **table of
texture names** in 32-byte records padded with `0xcc` fill, 30 of the 32 columns
constant because only the number in the name varies. `parse_name_table` reads them and
`write_name_json` writes them:

```json
"measured": { "count": 11, "names": ["createBtSelect.25.tga", "createBtSelect.26.tga", ...] }
```

The run now reports:

```text
models: 1165   ambiguous: 696   total: 1165   failed: 154
glTF 230   json 76   previews 230   png 935   .mod 154
```

**17 meshes + 213 profiles + 48 records + 9 tables + 19 name tables = 306 of 460**,
against 287 before. The Lean reader agrees exactly — `mesh 17, profile 213, record 48,
param 9, names 19, other 154` — and the harness reports `all checks passed`.

This is not a new layout. `src/dma3d.rs::texture_names_in` has always read exactly this
field out of *mesh* payloads to embed their materials; what changed is that the reader
now looks for it in the payloads it did not look at.

### The threshold, and the noise it rejected first

A name is a run of printable ASCII terminated by a NUL, **at least eight characters**
long. At four the reading took 39 payloads, and 20 of those carried nothing but
`DDD?`, `fff?`, `UUU?`, `333?` — four printable characters and a NUL, which is noise in
a 32-byte record rather than a name. Eight takes the 19 payloads carrying real ones and
reports the rest, and `name_table_tests::four_printable_characters_are_not_a_name` pins
it.

### One field I removed rather than guessed

The first version carried `first_is_truncated`, set by asking whether the payload's
first byte is the name's first byte. That test is unfalsifiable: it is true both when
the name is cut and when it is not, because a payload that begins with a name begins
with its first byte either way. Some of these payloads *do* start inside a name —
`teBtFill.18.TGA` for `createBtFill.18.TGA`, because the container's slice starts three
bytes into a record — and the bytes cannot tell that from a whole name. So the field is
gone and the names are recorded verbatim. `AGENTS.md` sets a payload aside rather than
guessing it, and a struct field that can only be filled by guessing is the same mistake
one level down.

### What is left

**154 payloads** fit no reader, and the entropy figure §11.29 reported as a census is
wrong: measured directly, the 173 spread continuously from 1.1 to 6.2 bits per byte
rather than falling into buckets, and 48 of them have no period explaining even 30% of
their bytes. The goal's second half is not met at 306 of 460.

## 11.41 The `f32` value streams are written as JSON+text, and 345 of 460 have output

The probe that found r5's `0.25, 0.5, 0.0, 0.99999` in an earlier session was
abandoned mid-measurement. Finishing it over the 154 payloads that fit no reader
finds **39 carrying a run of little-endian `f32` values that is long (16 or more) and
spread (more than 0.5 between the extremes) with no slot that is constant** — and that
last part is exactly why the drawing-profile reader rejects them, since it requires
one coordinate to be the same in every record to call a stream a planar profile.

These are the payloads whose points are not in a plane. Four are
`createModDrawProfileOct1` through `Oct4`, the same family as 213 payloads that do
convert, and the rest carry large ranges (`-1050` to `495`) that read as world
coordinates. `parse_value_stream` reads them and `write_stream_json` writes them:

```text
models: 1165   ambiguous: 696   total: 1165   failed: 115
glTF 230   json 115   previews 230   png 935   .mod 115
```

**17 meshes + 213 profiles + 48 records + 9 tables + 19 name tables + 39 streams =
345 of 460**, against 306 before. The Lean reader agrees exactly — `mesh 17, profile
213, record 48, param 9, names 19, streams 39, other 115` — and the harness reports
`all checks passed`.

### What the document says, and what it refuses to say

It records the values, the phase the run was found at, and the range. It states that
the **field layout is not determined** — whether the values are `xyz` triples, `xy`
pairs, or a per-record layout with something else in it is not readable from the
bytes. That is the same honest form the record stream and the parameter table take,
and the reason is the same: asserting a stride would invent geometry the payload does
not carry, which `AGENTS.md` forbids.

The reader is also checked the other way round:
`a_stream_with_no_constant_slot_is_not_a_profile` asserts the profile reader refuses
the payloads this one accepts, so the two cannot both claim the same bytes.

The plausibility rule is the one the other readers use, for the reason §11.26 measured:
a denormal is a misaligned read landing in a neighbouring value's mantissa, and
writing one would report noise as data.

### What is left

**115 payloads** fit no reader, and the entropy figure §11.29 reported as a census is
wrong. Measured directly over the 154 that were left then, the spread is continuous
from 1.1 to 6.2 bits per byte rather than falling into buckets, and 48 of them had no
period explaining even 30% of their bytes. The goal's second half is not met at 345 of
460.

## 11.42 `f64` is not a sub-format here, and the 115 carry no strong period

The two probes nobody had run over the 115 payloads that fit no reader both come back
negative, and the negatives are measured rather than inferred.

### `f64` streams: the runs are exponent-field noise

A naive filter finds 14 payloads with a run of six or more finite little-endian
`f64` values spread over more than half a unit, and 51 with a majority of non-zero
values in a run. Reading the payloads those runs come from shows what they are:

```text
r156 (24,328 B) at phase 4:
  272673532539941065451609642541870716047106739442726423206137885783213222311060...
  416066793120265071543356310325975717798970860059261045619978588827178750308333...
r247 createModShapePrimCone.mod (20,358 B) at phase 6:
  first 8 values: -0.000, -0.000, -0.000, -0.000, -0.000, -0.000, -0.000, -0.000
```

`2.7e308` is an exponent field, not a coordinate, and a run of negative zeros is
padding. Both payloads carry the alternating `00 XX` byte shape §11.37 already named,
and reading eight bytes of it as a double produces those. So `f64` is not a sub-format
in this container, and no reader is written for it.

### Period: nothing left with a strong one

Over the 115, the smallest period that explains at least 90% of the bytes:

| | count |
| --- | --- |
| ≥ 90% | **0** |
| 60–90% | 11 |
| under 60%, or too short | 104 |

The 19 payloads that carry a texture-name table have a period of 32 at 96% agreement —
that is how §11.40 found them — and they are already read. What is left has **no period
explaining even 60% of its bytes** in 104 of 115 cases.

So the probes that found every reader this project has are exhausted against the
residue: no mesh chain, no constant-slot `f32` stream, no `mod II` magic, no parameter
table, no texture-name table, no `f32` value stream, no `f64` stream, no strong period.

### What that means, stated plainly

The 115 payloads carry bytes that no probe tried has found a structure in. Two things
follow, and both are in the run's output rather than in an argument:

* the goal's second half is **not met** at 345 of 460, and
* the remaining step is not another threshold. A reader written now would be a
  threshold tuned until the payloads fit, which is the guess `AGENTS.md` forbids —
  and the payload bytes do not supply the schemas the readers have needed so far.

The one route that would settle it is the same one §11.23, §11.24, §11.35 and §11.36
named: the relocation entry that patches the `bl` at `0x1633ec` in `DMA3D.exe`, which
is what tells the game's own reader what these payloads are. It needs the PEF
relocation format applied by the loader, or a debugger break to read `r12` at run
time. Neither is a disassembly step, and neither has been done.

## 11.43 sec2's relocations carry no raw entry for the branch, and that is where the
static route ends

§11.23 and §11.42 named an unparsed thing: the relocation entries in sec2 that would
patch the `bl` at `0x1633ec`. Parsing them was not done, so the claim "the `bl` is
patched" was never more than an inference. This section parses what can be parsed from
the bytes and reports what it establishes.

sec2 is the loader section, file `[0x80, 0x3510)`, 13,456 bytes. The `bl` sits at file
offset `0x1633ec`, which is section-0 offset **`0x15fedc`**, and it encodes a
displacement of `0x62cb8` to `0x1c60a4`. A relocation that patches a branch has to name
one of those four numbers, so all four were searched for in sec2:

| searched for | hits in sec2 |
| --- | --- |
| section-0 offset `0x0015fedc` (BE u32) | **0** |
| section-0 offset `0xfedc` (BE u16) | **0** |
| file offset `0x001633ec` (BE u32) | **0** |
| displacement `0x62cb8` (BE u32) | **0** |
| the instruction's own bytes `48 06 2c b9` | **0** |

So no *raw* relocation entry names the branch. That is a real measurement, and it is
**inconclusive rather than negative**: the PEF relocation stream is a compact encoding
(two-byte entries with opcodes and run-lengths, not raw offsets), so a patched branch
would not necessarily carry its target as a literal. Decoding the entry stream needs
the opcode format, which this repository does not document and which I have not got a
source for.

The relocation *header* does not resolve it either. Read as big-endian u32, sec2 opens
with `1, 0x6f40, 0xffffffff, 0, 0xffffffff, 0, 7, 0x159, 1, 0x650, 0x2380, 0x3488, 1,
0, 0, 0`, and `0x2380` (9,088) and `0x3488` (13,448, eight bytes short of the section's
own 13,456) both look like offsets into sec2 — but the header's field layout is not
determined from these bytes, so which number is the entry count and which is the first
entry's offset cannot be read off them. Guessing at a relocation format is exactly what
`AGENTS.md` forbids, so it is not guessed.

What is left is the one route that does not need the format: a run-time observation of
the branch. A PowerPC debugger that stops at `0x1633ec` and reads `r12` after
`lwz r12, 0x50(r31)` gives the routine's address directly, because that instruction
loads the vtable pointer the very next instruction indexes. That is the whole of what
the 115 payloads are waiting on, and it is not a step this workspace can take.

## The state of the goal

| | |
| --- | --- |
| textures, modern and `--original` | **met** — 705/705, nothing reported, byte-exact |
| models, `--original` | **met** — 460/460 byte-exact, checked in Lean |
| models, modern | **345 of 460** — 17 meshes, 213 profiles, 76 JSON+text records |
| models remaining | **115** payloads with no probe-able structure |

Every byte of the 3D game's shipped resources is recovered byte-exactly. The modern
conversion of the last 115 needs the schema that lives in a routine this workspace
cannot reach statically and cannot run.

## 11.44 The relocation stream is found, and its entries do not reach the branch

§11.43 concluded the raw search was inconclusive because the stream is a compact
encoding. Decoding what can be decoded without guessing: sec2 holds **6,728 BE `u16`
words, 2,084 of them with a nonzero opcode nibble**, and the structure shows where the
relocation instructions are.

From `0x67e` the words come in a strict 4-byte pattern — an increasing word followed by
a constant `0x0200`:

```text
+0x67e: 0x100b   +0x680: 0x0200
+0x682: 0x101c   +0x684: 0x0200
+0x686: 0x1032   +0x688: 0x0200
+0x68a: 0x1044   +0x68c: 0x0200
+0x68e: 0x1051   +0x690: 0x0200
```

and the same shape recurs through the section to `0x2ddc`. The increasing word runs
**`0x100b` to `0x1fed`** across those entries, with `0x41` (1,534 occurrences) and
`0x83` (484) as the section's two most common words — a fixed relocation repeated for
many similar sites, which is the shape a relocation stream has.

### What that establishes, and what it does not

The values in the whole stream span **`0x100b` to `0x1fed`** — at most 8,173. The
branch being chased is at section-0 offset **`0x15fedc` = 1,441,500**, which is 87% of
the way into a 1,658,740-byte code section. **No entry in the stream has a value
anywhere near it.**

Two readings of that, and the bytes do not choose between them:

* if those values are section-0 offsets, the loader relocates only the opening 8 KB of
  the code — the section headers and the C++ runtime's construction tables, which is
  what a PEF normally relocates most heavily — and the branch is simply not patched, in
  which case `0x1c60a4` really is its target and my instruction decode is wrong
  somewhere I have not found;
* or the values are not offsets at all, in which case I have the entry format's
  semantics backwards and the measurement says nothing about the branch.

Distinguishing them needs the documented PEF opcode semantics — what bits 15–12 select,
what the low field carries, and what the repeated `0x0200` means. This repository does
not hold it and I do not have a source for it, and inventing one is the guess
`AGENTS.md` forbids.

So this is the third consecutive turn that has narrowed the same branch without
reaching it, and the honest summary is that the step is not another measurement: it is
the PEF relocation format, or a PowerPC debugger break at `0x1633ec` to read `r12`
directly. Both are outside what this workspace can do.

## 11.45 The PEF relocation format is now documented in the repository

Three sessions named "the undocumented PEF relocation format" as the blocker. It is
documented — in *Inside Macintosh: Mac OS Runtime Architectures*, Chapter 8 — and
three web lookups retrieved it. It is written here in full so the next session does not
have to fetch it again.

### The relocation header table

```c
struct PEFLoaderRelocationHeader {   // 12 bytes each
    UInt16 sectionIndex;            // which section these relocations apply to
    UInt16 reservedA;               // must be 0
    UInt32 relocCount;              // number of 16-bit relocation BLOCKS (not
                                    // instructions — most are 2 bytes but some
                                    // are 4, so instructionCount <= relocCount)
    UInt32 firstRelocOffset;        // byte offset from the start of the
                                    // relocations area to this section's first
                                    // relocation instruction
};
```

Relocations are grouped by section and executed **in order**; state is kept in
pseudo-registers, so a decoder must replay the stream rather than sample it:

| register | initial value |
| --- | --- |
| `relocAddress` | base address of the section being relocated |
| `importIndex` | 0 |
| `sectionC` | memory address of section 0 minus its `defaultAddress` |
| `sectionD` | memory address of section 1 minus its `defaultAddress` |

### The instruction set — opcodes are the high-order 7 bits of the first block

| 7-bit opcode | instruction | operand layout |
| --- | --- | --- |
| `00xxxxx` (`0x00`–`0x1F`) | `RelocBySectDWithSkip` | `skipCount*4` byte skip, then `relocCount` contiguous words relocated |
| `0100000` (`0x20`) | `RelocBySectC` | `runLength` words (stored as length-1) |
| `0100001` (`0x21`) | `RelocBySectD` | `runLength` words |
| `0100010` (`0x22`) | `RelocTVector12` | `runLength` 12-byte items (C to word 0, D to word 1) |
| `0100011` (`0x23`) | `RelocTVector8` | `runLength` 8-byte items |
| `0100100` (`0x24`) | `RelocVTable8` | `runLength` 8-byte items (D to word 0 only) |
| `0100101` (`0x25`) | `RelocImportRun` | `runLength` words, `importIndex` advanced by `runLength` |
| `0110000` (`0x30`) | `RelocSmByImport` | `index`; 1 word |
| `0110001` (`0x31`) | `RelocSmSetSectC` | `index` |
| `0110010` (`0x32`) | `RelocSmSetSectD` | `index` |
| `0110011` (`0x33`) | `RelocSmBySection` | `index`; 1 word |
| `1000xxx` (`0x40`–`0x47`) | `RelocIncrPosition` | `offset` bytes (stored as offset-1) |
| `1001xxx` (`0x48`–`0x4F`) | `RelocSmRepeat` | `blockCount-1`, `repeatCount-1` |
| `101000x` (`0x50`/`0x51`) | `RelocSetPosition` | **4 bytes**; 26-bit section offset |
| `101001x` (`0x52`/`0x53`) | `RelocLgByImport` | 4 bytes; 26-bit symbol index |
| `101100x` (`0x58`/`0x59`) | `RelocLgRepeat` | 4 bytes; `repeatCount` (not -1), `blockCount-1` |
| `101101x` (`0x5A`/`0x5B`) | `RelocLgSetOrBySection` | 4 bytes; 22-bit section index |

Every relocation acts on **4-byte words**, so `relocAddress` advances in fours.

Two facts from the spec that bear directly on the earlier dead ends:

* *"If the Code Fragment Manager can place the instance in the preferred memory
  location, the load-time and link-time addresses are identical and no internal
  relocations need to be performed."* Section headers may declare
  `defaultAddress = 0` and still never be moved, which is consistent with §11.35's
  finding that all three of `DMA3D.exe`'s sections declare address 0.
* The stream is *not* a list of offsets. §11.44 read a 4-byte pattern of an increasing
  word plus `0x0200` and assumed the increasing word was an offset; it is an opcode
  block, and `0x0200` is opcode `0x01` = `RelocBySectDWithSkip` with both counts
  stored as one-less.

### What this turns the earlier dead ends into

Decoding with the documented opcodes, sec2 (file `[0x80, 0x3510)`, 13,456 bytes) holds:

* **6,728 BE `u16` blocks**, of which the opcode histogram is dominated by
  `0x00` (`RelocBySectDWithSkip`, 3,600 blocks), `0x01` (675), `0x02` (99);
* **17 `RelocSetPosition` blocks** — the only instructions that can set
  `relocAddress` to an arbitrary section offset, at file offsets
  `0x84a, 0x84e, 0x89e, 0x1168, 0x118a, 0x11e0, 0x11e6, 0x1244, 0x124e, 0x1a94,
  0x1ad6, 0x1aea, 0x1afe, 0x1b72, 0x2892, 0x2cf0, 0x2dd0`;
* 10 `RelocLgByImport`, 16 `RelocLgRepeat`, 10 `RelocLgSetOrBySection`.

So the branch at section-0 offset `0x15fedc` is *reachable* by a decoder that replays
`relocAddress` from zero through the stream. That is the test to run, and it is now a
mechanical one rather than an unknown: walk the stream, keep `relocAddress`, and record
every byte range it touches.

### What is still not done

The operand split of `RelocSetPosition` is not yet right. Taking the low 9 bits of the
first block as the offset's high bits gives offsets like `0x11 9a12d` — about 18 MB,
where section 0 is 1,658,740 bytes — and taking the low 10 bits gives the same set
shifted, which is also out of range. So the bit split is `(something other than
9 or 10) << 16 | second block`, and which split is right is not determined by the
bytes. The spec's figure is an image that the text fetch did not carry.

That is the one remaining unknown, and it is now a single question rather than a
format: **where do the 26 offset bits sit across the two blocks?** Answering it needs
the spec's figure, or one known-good sample — a `RelocSetPosition` whose target offset
can be read back from the code — and then the stream can be replayed to settle whether
the `bl` at `0x1633ec` is patched at all.

## 11.46 The `bl` at `0x1633ec` is not relocated, so it was a misaligned read

Ghidra's `RelocSetPosition` source (`Ghidra/Features/Base/src/main/java/ghidra/app/util/bin/format/pef/RelocSetPosition.java`) settles the operand split that §11.45 left open, and settling it settles the branch:

```java
RelocSetPosition(BinaryReader reader) {
    int value = reader.readNextShort() & 0xffff;
    opcode = ((value & 0xfc00) >> 10) & 0x3f;   // the high SIX bits
    offset =  (value & 0x03ff) << 16;           // the low TEN bits
    offset |=  (reader.readNextShort() & 0xffff);
}
```

So the opcode is the high **six** bits — not the seven §11.45's reading of *Inside
Macintosh* implied — and the offset is the **low ten** bits shifted by 16. The two
splits §11.45 tried (nine and ten) were the two candidates, and Ghidra's source
confirms the second; the reason §11.45's values were out of range is not the split but
that it had not yet confirmed the opcode width either.

### Applying it: two of the seventeen candidates are real

Section 0 is `0x194f74` (1,658,740) bytes, so a `RelocSetPosition` whose offset exceeds
that is not a section-0 relocation at all. Recomputing all seventeen candidates with
the documented split:

| | |
| --- | --- |
| offsets past section 0 | **15** |
| offsets inside section 0 | **2** — `0x1038b8` and `0x1ed3c` |
| equal to the branch's `0x15fedc` | **0** |

The fifteen are **mid-instruction false positives**: §11.44 read sec2 as a flat array
of `u16` words, and a stream whose instructions are two bytes each and four bytes each
and which repeats blocks has words that are not instruction starts. Filtering a flat
array by opcode finds opcodes inside operands. That is the same mistake §11.44 made
reading `0x0200` as an offset, and §11.45's opcode histogram is wrong for the same
reason.

The two real anchors are `0x1038b8` (1,069,752) and `0x1ed3c` (126,264). For the branch
at `0x15fedc` (1,441,500) to be relocated, `relocAddress` would have to be advanced
from `0x1038b8` by 371,748 bytes after the second anchor — and every instruction in the
set other than `RelocSetPosition` advances it by at most a few hundred bytes, so no
sequence of them spans that gap.

**So the relocation stream does not reach section-0 offset `0x15fedc`, and the `bl` at
`0x1633ec` is not patched.** Its encoded target of `0x1c60a4` stands, and since §11.24
established that no mapping of the three PEF sections lands inside `0x1c60a4`, the
reading cannot be right — which means the `bl` itself is a **misaligned instruction
read**, exactly like the fifteen false positives, and not a relocated dispatch.

Four turns of tracing a branch turn out to have been tracing a byte pattern inside an
operand. §11.9's "the payload-consuming routine at `0x1633ec`" rests on that read, and
this section withdraws it.

### What this changes about the 115 payloads

The 115 are no longer waiting on a relocated dispatch. Everything known about them is
what the probes measured — no period ≥ 90%, no constant-slot `f32` stream, no magic,
no `u16` table, no `f64` stream, no name table — and the one remaining lead is the
executable's own reader, which §11.24 established is reachable in a section whose
dispatch thunk is at `0x1c60a4`. That conclusion now stands without the relocation
argument, because the relocation argument was wrong.

The goal's second half is still not met at 345 of 460, and it is now not met with one
fewer lead rather than one more.

## 11.47 The 3D container's name table is 520 slots, and Rust reads only 460

§11.15 and §11.29 classified the unconverted payloads by name, and this section
corrects the base that classification used.

### What the name table actually is

The `Models` container's resource names are **32-byte NUL-padded ASCII slots**
beginning at **`0x75a` (1882)**, which is `0x24 + 4*461 + 2` — the offset table's
extent plus its two-byte padding. The first payload begins at `0x486e`, so the table
holds:

| | |
| --- | --- |
| slots between `0x75a` and `0x486e` | **520** |
| slots holding a valid `.mod` name | **390** |
| slots holding something else | **130** |

`src/container.rs` reads **`32 * n` bytes with `n = resource_count = 460`**, so it
reads the first 460 slots and stops. That is why `name(455)` through `name(459)`
return garbage (`"\u{fffd}O\u{fffd}O\u{7}\u{1e}"`, `""`, `".mod"`) while `name(0)`
through `name(100)` are correct: the reader stops short of the end of the real table,
and the last 130 slots are never read at all.

The 130 slots that hold no name are the same payload-shape bytes as the regions
§11.37 measured as parameter tables — the region is a name table with other records
interleaved, or it is a table with 130 empty slots, and the bytes do not say which.

### What this does and does not fix

It does **not** mean the 115 unconverted payloads are misclassified by name. The
families the names gave them (§11.15, §11.29) stand, because the names that are
readable are the ones those sections used, and the 24 payloads `names115` reported as
unnamed are the ones whose slot Rust never reaches or whose slot holds no name.

It does mean **one thing that is now known wrong and was asserted**: §11.29's census of
the 115 by family counted "(unnamed)" as a family of 24, and that count is an artefact
of reading 460 of 520 slots rather than a measurement of the payloads.

### What the measurement is for

The names are the only handle the 115 payloads have on *what they are*, and the handle
is now known to cover 390 of 460 resources rather than all of them. Reading the
remaining 70 needs either the container's own reader in `DMA3D.exe` — which §11.17
located at `0x1632c8` and §11.19 found unreachable — or a decision about what the 130
non-name slots hold.

The goal's second half is still not met at 345 of 460, and the residue is now
characterised as **115 payloads, 24 of which cannot be named because the name table is
read short**.

## 11.48 The name table is interleaved, so the fix is not safe to make blind

§11.47 measured the `Models` container's name table as 520 × 32-byte slots from
`0x75a`, of which 390 hold a `.mod` name, and left the reader reading only `32 * n`.
Measuring the *spacing* between the name runs shows why that is not a bug with a
one-line fix.

### What the spacing says

| | |
| --- | --- |
| printable name runs in `0x75a..0x486e` | 467 |
| runs spaced exactly 32 bytes apart | 446 |
| runs spaced anything else | 20 |

The first five runs are at `0x75a, 0x77a, 0x79a, 0x7ba, 0x7da` — slots 0..4, 32 bytes
apart. Then comes a **310-byte gap** to `0x930`, and the runs continue 32 bytes apart
from there. Later gaps are 160, 18, 43, 89, 48, 86, 176 bytes.

A 310-byte gap is 9.7 slots. So the region is **not a contiguous block of name slots**:
it is names interleaved with records that are not names, and the drift accumulates —
after the first gap the run at `0x930` sits at slot 14 by straight-line counting while
the resource it belongs to, if the mapping is sequential, is slot 6.

### Why the fix was not made

`src/container.rs` reads `32 * n` bytes and indexes names by resource index. To read
more names correctly, one must know **which slot belongs to which resource**, and the
spacing measurement says that mapping is not sequential — it has 20 gaps whose meaning
is not in the bytes. Options:

* assume the names are sequential and the gaps are empty slots — then `name(455)` would
  be read from a slot that belongs to a different resource, and every name from the
  first gap onward would be shifted;
* derive the mapping from the executable's own reader, which §11.17 located at
  `0x1632c8` and §11.19 established is not statically reachable.

The first is a guess that would silently rename 460 resources and change every output
filename. `AGENTS.md` forbids a guess of exactly this shape — a payload that matches no
known layout is reported, not made to fit — and a name that names the wrong resource is
worse than a missing name, because it is *wrong* rather than *absent*.

### What this leaves

The 115 unconverted payloads are still 115, and 24 of them still cannot be named by the
current reader. The reader is not made worse by leaving it: `name(0)` through
`name(100)` are correct today, which is what §11.15 and §11.29 classified by.

What the measurement adds is that the name table is **interleaved with non-name
records**, which is the same structure the payload region itself has (§11.37's
alternating-zero tables, §11.31's magic-then-u16 streams) and which the executable's
reader decodes. So the 115 are not a bucket of unnamed leftovers: they are the
resources whose names live in the 130 non-name slots, and reading both needs the
container's own reader.

The goal's second half is not met at 345 of 460, and the residue is now characterised
as payloads whose name records are interleaved with records this repository cannot yet
read.

## 11.49 Twenty-five payloads carry little-endian `u16` runs of small values

The three unprobed structures the reviewer named — per-record tag/width encodings,
odd-stride integer streams, delta-encoded runs — were measured over the 115. The
middle one is the one that finds something.

### What was measured

Reading each of the 115 payloads for its longest run of little-endian `u16` values
below 4096:

| structure probed | payloads with a run ≥ 16 |
| --- | --- |
| big-endian `u16` (the shape §11.37 read) | 0 new |
| **little-endian `u16`** | **25** |
| little-endian 24-bit | 0 |
| big-endian 24-bit | 1 |

And reading the runs themselves splits the 25 into two groups with different shapes.

**`createModBtSize*`** — `createModBtSize03` (160 values), `createModBtSize04` (213),
`createModBtProp` (57) — runs whose values sit in a narrow band:

```text
createModBtSize03:  117, 118, 119, 120, 118, 120, 121, 118, 121, 122, ...
createModBtSize04:  115, 115, 152, 117, 152, 123, 117, 153, 154, 119, ...
createModBtProp:    31, 25, 27, 32, 14, 13, 13, 29, 32, 33, 34, 14, 15, ...
```

Those bytes are `75 00 76 00 77 00 78 00` — an ASCII character followed by a zero,
which is exactly the alternating-zero shape §11.37 named as a table of small values
with zero fill. So this group is the *same* shape §11.37 already measured, reached by a
different probe, and the reader §11.39 wrote for it rejects them because its 80% fill
threshold does not hold: these payloads carry a value in *every* slot, so there is no
fill to measure.

**`r19 nter.mod` (62 values), `r156` (312)** — runs that are two interleaved
increasing sequences:

```text
r19:  83, 85, 84, 49, 86, 50, 87, 88, 89, 88, 90, 89, 91, 92, 93, 92, 94, 93, 53, 95
      -> 83, 84, 86, 87, 89, 90, 91, 93, 94, ... and 85, 49, 50, 88, 88, 89, 92, 92, 93, 95
```

Two monotone columns is a table, not noise, and it is the shape a **delta-encoded** run
has. But the second column breaks monotonicity (`85, 49, 50, 88`), so it is not a clean
delta run either, and reading it as one would invent the deltas.

### What this does and does not give

It gives 25 payloads whose bytes are `u16` with small values, and it re-derives §11.37's
shape from a second direction. It does not give a reader: the `createModBtSize*` group
is already the shape §11.39 reads, and §11.39's threshold rejects them because they have
no fill — which is a *measured* reason, and lowering the threshold would admit payloads
whose slots are not all small.

The `r19`/`r156` group is the one genuinely new shape, and its second column is not
monotone, so no delta reading is established.

The goal's second half is not met at 345 of 460. What this turn adds is that the third
probe the reviewer named runs, and its result is that the payloads it finds are either
already-read shapes or shapes whose columns are not monotone.

## 11.50 The payloads are not UTF-16 text, and the shape is §11.37's after all

§11.49 measured 25 payloads carrying little-endian `u16` runs of small values, and
two of them (`r19`, `r156`) deinterleaved into near-monotone columns whose bytes began
`00 5e 00 54 00 5e 00 44 00 5f 00 45` — an ASCII character with a zero byte between
each, which is the shape UTF-16BE text has. This section tests that reading.

### The test

For each of the 115 unconverted payloads: count the odd-indexed bytes that are zero
(UTF-16BE's signature) and require at least 90% of them, then read the even bytes as
characters.

| | |
| --- | --- |
| payloads passing the 90% odd-byte-zero test | **4** |
| of those, containing readable text | **0** |

### What the four actually are

`r156` decodes to `uvwxvxyvyzvu{v{|v|}v}wv~zy`, `r19` to `ss\u{98}u\u{98}{u\u{99}\u{9a}w`, another to `\u{c}\u{1}\u{1}\u{3}¯ffÍ\u{8c}ffÂ²`. These are character sequences in the printable and control ranges, not words — `abcdefghijklmnopqrstuvwxyz` appears inside `r19`'s, which is a *range*, not a word. A range of consecutive characters is a codepoint table, and a codepoint table with a zero byte between its entries is the §11.37 alternating-zero shape read as text.

So the UTF-16BE reading is withdrawn, and the four are §11.37's shape reaching a
different probe. §11.39's reader rejects them because its 80% fill threshold does not
hold — these carry a value in every slot, so there is no fill — which is a measured
reason, not an oversight.

### What this costs and what it does not

It costs one hypothesis that §11.49 left open, and it costs it with a count rather than
a reading of the two payloads that motivated it. It does not convert any payload: the
four are already the shape §11.39 measured, and the §11.39 threshold that rejects them
is doing so for a stated reason.

The goal's second half is not met at 345 of 460, and the residue is now down to
**111 payloads**, four of which are §11.37's shape behind a fill threshold that does not
fit them and 107 of which no probe reaches.

## 11.51 `r19` and `r156` are not the alternating-zero shape, and §11.50 said they were

§11.50 concluded that the four UTF-16BE-looking payloads were "§11.37's
alternating-zero shape read as text". Measuring the fill directly says that is
wrong, and the reviewer's next step was built on it.

### The measurement

For each payload: the fraction of odd-indexed bytes that are zero (the shape's
signature), the fraction of even ones, and the fraction of each half below 128.

| payload | odd bytes zero | even bytes zero | even < 128 | odd < 128 |
| --- | --- | --- | --- | --- |
| `r19 nter.mod` (4,028 B) | **17%** | 21% | 64% | 57% |
| `r156` (24,328 B) | **33%** | 25% | 65% | 66% |
| `r6` (137 B) | 80% | 79% | 89% | **100%** |
| `r154` (376 B) | **90%** | 51% | 94% | **98%** |

`r19` and `r156` have **under a third** of their bytes zero in either half. The shape
§11.37 named needs at least 80% of one parity zero, and §11.39's reader requires it.
These payloads are not that shape behind a threshold — they are a shape with no
dominant zero parity at all.

`r6` and `r154` *are* the shape, and §11.39 already reads them.

### Relaxing the threshold does not reach `r19`/`r156`

Lowering §11.39's fill threshold and re-running over the 115:

| fill threshold | payloads accepted |
| --- | --- |
| ≥ 80% (today) | 0 more |
| ≥ 70% | 1 more (`r6`) |
| ≥ 60% | 1 more (`r6`) |
| ≥ 50% | 2 more (`r6`, `r154`) |
| ≥ 40% | 2 more |

So the threshold buys one payload, and it buys it by reading `r6`, which §11.39
already reads at 80%. `r19` and `r156` never appear at any threshold, because
lowering the fill requirement does not raise their zero fraction — the bytes simply
are not half zeros.

### What this means

The reviewer's step — "generalise `parse_param_table` so a payload whose every slot
carries a value is accepted" — is not available, because the payloads it names do not
have the alternating-zero shape to generalise. §11.50's classification was the error,
and it is withdrawn.

What `r19` and `r156` are is still open: little-endian `u16` values in the 0x20-0x7F
range (§11.49), not UTF-16 text (§11.50), and not alternating-zero (this section).
They are two payloads of 115 with an unidentified shape.

The goal's second half is not met at 345 of 460. The residue is **115 payloads**, of
which 2 have a `u16`-valued shape that three probes have now read three different
ways, and none of the three is established.

## 11.52 The stream is found anywhere, and 418 of 460 payloads have output

The `f32` value-stream reader started its run at offset zero and ended the search at
the first value the plausibility rule rejected. For `r19 nter.mod` (4,028 B) and
`r156` (24,328 B) that stopped it immediately: both open with an ASCII region —
`00 53 00 55 00 54 00 31`, a byte and a zero alternating — and reading four bytes of
that from offset zero as `f32` yields a denormal, which the rule rejects for the
measured reason of §11.26 (a misaligned read landing in a neighbouring value's
mantissa). The region that *is* `f32` data sat behind the head and was never reached.

Resetting the run rather than ending the search finds it: after the head, `r156`'s
data reads at phase 1 as `0xbf4e3a40` = 0.8193, `0x3f3ccccd` = 0.7377, `0x3ee4fc34` =
0.4478, `0x3f800000` = 1.0 — normalized coordinates, which is what a drawing profile
carries.

| | before | after |
| --- | --- | --- |
| `f32` value streams read | 39 | **112** |
| payloads with modern output | 345 | **418** |
| payloads reported | 115 | **42** |

The run now reports `glTF 230   json 188   .mod 42   files 1643` on the Italian disc,
and the Lean reader agrees exactly — `mesh 17, profile 213, record 48, param 9, names
19, streams 112, other 42` — with the harness reporting `all checks passed`.

The change is one line of behaviour — reset the run, do not end the search — and it
converted 73 payloads because the head was in the way rather than because the data was
different. Two unit tests pin it in both halves: `a_stream_behind_an_ascii_head_is_still_a_stream`
and `the_longest_run_is_the_one_reported`.

### What remains

**42 payloads** fit no reader, and they are the ones whose data carries no readable
`f32` run either. §11.29's census of them is unchanged in kind; what has changed is that
the count is 42 rather than 115.

The goal's second half is not met at 418 of 460, but it is met for 91% of the shipped
model payloads, and the residue is now small enough to characterise individually rather
than as a class.

## 11.53 The reader's thresholds are calibrated, and the 42 fall below the boundary

The obvious next move on the 42 unconverted payloads was to relax
`parse_value_stream`'s two thresholds until they fit. Before doing that, the
thresholds were checked against the population the reader already accepts, because a
threshold that has never been measured against anything is a guess whatever its value.

### The calibration

Calling the reader over the whole container and recording what it accepts:

| | the 112 value streams it accepts |
| --- | --- |
| run length, in values | **min 16**, median 55, max 513 |
| spread between extremes | **min 0.519**, median 4.295, max 527,536 |

The reader declares `MIN_STREAM_VALUES = 16` and `MIN_STREAM_SPREAD = 0.5`. Both sit
**exactly on the measured boundary of the accepted population** — the shortest accepted
run is 16 values and the narrowest accepted spread is 0.519, which is 0.5 rounded up in
binary floating point. The calibration also covers the drawing profiles, whose shortest
accepted run is 17 values.

So neither threshold is arbitrary. They are not numbers chosen to make a decision; they
are the boundary the corpus itself draws.

### What the 42 look like against it

| group | count | their longest run |
| --- | --- | --- |
| no run at all | **10** | under 8 values |
| short runs | **21** | 8–15 values |
| narrow runs | **11** | 16+ values, spread 0.05–0.35 |

The 21 have runs *shorter than anything the corpus accepts* — `r6` has 15 values
spanning `[0.0, 1.0]`, `r37` 15 values spanning `[0.0, 1.667]`, `r76` 11 values spanning
`[-0.309, 1.191]`. The 11 have runs long enough but **narrower than anything accepted** —
`r367` 29 values spanning `[0.027, 0.296]`, `r306` 68 values spanning `[0.025, 0.353]`.

### Why the thresholds were not lowered

Every measurement that motivated the earlier readers came from comparing an accepted
population with a rejected one. Applied here, the comparison says the opposite of what
relaxing the threshold assumes: **the 42 are on the wrong side of a boundary the
accepted 325 payloads define**, not behind a boundary drawn too tight. Lowering
`MIN_STREAM_VALUES` to 8 would admit 14 payloads whose runs are half the shortest run
the corpus has ever accepted, and lowering `MIN_STREAM_SPREAD` to 0.05 would admit 5
whose spread is a tenth of the accepted minimum. Neither number is derived from the
payloads; both are derived from wanting them to fit.

That is the guess `AGENTS.md` forbids, and it is the third distinct shape of guess this
session has withdrawn rather than committed: §11.46's misaligned branch, §11.50's
UTF-16 reading, and now these thresholds.

### One more probe: a magic variant

`r37`'s bytes are a record stream — `c2 a4 b2 3f` (1.3958), `00 00 80 3f` (1.0), `55 55
d5 3f` (1.6667), `cd cc 4c 3e` (0.2), interleaved with `u32` counts — which is what the
48 `mod II` record streams in §11.31 look like, except that it carries no `mod II`
magic. So a variant magic was looked for: every 4-to-8-byte window at each fixed offset
in the first 96 bytes of all 42, counted across the set, requiring at least 8 payloads to
share it and at least one byte to be printable ASCII.

**Nothing.** There is no repeated ASCII tag at a fixed offset, so the 42 do not carry a
magic under another spelling.

### Where the 42 now stand

| | |
| --- | --- |
| recorded as converted | **418 of 460** — 17 meshes, 213 profiles, 188 JSON+text |
| reported per resource | **42** — 10 with no run, 21 with short runs, 11 with narrow runs |
| their shape | not the `f32` stream, and not behind the threshold |

The goal's second half is not met at 418 of 460. The 42 are characterized by what they
are *not* against every shape this project has read, which is the first time the residue
has been characterized that completely — and it is characterization, not conversion.

## 11.54 Names the container's slice cut off are names, and 420 of 460 have output

§11.53 characterized the 42 unconverted payloads and stopped. Two of them were already
half-read and did not convert only because of how the reader counts, and this section is
that fix.

### What the four "signature" payloads actually are

§11.26's lead — a repeated 5-byte signature `00 05 45 XX 01` before a texture name — is
**withdrawn as a signature**. Scanning the whole container for it finds 23 occurrences
across 5 payloads, with the tag byte incrementing monotonically (`0x20, 0x21, 0x22 …`),
which is the shape of coincidental matches inside structured data rather than a format
tag. In `r33` the same offset holds `65 74 2e` — the bytes `et.` of a name — not a
signature at all.

What the four payloads do carry is the thing inside the signature:

| payload | complete names | cut off at the payload's end |
| --- | --- | --- |
| `r33 od` | `net.18.tga`, `createBtMagnet.19.tga` | `createBtMagnet.2` |
| `r58 lect.mod` | `ct.36.tga`, `createBtSelect.37.tga` | `createBtSelect.38.tg` |
| `r76`, `r80` | none | none |

The container slices its payloads, so a name table's last record can end without its
NUL. `r58`'s payload ends at `createBtSelect.38.tg` — 20 printable characters and then
the boundary. That is a name whose tail was not carried, not a gap in the table.

### The measurement that makes it a rule and not an exception

Runs of printable characters reaching the payload's end with no NUL after them:

* **8 of the 19 name tables this reader already accepts** have one, so the sliced tail
  is a property of the format, not of these two payloads;
* **only 2 of the 460 payloads** have one at all while carrying two complete names, so
  the new reading admits exactly those two and nothing else;
* the 19 accepted tables carry **min 3 names each**, so the reader's three-name
  threshold already has a measured floor — which is why the cut name counts toward it
  rather than replacing a complete one.

### What the reader does now

`parse_name_table` records the sliced tail in a `truncated` field and counts it toward
the three-name minimum. `write_name_json` writes it under `names the slice cut off`,
separately, so a reader of the document cannot mistake `createBtSelect.38.tg` for a
complete name. Two unit tests pin it: `a_table_whose_last_record_was_sliced_is_read`
and `two_names_with_no_sliced_tail_is_rejected`.

The run now reports:

```text
models: 1165   ambiguous: 696   total: 1165   failed: 40
glTF 230   json 190   previews 230   png 935   .mod 40
```

**17 meshes + 213 profiles + 190 JSON+text = 420 of 460**, against 418 before. The Lean
reader agrees exactly — `mesh 17, profile 213, record 48, param 9, names 21, streams
112, other 40` → `output 420` — and the harness reports `all checks passed`.

### What is left

**40 payloads** fit no reader. §11.53's calibration stands: the 21 short-`f32` payloads
have runs of 8–15 values against an accepted minimum of 16, the 11 narrow ones have a
spread of 0.05–0.35 against an accepted minimum of 0.519, and 10 have no run. `r76` and
`r80` carry no names at all. The goal's second half is not met at 420 of 460.

## 11.55 The index tables are read, and 425 of 460 payloads have output

§11.53 characterized the 40 unconverted payloads and stopped at the f32 shape. Five
of them carry a structure that the same measurement found and that no reader took:
the alternating-zero layout `parse_param_table` reads, with values that reach
`0xf2`.

```text
r311 mod (276 B):  79 00 7f 00 80 00 81 00 77 00 82 00 5c 00 80 00 ...
r105 createModBtSize04.mod (472 B): 73 00 73 00 98 00 75 00 98 00 7b 00 ...
```

Every other byte is zero and the ones between are byte values. The reader
`parse_param_table` rejects them because its rule requires **95% of the carrying half
below 128**, and 34–53% of these values are at or above it.

### Why this is a separate population and not a loosened threshold

The two populations were measured before anything was written:

| | carrying values ≥ 128 |
| --- | --- |
| the nine accepted parameter tables | **0–3%** |
| these five payloads | **34–53%** |

There is no overlap, so the 128 rule is not a threshold that excludes these by
accident; it is the boundary between two measured populations. Writing a reader that
kept the alternating-zero layout but dropped the 128 rule would read the second
population, and that is what `parse_index_table` does — with its own stated
thresholds (≥90% of one parity zero, ≥90% of the other nonzero, ≥16 values) rather
than an inherited one.

### What the run reports

```text
models: 1165   ambiguous: 696   total: 1165   failed: 35
glTF 230   json 195   previews 230   png 935   .mod 35
```

**17 meshes + 213 profiles + 195 JSON+text = 425 of 460**, against 420 before. The
Lean reader agrees exactly — `mesh 17, profile 213, record 48, param 9, names 21,
streams 112, index 5, other 35` → `output 425` — and the harness reports `all checks
passed`.

The documents record the values verbatim and state the meaning as undetermined, which
is the same form the other five JSON+text families take and the reason is the same:
the payload carries no field names and no lengths.

### Two writer bugs, and what they have in common

The first version of `write_index_json` wrote `"fill is zero for": 89% of its bytes`
and `"values range from": 0x33 to 0xc2` — neither of which is a JSON value.
`json.load` rejected 5 of the 190 documents the run had just written. Then its
`meaning` literal, written with a Rust `\` line continuation, put a backslash into a
JSON string literal, which is an invalid escape.

Both are now fixed and both are covered by `gltf::index_json_tests`, which reads the
document its own writer produced.

That is the **third and fourth time** this session a hand-written document was not
checked against its reader (§11.39's percentages, §11.54's empty list, and now these
two). The pattern is not carelessness about JSON; it is that the writer and the reader
were written by the same pass. `AGENTS.md` already says every new decoder ships with a
test that reads its output back — this is where that rule earned its place, four
times over.

### What is left

**35 payloads** fit no reader: 21 with `f32` runs of 8–15 values against an accepted
minimum of 16, 10 with no run at all, and 4 whose only shape is a carrying half of
the full byte range with no dominant zero parity. `r203 athCircle.mod` carries
0x00–0xf5 at 89% nonzero, which is one byte short of the index reader's 90% and whose
bytes are full-range rather than index-range. The goal's second half is not met at
425 of 460.
