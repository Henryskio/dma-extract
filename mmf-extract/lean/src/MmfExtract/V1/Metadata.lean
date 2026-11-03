import MmfExtract.Bytes
import MmfExtract.Types
import MmfExtract.Path

/-!
The first game's per-resource metadata region.

A port of `src/v1/metadata.rs`. Version 1 keeps the later versions' shape up to
the end of the 32-byte names table -- an offsets table, two bytes of padding,
then the names -- and then has a metadata region of its own:

```text
Pictures   4*N  u32 handle array
             2   marker, always 0x0007
             N   flags byte array
             N   kind byte array
            4*N  {u16 height, u16 width} dimension table
            4*N  u32 uncompressed-size table
             N   byte array whose accessor is unknown
             2   u16 palette count
      count*1024  palette bank
Sounds     4*N  u32 Mac timestamps
             N   format byte array
             2   padding
Blobs      4*N  u32 handle array
             2   padding
Scripts    (no version-1 metadata region)
```

The region must end exactly at `offsets[0]`, which is what pins the layout
down; a container where it does not is rejected rather than guessed at.
-/

namespace MmfExtract.V1.Metadata

open MmfExtract.Bytes
open MmfExtract (ContainerKind Dimension)
open MmfExtract.Path (str)

/-- Number of bytes in a 256-entry palette. -/
def paletteBytes : Nat := 1024

/-- Everything a first-game metadata region contributes. -/
structure V1Metadata where
  /-- Sounds only: the per-resource format byte. -/
  attr : ByteArray
  flags : ByteArray
  kinds : ByteArray
  dimensions : Array MmfExtract.Dimension
  /-- Each resource's declared **uncompressed** size; `0` means stored as-is. -/
  uncompressedSizes : Array UInt32
  /-- The palette bank: `count` 1024-byte palettes, first game only. -/
  palettes : Array ByteArray
  deriving Inhabited

/-- The empty metadata a first-game Scripts container contributes. -/
def default : V1Metadata :=
  { attr := ByteArray.empty, flags := ByteArray.empty, kinds := ByteArray.empty,
    dimensions := #[], uncompressedSizes := #[], palettes := #[] }

/-- Parse a first-game metadata region.

`base` is the offset just past the `4*N` u32 handle array the caller has
already consumed, and `payloadStart` is `offsets[0]`. The region must end
exactly at `payloadStart`.
-/
def parse (data : ByteArray) (kind : ContainerKind) (n : Nat) (base payloadStart : Nat)
    (bigEndian : Bool) (path : System.FilePath) : Except String V1Metadata := do
  let readU16 off := Bytes.u16At data off bigEndian
  let readU32 off := Bytes.u32At data off bigEndian

  let need := base + 4 * n
  if need > data.size then
    throw s!"{MmfExtract.Path.str path}: version 1 metadata does not fit (need {need}, have {data.size})"

  let mut attr : ByteArray := ByteArray.empty
  let mut flags : ByteArray := ByteArray.empty
  let mut kinds : ByteArray := ByteArray.empty
  let mut dimensions : Array MmfExtract.Dimension := #[]
  let mut uncompressedSizes : Array UInt32 := #[]
  let mut palettes : Array ByteArray := #[]

  -- Every kind's region starts after the `4*N` u32 array.
  let mut cursor := base + 4 * n

  match kind with
  | .pictures =>
    -- The 2-byte sub-version marker. Every shipped picture container reads 7
    -- here, and `0x4183`'s reader requires it.
    if cursor + 2 > data.size || (← readU16 cursor) != 7 then
      throw s!"{MmfExtract.Path.str path}: version 1 picture metadata marker is not 7"
    cursor := cursor + 2

    for i in [0:n] do
      flags := flags.push (← Bytes.byteAt data (cursor + i))
    cursor := cursor + n

    for i in [0:n] do
      kinds := kinds.push (← Bytes.byteAt data (cursor + i))
    cursor := cursor + n

    for i in [0:n] do
      -- The pair is {height, width}: the SECOND u16 is the width.
      let h ← readU16 (cursor + 4 * i)
      let w ← readU16 (cursor + 4 * i + 2)
      dimensions := dimensions.push { width := w, height := h }
    cursor := cursor + 4 * n

    for i in [0:n] do
      uncompressedSizes := uncompressedSizes.push (← readU32 (cursor + 4 * i))
    cursor := cursor + 4 * n

    -- One more N-byte array whose accessor is unknown; skipped.
    if cursor + n > data.size then
      throw s!"{MmfExtract.Path.str path}: version 1 unknown byte array does not fit (need {cursor + n}, have {data.size})"
    cursor := cursor + n

    let paletteCount := (← readU16 cursor).toNat
    cursor := cursor + 2

    for k in [0:paletteCount] do
      let start_ := cursor + k * paletteBytes
      let want := start_ + paletteBytes
      if want > data.size then
        throw s!"{MmfExtract.Path.str path}: palette bank overruns the file (need {want}, have {data.size})"
      palettes := palettes.push (data.extract start_ want)
    cursor := cursor + paletteCount * paletteBytes
  | .sounds =>
    -- One `N` byte array (the format byte) and 2 bytes of padding. The `4*N`
    -- array ahead of it holds the Mac timestamps, read as `extra_u32`.
    if cursor + n + 2 > data.size then
      throw s!"{MmfExtract.Path.str path}: version 1 sound metadata does not fit (need {cursor + n + 2}, have {data.size})"
    for i in [0:n] do
      attr := attr.push (← Bytes.byteAt data (cursor + i))
    cursor := cursor + n + 2
  | .blobs =>
    -- Nothing beyond the 4*N array plus 2 bytes.
    cursor := cursor + 2
  | _ => pure ()

  if cursor != payloadStart then
    throw s!"{MmfExtract.Path.str path}: version 1 metadata ends at {cursor} but the first payload starts at {payloadStart}"

  return { attr, flags, kinds, dimensions, uncompressedSizes, palettes }

end MmfExtract.V1.Metadata
