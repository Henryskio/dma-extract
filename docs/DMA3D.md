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

### 2.1 The two region sizes, corrected

| kind | region length | measured on |
|---|---|---|
| `Models` | `4*N + 100` | `DMA3D.MMM`, N=460: region 1,940 bytes |
| `Textures` | `32*N + 4*N + 2*N + 2` | `DMA3D.MMX`, N=705: region 26,792 bytes |

Both were checked structurally, not just arithmetically: `offsets[0]` lands
exactly at `names_end + region_len`, `offsets[N]` equals the file length, the
table is monotone, and each region holds Mac-style timestamps in the `0xb5..`
range the sound containers use.

The `Textures` region was measured **wrong** at first, as `6*N + 2`. That is
too small by a whole 32-byte-per-resource table, and `container.rs` read the
`attr`, `flags`, `dimensions` and `sizes` arrays out of the middle of the names
table as a result. Nothing downstream used them, which is why the byte-exact
round-trip tests never noticed. The corrected reading of the 26,792 bytes is:

```text
names        32 * N         "CreationSlotTex.TGA", ...
timestamps    4 * N u32     0xb5d16d67, ... Mac-style
two bytes     2 * N         not identified; see §6
padding       2 bytes
```

The models' 100-byte trailer is **not** a per-resource array. Its content is
a reference to another container:

```text
+0   u16 2
+2   u16 1
+4   u32 0
+8   u32 the texture container's timestamp
+12  30 bytes  "DMA3D.MMX", NUL padded
+42  58 bytes  0xff padding
```

That is the models' record of which texture file their materials come from.
It is measured, not decoded.

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

Src bins that produce the numbers here: `animwalk` (walk a candidate grammar
and report whether it closes the payload), `animscan` (find the counted index
array and check that `D` equals the distinct count), `modinfo --tile`, and
`grammar` (test the loader's own `[u16 2][u16 N][u16 N][u32 N]` shape).

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

### 4.2 The width and the height — the weakest measurement

The container carries **no dimension table**; §2.1 shows its metadata is names
and timestamps. So the dimensions come from the pixel data itself.

`A` is a power of two, so it pins the area and nothing else. **678 of the 705**
areas are perfect squares (8×8 through 256×256) and are read at that root. The
remaining **27** are the 1:2 rectangles (`16×32`, `32×64`, `64×128`,
`128×256`), each with two readings, and they are split by a measured statistic:

> **row continuity** — for a candidate width `W`, the share of vertically
> adjacent pixel pairs carrying the same index. The right width is the one where
> a column of the image is *read* as a column.

It was validated on the 678 squares, where the answer is known in advance: for
real textures with three or more palette entries in use, it lands on the square
root **643 of 659** times. The 16 exceptions are textures whose content repeats
with a short period (buttons built from an 8-pixel motif, dot patterns). A
second, independent check — the dominant horizontal period, which must divide
the true width — agrees with the same width family in every case and never
contradicts a square.

So this is a measurement with a success rate, not a proof: **27 of 705 textures
may be transposed**, and `src/dma3d/texfile.rs` records which orientation each
one was read at (`Axis::Square` / `Wide` / `Tall`) so the choice is reviewable
rather than silent. A real fix needs the reader in `DMA3D.exe`.

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
* `dma3d::write_index` writes one **`index.html`** at the output root: 705
  textures and 144 objects, each as a picture, each linked to its file, with a
  short how-to-open-a-gltf note. No script, no stylesheet, no network, so it
  opens by being double-clicked from a local folder.

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

## 6. Known failures

`DMA3D.MMF` (Films, 3 resources) does **not** extract: the shared film decoder
reports `bad bounds top=258 left=772 bottom=0 right=0` on film 0. The other two
films are not reached. The 22-byte record the shared decoder reads must be laid
out differently in this build; that is the next thing to measure. Everything
else on both discs extracts.

No texture or model payload fails to decode in modern mode. The two
weaknesses are measured and bounded, not silent: the 27 texture orientations
(§4.2) and the animation, which is not decoded at all and is therefore carried
as a `.tail` file rather than as glTF channels.

## 7. Measured vs inferred

**Measured** — the skeleton; the two corrected region sizes of §2.1; the
460/705/145/3/3 resource counts; the byte-exact payload of every one of the
1,165 3D resources on both discs; the `mod II` magic on all 460 payloads; the
mesh chain in 144 of them with zero-based indices and unit-length normals; the
tail's `V` float pairs and the header distribution of §3.2 (94 / 49 / 1); the
`[u32 count][32-byte names]` texture table; the texture palette/index split and
`K = 1 + max(index)` over all 705; the 129-class `MM3D*` RTTI vocabulary of
§3.3; the `DMA3D.MMX` reference inside the models' trailer.

**Measured with a known error rate** — the orientation of the 27 non-square
textures (§4.2, 97.6% on a known-answer set).

**Inferred / open** — the animation frame size and the number of frames, which is
what stops the record grammar of §3.4 from closing (§3.4); the `2*N` two-byte
field in a `Textures` container
(not identified; §2.1); the animation record grammar and which node each track
drives (§3.2); the material link — no mesh-bearing payload names a texture
(§3.2); the meaning of `createPropUI*`'s fields (§3.5); the film record
layout (§6).
