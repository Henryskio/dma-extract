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

**How the count is read.** `Container::walk_count` (both halves) walks out from
the declared count while each next table entry is a plausible payload offset — a
step of at least 4 bytes and at most 2 MiB — and stops where the table stops
describing payloads. That entry is the container's length. The declared count is
kept as `declaredCount` so the disagreement is reported, not hidden: `781` against
`705` for the Italian `DMA3D.MMX`, `460` against `460` everywhere else.

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
