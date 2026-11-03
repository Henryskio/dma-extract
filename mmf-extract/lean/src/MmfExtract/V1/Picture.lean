import MmfExtract.Container
import MmfExtract.Picture
import MmfExtract.V1
import MmfExtract.V1.Lzss

/-!
The first game's picture resources.

A port of `src/v1/picture.rs`. A version-1 payload is stored either as-is or as
an LZSS stream, which is what the uncompressed-size table says; the expanded
stream is an ordinary shared-format picture payload: the same `2*(2h+1)` count
word, the same big-endian offset table, the same alternating
transparent/opaque run bytes and the same colour records (`docs/GAME1.md` §4.3).

This module therefore never touches the container's decoded-size table: the
LZSS step replaces the shared LZ77 step, and everything after it is
[`MmfExtract.Picture`]'s own work.
-/

namespace MmfExtract.V1.Picture

open MmfExtract.V1

/-- The payload of picture resource `i`, after the decompression step the first
game's container applies.

This is also what the `--original` mode writes out, so the two paths share it
rather than each re-deriving the rule.
-/
def payload (container : MmfExtract.Container.Container) (i : Nat) :
    Except String ByteArray := do
  let raw ← container.payload i
  let uncompressed := uncompressedLen container i
  if uncompressed == 0 then
    return raw
  match Lzss.decompress raw uncompressed with
  | .error e =>
    throw s!"expanding resource {i} ('{container.name i}') from {raw.size} to {uncompressed} bytes: {e}"
  | .ok out => return out

/-- Decode picture resource `i` of a version-1 container. -/
def decode (container : MmfExtract.Container.Container) (i : Nat) :
    Except String MmfExtract.Picture.Decoded := do
  let data ← payload container i
  MmfExtract.Picture.decodeBytes container i data (paletteOf container i)

/-- Which layout resource `i` is stored in. -/
def pixelFormat (container : MmfExtract.Container.Container) (i : Nat) :
    Option MmfExtract.Picture.PixelFormat := do
  let data ← (payload container i).toOption
  MmfExtract.Picture.pixelFormat container i data (paletteOf container i)

end MmfExtract.V1.Picture
