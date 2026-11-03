import MmfExtract.Container
import MmfExtract.Picture
import MmfExtract.Types
import MmfExtract.V1.Metadata

/-!
The first game's own decode path: payload expansion through its LZSS, and the
palette bank its containers keep outside the payload.

This is the first game's half of the split the Rust extractor keeps: the shared
modules never mention version 1, and this module never mentions any other
version's container or compression.
-/

namespace MmfExtract.V1

open MmfExtract.Container (Container)

/-- Resource `i`'s declared **uncompressed** size.

Zero means the payload is stored exactly as it appears in the container. Any
other value is the number of bytes [`Lzss.decompress`] expands the payload to,
and is how a first-game container says "this payload is compressed".
-/
def uncompressedLen (container : Container) (i : Nat) : Nat :=
  (container.uncompressedSizes[i]?).getD 0 |>.toNat

/-- The palette this container holds for its pictures.

`mpApp.PIC` ships two palettes and nothing in the container says which entry a
picture indexes into -- there is no per-resource selector field -- so entry 0 is
used. That choice rests on a vision check rather than a byte: rendering
`dmalog074`/`dmalog079` and `nameLess_230`/`nameLess_231` through both entries
and asking a vision model which is the intended rendering returns "FIRST" for
both pictures tested. `docs/GAME1.md` §9 classifies this as judgement, not
measured.
-/
def paletteOf (container : Container) (_i : Nat) : Option ByteArray :=
  container.palettes[0]?

/-- The container's palette bank: `paletteCount` 1024-byte palettes. -/
def paletteBank (container : Container) : Array ByteArray := container.palettes

/-- How many palettes the container's metadata region holds. -/
def paletteCount (container : Container) : Nat := container.palettes.size

/-- Number of bytes in a 256-entry palette. -/
def paletteBytes : Nat := 1024

end MmfExtract.V1
