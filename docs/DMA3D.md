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
adds two kinds:

| kind | region length | measured on |
|---|---|---|
| `Models` | `4*N + 100` | `DMA3D.MMM`, N=460: region 1940 bytes |
| `Textures` | `6*N + 2` | `DMA3D.MMX`, N=705: region 4232 bytes |

Both were checked structurally, not just arithmetically: `offsets[0]` lands
exactly at `names_end + region_len`, `offsets[N]` equals the file length, the
table is monotone, and the `4*N` u32 block at the head of each region holds
Mac-style timestamps in the `0xb5..` range the sound containers use.

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

## 3. What is extracted, and what is not

`src/dma3d.rs` extracts the two new kinds. Every resource is written with its
payload **verbatim** — `.mod` for models, `.tex` for textures — in both the
modern and `--original` paths, so both are byte-exact by construction. On the
Italian disc that is 460 models (4,120,513 bytes) and 705 textures
(3,989,968 bytes); on the English disc the same counts and the same
byte-exactness.

That passthrough is deliberate and is the honest state of the work: the
payload *contents* are not decoded yet, so nothing here claims they are.

* **The model payload** starts with the six bytes `mod II` and then a
  little-endian stream of `u32`s and IEEE floats. It is not a flat vertex
  buffer: a second `mod II` header appears about 100 bytes into
  `createAmbientLight.mod`, so the payload is a sequence of self-describing
  records. Sizes run from 52 bytes (`createAmbientLight.mod`) to 2,654,687
  (`createModMickeyAx.mod`).
* **The texture payload** is 4-byte aligned and mostly carries a 256-entry
  RGBA prefix, but not always — a 260-byte resource cannot — so the form is
  per-resource and the selector is in the metadata that is not yet
  identified. Pixel counts are not always a product of a width and a height
  (one is prime), so the payload is not a plain `w*h*4` array either.

The engine's own names come from `DMA3D.exe`'s RTTI: `MM3DModelMolecule`,
`MM3DModDatabase`, `MM3DIndexedMeshDeformer`, `MM3DTexureCollection`,
`DMAFindDeformersFromMeshVisitor`, `MM3DDisplayTreeNodeIndexedMesh`. The
resource readers report `Bad checksum / Bad version number / Bad header in
model|texture resource file '%s'`, so both formats carry their own validation.

### 3.1 What the reader in `DMA3D.exe` shows

The `Models` container's class has its vtable at `0x539ba4`; the tag getter
at slot 0 is `mov eax, <tag>; ret`, which is how the class was found. Its
**load** method is slot 1 (`0x4b2b60`), and it does the following, in order:

1. `call 0x4d4070` — a validity check (returns false → report failure 6);
2. `call 0x4e87c0` — stream read into the object's stream at `this + 0x210`;
3. `cmpw $0x2, <local>` — **the first u16 read must be 2** (else failure 4);
4. a second `0x4e87c0` read into `this + 0x470`, whose u16 is tested for
   zero and, if zero, jumps to the tail;
5. a buffer-resize loop (`0x4a61a0`, `0x4d71a0`) walking a count at
   `this + 0x474`.

So the stream carries its own `u16` version field equal to **2** after
whatever wrapper consumed the `mod II` magic, and the payload is then walked
as counted records. The `Textures` class is at `0x539e28` and works the same
way through the same stream helpers (`0x4e87c0` read, `0x4e8a50` read/write,
`0x4e8a90` read).

### 3.2 The mesh — located and measured

Each payload is one self-contained `mod II` stream: verified on all 460, the
magic appears exactly once, at offset 0. After it comes `u32 16`, then a header
whose word values are small and mostly distinct, then the mesh.

The payload is a sequence of **counted record arrays** — the shape the loader
at `0x4b2b60` already told us to expect, since it walks counted lists after
checking the leading `u16`:

```text
u32   C                 index count
u16[C]                  index list, one-based, values < V
u32   V                 vertex count
vertex[V]               16 bytes: x, y, z (f32) + w (f32, 1.0)
u32   V                 normal count (== V)
normal[V]               16 bytes: nx, ny, nz (f32) + a fourth f32
...                     texture coordinates, then animation tracks
```

The layout holds on **31 of the 42 non-UI static props** (`createProp*`); the
`createPropUI*` variants are the ones it does not fit (§3.3).

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
| remainder | `0x1773` | 1,237 B |

The normal array is what identifies the vertex array as geometry rather than a
float table: its `(nx, ny, nz)` triples are **unit vectors** (`|v| == 1.0`),
whereas the vertex coordinates are positional (`|v|` up to ~2.41). Both arrays
carry a fourth float, which is `1.0` in every vertex record and is *not*
constant in the normal records.

`createPropGuitar.mod` fits the same shape at `C = 174`, `V = 68` (68 vertex
records, a 68-entry normal array), then 25,821 bytes that are mostly
animation.

### 3.3 The four remaining questions, answered

