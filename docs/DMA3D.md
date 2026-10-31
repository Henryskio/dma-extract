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

### 3.2 What the payload bytes show

Each payload is one self-contained `mod II` stream: verified on all 460,
the magic appears exactly once, at offset 0. After it comes `u32 16`, then
a stream of little-endian words. Reading those as u32 gives a header table
whose values are small and mostly distinct — `1, 24, 22, 18, 7, 16, 690, 6,
127, 32, 26, 30, 9, 31, 4, 19, 28, 20, 23, 27, 33, …` on
`createModMickeyAx.mod` — which reads like a record-type or record-size
table rather than data.

The largest payload, `createModMickeyAx.mod` (2,654,687 bytes), has one long
run of little-endian floats, and it is **not** geometry: at offset 649,012
there are 8,298 plausible floats whose first elements increase by exactly
`1/30` (`0.0, 0.0333, 0.0667, 0.1, …`) while the second element oscillates
around 5.70. Those are `(time, value)` pairs on a 30 fps base — an
**animation track**. That matches the asset's own name (`MickeyAx`) and the
class names (`MM3DIndexedMeshDeformer`, `DMAFindDeformersFromMeshVisitor`):
the container holds deformable, animated models, not just static meshes.

**So the vertex/index buffers exist but have not been located.** The
animation run occupies 33,192 of 2,654,687 bytes; the remaining ~2.6 MB is
mostly u16-sized words, which is where the geometry is expected to be.

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