**(1) The header does not carry the counts.** Searched every byte of the
header of all 42 props for a field equal to `C`, to `V`, or to the offset at
which the mesh starts — as `u32` and as `u16` — and nothing matches. The
header's head is constant (`16, 65537, 24, 1, 1, 2, 0, 1, …` in every file),
its tail is short and variable (the last words are `…, 1, 0, n` with `n`
1–4), and its length varies from 163 to 263 bytes. So a decoder locates the
section chain **by its structural signature** rather than by a table:
find `C`, check `0xa7 + 2*C` starts a `u32` whose value starts `V` records of
`w == 1.0`, then check the next `u32` equals `V` again and starts unit-length
`w == 0.0` records. All 42 props satisfy that chain, which is what makes it a
format and not a coincidence.

**(2) The fourth float is a record-type marker.** In every vertex record it is
exactly `1.0`; in every normal record it is exactly `0.0` (measured over all
68 vertices and normals of `createPropGuitar.mod`, single distinct value in
each). So `(x, y, z, 1.0)` is a position and `(nx, ny, nz, 0.0)` is a
direction — the fourth float tags the kind rather than carrying data.

**(3) What follows the normal array.** `u32 0`, `u32 1`, `u32 V`, then floats.
Measured on `createPropGuitar.mod` (`V = 68`): after the 68 normals come
`00 00 00 00 | 01 00 00 00 | 44 00 00 00` (0, 1, 68) and then a float stream
that opens `ac bc b4 3e b4 10 cb 3c | 7e db b4 3e 40 6a cc 3c` — i.e.
`(0.3530, 0.0248, 0.3530, 0.0250)`, two texture-coordinate pairs with a small
difference between them. So the block is keyframed texture coordinates, not a
flat UV table: the second pair differs from the first slightly, which is what
a track looks like rather than a per-vertex UV.

**(4) The animation grammar — the shape is now measured.**

The tail opens with a **universal header**: `u32 0`, `u32 1`, `u32 V`,
present at the start of the tail in **all 42** static props (`V` here being
the vertex count, 24–257). That triple is the anchor for everything after the
mesh.

What follows it is **not uniformly texture coordinates**. After the header
there are `V` pairs of little-endian floats; on `createPropGuitar.mod` all 68
of them land in `0..1` (`(0.3530, 0.0248)`, `(0.3532, 0.0250)`, …), which is
what texture coordinates look like — but on `createPropBed.mod` **zero** of the
256 pairs do, and the rest of the props scatter in between (119/151 on
`createPropBucket.mod`, 7/256 on `createPropRooster.mod`). So the block after
the header is the same *shape* everywhere but its *meaning* varies, and the
texture-coordinate reading is confirmed for one file only.

**The animation is named.** `createPropGuitar.mod` ends with a table of
ASCII names, 13 bytes apart:

```text
0x6f6c  "key06"
0x6f8b  "key01"
0x6f98  "key02"
0x6fa5  "key03"
0x6fb2  "key04"
0x6fbf  "key05"
```

so the tracks are called `key01`…`key06` and the table lives at the **end** of
the payload. Immediately after the UV block, `createPropGuitar.mod` has
`u32 3, u32 3` — a track count of 3 — and then the keyframe data, whose first
keyframe is `(0.0, 4.6777)`. The keyframe streams step by `1/30` s and
`0.1333` s (30 fps and 7.5 fps), with the sampled value oscillating around a
centre, which is what `MM3DIndexedMeshDeformer` and
`DMAFindDeformersFromMeshVisitor` would drive.

So a decoded model is *not* static geometry: the tail holds the texture
coordinates and a named set of deformation tracks, and the glTF output has to
carry both.

### 3.4 `createPropUI*` — measured, and it is not a mesh

All 42 of them are **exactly 276 bytes** (minimum, maximum and every value in
between equal), and the mesh layout does not fit any of them: a scan for the
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
hypothesis, not a decode: what the fields mean has not been traced. A decoder
for them should emit the reference (and the pair of UVs) rather than geometry.

## 4. Known failures

`DMA3D.MMF` (Films, 3 resources) does **not** extract: the shared film
decoder reports `bad bounds top=258 left=772 bottom=0 right=0` on film 0.
The other two films are not reached. The 22-byte record the shared decoder
reads must be laid out differently in this build; that is the next thing to
measure. Everything else on both discs extracts.

## 5. Measured vs inferred

**Measured** — the skeleton and region sizes above; the 460/705/145/3/3
resource counts; the byte-exact payload of every one of the 1,165 3D
resources on both discs; the `mod II` magic on all 460 model payloads; the
`DMA3D.MMX` reference inside the models' trailer.

**Inferred / open** — the metadata's `2*N` byte block in a `Textures`
container (read as `attr`/`flags` at the picture positions, unconfirmed);
the per-resource palette selector; the texture dimensions; the whole `.mod`
record grammar; the film record layout.
