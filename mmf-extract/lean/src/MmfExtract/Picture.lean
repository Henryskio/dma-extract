import MmfExtract.Bytes
import MmfExtract.Container
import MmfExtract.Lz77
import MmfExtract.Png

/-!
MMFW picture resources: decode to RGBA.

A port of `src/picture.rs`. A picture resource decompresses to one of these
layouts, and the payload length is what tells them apart -- the container's
`flags` byte is only a hint and its `kind` byte only feeds the run-length
sub-format:

* **32 bits per pixel** -- a flat array of pixels, four bytes each, in the order
  the game stored them: **transparency, red, green, blue**. The first byte is
  an inverted alpha (0 = fully opaque, 255 = fully transparent) and the colour
  bytes are straight, non-premultiplied values.
* **32 bits per pixel, row padded** -- each row padded to a multiple of four
  bytes, which makes the payload longer than a flat `4*w*h` array.
* **8 bits per pixel with a 1024-byte palette prefix** -- 256 entries of four
  bytes each, same order, then palette indices; rows padded to a multiple of
  four bytes.
* **8 bits per pixel without a palette** -- an 8-bit coverage mask: each byte
  is an alpha value for a shape the game tints at runtime, rendered white.
* **the two-section run-length sub-format** -- runs and colours per row, with
  the offset table at the start. See [`rleRgba`].
-/

namespace MmfExtract.Picture

open MmfExtract (ContainerKind Dimension)
open MmfExtract.Container

/-- Number of bytes in a 256-entry palette. -/
def paletteBytes : Nat := 1024

/-- How a picture resource's pixels are stored. -/
inductive PixelFormat where
  /-- 32 bits per pixel: transparency (inverted alpha), red, green, blue. -/
  | transpRgb32
  /-- 32 bits per pixel with every row padded to a multiple of four bytes. -/
  | transpRgb32Padded
  /-- 8 bits per pixel, with a 1024-byte palette prefix. -/
  | paletted8
  /-- 8 bits per pixel without a palette: each byte is alpha coverage. -/
  | alphaMask8
  /-- The two-section run-length sub-format. -/
  | runLengthRle
  deriving Inhabited, DecidableEq

/-- Convert a stored 32-bit word (transparency, red, green, blue) into straight
RGBA. -/
def toRgba (t r g b : UInt8) : ByteArray :=
  ByteArray.mk #[r, g, b, (255 - t.toNat).toUInt8]

/-- Row stride of an 8bpp image: each row is padded to a multiple of four. -/
def stride8 (width : Nat) : Nat :=
  (width + 3) / 4 * 4

/-- Expand an 8bpp coverage mask to straight RGBA.

Each byte is alpha; the colour is plain white so the mask can be tinted when
composited. Index 0 is fully transparent.
-/
def maskRgba (indices : ByteArray) (width height : Nat) : ByteArray := Id.run do
  let stride := stride8 width
  let mut rgba := ByteArray.emptyWithCapacity (width * height * 4)
  for y in [0:height] do
    for x in [0:width] do
      let a := (indices[y * stride + x]?).getD 0
      rgba := rgba.append (ByteArray.mk #[255, 255, 255, a])
  return rgba

/-- Expand an 8bpp indexed image to straight RGBA with an explicit row stride.

One shipped film container stores rows unpadded (`stride == width`); the other
pads to a multiple of four. Rows shorter than the table says are padded with
transparent pixels rather than failing the resource.
-/
def indexedRgbaStride (palette indices : ByteArray) (width height stride : Nat) :
    ByteArray := Id.run do
  if stride == 0 then return ByteArray.empty
  let mut rgba := ByteArray.emptyWithCapacity (width * height * 4)
  for y in [0:height] do
    let start := y * stride
    if start >= indices.size then
      for _ in [0:width] do
        rgba := rgba.append (ByteArray.mk #[0, 0, 0, 0])
      continue
    let stop := min indices.size (start + width)
    for i in [start:stop] do
      let idx := indices[i]!
      let slot := idx.toNat * 4
      let t := (palette[slot]?).getD 0
      let r := (palette[slot + 1]?).getD 0
      let g := (palette[slot + 2]?).getD 0
      let b := (palette[slot + 3]?).getD 0
      rgba := rgba.append (toRgba t r g b)
    for _ in [stop - start:width] do
      rgba := rgba.append (ByteArray.mk #[0, 0, 0, 0])
  return rgba

/-- Expand an 8bpp indexed image with the padded row stride. -/
def indexedRgba (palette indices : ByteArray) (width height : Nat) : ByteArray :=
  indexedRgbaStride palette indices width height (stride8 width)

-------------------------------------------------------------------------------
-- The run-length sub-format
-------------------------------------------------------------------------------

/-- Scan `body` for the `2*height+1`-entry offset table.

Returns the entries and the whole body, or `none` when the shape does not
match. `entries[0]` must equal the table's own byte size, and the entries must
be non-decreasing and land inside the body.
-/
def locateRleTable (body : ByteArray) (height : Nat) : Option (Array Nat × ByteArray) := do
  let h := height
  let n := 2 * h + 1
  if h == 0 then none
  else
    let mut entries : Array Nat := #[]
    let mut prev : Nat := 0
    let mut valid := true
    for k in [0:n] do
      let slot := 2 * k
      let hi := ((body[slot]?).getD 0).toNat * 256
      let lo := ((body[slot + 1]?).getD 0).toNat
      let v : Nat := hi + lo
      if v < prev then valid := false
      prev := v
      entries := entries.push v
    let first := (entries.getD 0 0 : Nat)
    let last := (entries.getD (n - 1) 0 : Nat)
    if !valid then none
    else if first != 2 * n then none
    else if last > body.size || last < 2 * n then none
    else some (entries, body)

/-- Foreground pixels in a row's run list, ignoring trailing bytes.

The runs sum to the image width, so the walk stops once the width is reached.
That is what keeps the final row's trailing bytes from being read as extra runs.
-/
def foregroundPixels (runs : ByteArray) (w : Nat) : Nat := Id.run do
  let mut used : Nat := 0
  let mut fg : Nat := 0
  for j in [0:runs.size] do
    if used >= w then
      break
    let len := runs[j]!
    if j % 2 == 1 then
      fg := fg + len.toNat
    used := used + len.toNat
  return fg

/-- Whether every row's colour slice holds exactly one record per foreground
pixel.

This is the decisive test: it is what distinguishes the real layout from a
coincidental u16 ramp. The last row is allowed to fall short, because a
`kind == 0` payload carries a few trailing bytes that are not pixels.
-/
def rowsLineUp (entries : Array Nat) (body : ByteArray) (w h : Nat) (rec : Nat) : Bool :=
  if entries.size < 2 * h + 1 then false
  else Id.run do
    let mut good : Nat := 0
    for row in [0:h] do
      let runs := body.extract entries[row]! entries[row + 1]!
      let colours := body.extract entries[h + row]! entries[h + row + 1]!
      let expected := foregroundPixels runs w * rec
      if colours.size == expected then
        good := good + 1
      else if colours.size > expected then
        -- More colours than foreground pixels is impossible, so this is not
        -- the right layout.
        return false
      -- A short colour slice means the row under-fills; tolerate it, since a
      -- row's runs may not always total the width.
    return good * 10 >= h * 9

/-- Paint the rows into straight RGBA. -/
def decodeRleRows (entries : Array Nat) (body : ByteArray) (width height : Nat) (rec : Nat)
    (palette : ByteArray) : Option ByteArray := do
  let w := width
  let h := height
  let mut rgba : ByteArray := ByteArray.emptyWithCapacity (w * h * 4)
  for row in [0:h] do
    let runs := body.extract entries[row]! entries[row + 1]!
    let colours := body.extract entries[h + row]! entries[h + row + 1]!
    let mut cursor : Nat := 0
    let mut used : Nat := 0
    for j in [0:runs.size] do
      -- Stop at the width: the final row of a kind-0 payload carries trailing
      -- bytes that are not runs.
      if used >= w then
        break
      let len := runs[j]!
      if j % 2 == 0 then
        -- Background run: fully transparent.
        for _ in [0:len.toNat] do
          rgba := rgba.append (ByteArray.mk #[0, 0, 0, 0])
      else
        for _ in [0:len.toNat] do
          let px ←
            if rec == 1 then
              let idx := (colours[cursor]?).getD 0
              let slot := idx.toNat * 4
              let t := (palette[slot]?).getD 0
              let r := (palette[slot + 1]?).getD 0
              let g := (palette[slot + 2]?).getD 0
              let b := (palette[slot + 3]?).getD 0
              some (toRgba t r g b)
            else
              let t := (colours[cursor]?).getD 0
              let r := (colours[cursor + 1]?).getD 0
              let g := (colours[cursor + 2]?).getD 0
              let b := (colours[cursor + 3]?).getD 0
              some (toRgba t r g b)
          rgba := rgba.append px
          cursor := cursor + rec
      used := used + len.toNat
    -- Pad a short row with transparency rather than failing.
    while rgba.size < (row + 1) * w * 4 do
      rgba := rgba.append (ByteArray.mk #[0, 0, 0, 0])
  return rgba

/-- The palette one placement offers, or `none`.

The palette must not sit inside the offset table, which is what the Rust port's
overlap test rejects by clearing `palette` to `None`.
-/
private def paletteAt (data : ByteArray) (tableBase height : Nat) (palAt : Option Nat) :
    Option ByteArray :=
  match palAt with
  | some off =>
    if off + paletteBytes <= data.size &&
        !(off < tableBase + 2 * (2 * height + 1) && tableBase < off + paletteBytes) then
      some (data.extract off (off + paletteBytes))
    else
      none
  | none => none

/-- Try every palette placement for one `rec`, keeping the one whose per-row
colour counts line up exactly.
-/
private def placements (data : ByteArray) (tableBase rec : Nat) (bank : Option ByteArray)
    (width height : Nat) (sources : List (Option Nat)) : Option ByteArray :=
  match sources with
  | [] => none
  | palAt :: rest =>
    let palette := paletteAt data tableBase height palAt
    -- Fall back to the bank only when the payload has no palette of its own at
    -- this placement; `rowsLineUp` below is what decides whether it is right.
    let palette : Option ByteArray :=
      match palette, bank with
      | some p, _ => some p
      | none, some b =>
        if rec == 1 && b.size >= paletteBytes then some (b.extract 0 paletteBytes) else none
      | none, none => none
    if rec == 1 && palette.isNone then
      placements data tableBase rec bank width height rest
    else if rec == 4 && palette.isSome then
      placements data tableBase rec bank width height rest
    else
      match locateRleTable (data.extract tableBase data.size) height with
      | some (entries, body) =>
        if rowsLineUp entries body width height rec then
          decodeRleRows entries body width height rec (palette.getD ByteArray.empty)
        else
          placements data tableBase rec bank width height rest
      | none => placements data tableBase rec bank width height rest

/-- The places a palette can sit for one `rec`: a prefix, a suffix, and the
container's bank last. -/
private def paletteSources (data : ByteArray) (rec : Nat) : List (Option Nat) := Id.run do
  let pre : List (Option Nat) := if rec == 1 then [some 0, some paletteBytes] else []
  let suf : List (Option Nat) :=
    if rec == 1 && data.size > paletteBytes then [some (data.size - paletteBytes)] else []
  return pre ++ suf ++ [none]

/-- Try the run-length layout with the offset table at `tableBase`. -/
private def rleAt (data : ByteArray) (tableBase rec : Nat) (bank : Option ByteArray)
    (width height : Nat) : Option ByteArray :=
  placements data tableBase rec bank width height (paletteSources data rec)

/-- Decode the two-section run-length sub-format, or `none` when the payload
does not have this shape.

```text
[kind == 1]  1024 bytes  palette
u16[2h+1]                big-endian offset table
    entries[0]           the table's own byte size, 2*(2h+1); it is also the
                         offset at which the data begins
    entries[0..=h]       byte offset where row i's run-length slice starts
    entries[h..=2h]      byte offset where row i's colour slice starts
bytes                    run lengths, then colours, concatenated
```

Row `i`'s run lengths are the bytes `[entries[i], entries[i+1])` and **sum to
the image width**; the runs alternate background / foreground, starting with
background, so the runs at odd indices paint pixels. Row `i`'s colour slice
`[entries[h+i], entries[h+i+1])` then holds **one colour per foreground
pixel**, consumed left to right: a single palette index when the container says
`kind == 1`, or four bytes `(T, R, G, B)` when it says `kind == 0`.
-/
def rleRgba (data : ByteArray) (width height : Nat) (bank : Option ByteArray) :
    Option ByteArray := Id.run do
  -- The palette is usually a prefix when the container says kind == 1, but
  -- some resources (`TtuPt_*`) keep the table at offset 0 and either carry no
  -- palette at all or suffix it, so every placement is tried.
  let bases : List Nat :=
    if data.size > paletteBytes then [0, paletteBytes, data.size - paletteBytes] else [0]
  let mut result : Option ByteArray := none
  for tableBase in bases do
    if tableBase > data.size then
      continue
    for rec in [1, 4] do
      if result.isNone then
        result := rleAt data tableBase rec bank width height
  return result

-------------------------------------------------------------------------------
-- Layout detection and decoding
-------------------------------------------------------------------------------

/-- Which layout resource `index` is stored in.

`bank` is an optional palette the *container* holds outside the payload, the
same one [`decodeBytes`] takes; it is needed here because the run-length test
has to try the palette placements before it can tell a one-byte-colour payload
from a four-byte-colour one. Returns `none` when the payload matches no known
layout, so the caller can report the resource as a failure rather than guessing.
-/
def pixelFormat (container : Container) (index : Nat) (data : ByteArray)
    (bank : Option ByteArray) : Option PixelFormat := do
  let dim := container.dimension index
  let (width, height) := (dim.width.toNat, dim.height.toNat)
  let pixels := width * height
  let as32 := pixels * 4
  -- 32 bits per pixel with each row padded to a multiple of four bytes. It is
  -- only distinguishable from `as32` when the width is not a multiple of four,
  -- so the flat layout is tried first.
  let stride32 := 4 * stride8 width * height
  let as8 := paletteBytes + stride8 width * height
  let bare8 := stride8 width * height
  if data.size == as32 then
    some .transpRgb32
  else if data.size == stride32 then
    some .transpRgb32Padded
  else if data.size == as8 then
    some .paletted8
  else if pixels > 0 && data.size == bare8 then
    -- No palette: the bytes are an alpha coverage mask.
    some .alphaMask8
  else if (rleRgba data width height bank).isSome then
    some .runLengthRle
  else
    none

/-- The decoded pixels of one picture resource. -/
structure Decoded where
  width : Nat
  height : Nat
  format : PixelFormat
  /-- Straight RGBA, four bytes per pixel. -/
  rgba : ByteArray
  deriving Inhabited

/-- Decode `data`, already in its final layout, as picture resource `index`.

`bank` is an optional palette the *container* holds outside the payload,
consulted as a last resort when the payload carries none of its own.
-/
def decodeBytes (container : Container) (index : Nat) (data : ByteArray)
    (bank : Option ByteArray) : Except String Decoded := do
  let dim := container.dimension index
  let width := dim.width.toNat
  let height := dim.height.toNat
  let pixels := width * height
  let as32 := pixels * 4
  let stride32 := 4 * stride8 width * height
  let as8 := paletteBytes + stride8 width * height
  let bare8 := stride8 width * height
  let some format := pixelFormat container index data bank
    | throw s!"resource {index} ('{container.name index}'): {data.size} bytes match neither the 32bpp ({as32} or {stride32} padded) nor the 8bpp ({as8}, bare {bare8}) layout of a {width}x{height} picture"
  let rgba ←
    match format with
    | .transpRgb32 =>
      if data.size < as32 then
        throw s!"resource {index} ('{container.name index}'): {data.size} bytes for a {width}x{height} 32bpp picture, need {as32}"
      let mut out := ByteArray.emptyWithCapacity as32
      for i in [0:pixels] do
        let slot := i * 4
        out := out.append (toRgba data[slot]! data[slot + 1]! data[slot + 2]! data[slot + 3]!)
      pure out
    | .transpRgb32Padded =>
      -- Rows are `4*stride8(width)` bytes apart; the tail of each row is
      -- padding and is skipped.
      let row := 4 * stride8 width
      let mut out := ByteArray.emptyWithCapacity as32
      for y in [0:height] do
        let start := y * row
        let stop := min data.size (start + width * 4)
        for i in [start:stop] do
          if (i - start) % 4 != 0 then
            continue
          out := out.append (toRgba data[i]! data[i + 1]! data[i + 2]! data[i + 3]!)
        for _ in [(stop - start) / 4:width] do
          out := out.append (ByteArray.mk #[0, 0, 0, 0])
      pure out
    | .paletted8 =>
      pure (indexedRgba (data.extract 0 paletteBytes)
        (data.extract paletteBytes data.size) width height)
    | .alphaMask8 =>
      pure (maskRgba data width height)
    | .runLengthRle =>
      match rleRgba data width height bank with
      | some rgba => pure rgba
      | none => pure ByteArray.empty
  return { width, height, format, rgba }

/-- The payload of picture resource `index`, after the decompression step this
container version applies.

A zero in the container's decompressed-size table means the payload was stored
uncompressed and is returned as-is; any other value means it is a byte-oriented
LZ77 stream of that many bytes.

This is also what the `--original` mode writes out, so the two paths share it
rather than each re-deriving the rule: the file written is exactly the bytes
the game's runtime would hand to its blitter.
-/
def payload (container : Container) (index : Nat) : Except String ByteArray := do
  let raw ← container.payload index
  if (container.sizes[index]?).getD 0 == 0 then
    return raw
  match Lz77.decompress raw with
  | .ok out => return out
  | .error e =>
    throw s!"decompressing resource {index} ('{container.name index}'): {e}"

/-- Decode picture resource `index` of `container`.

This is the whole path for a container whose payloads need only the shared LZ77
step. A container version that compresses payloads differently should expand its
payload itself and call [`decodeBytes`]; that division is what keeps this
function free of other versions' business.
-/
def decode (container : Container) (index : Nat) : Except String Decoded := do
  let data ← payload container index
  decodeBytes container index data none

/-- Write a decoded picture as a PNG file. -/
def writePng (path : System.FilePath) (decoded : Decoded) : IO Unit := do
  let expected := decoded.width * decoded.height * 4
  if decoded.rgba.size < expected then
    throw (IO.userError s!"{MmfExtract.Path.str path}: picture has {decoded.rgba.size} bytes but {decoded.width}x{decoded.height} needs {expected}")
  IO.FS.writeBinFile path
    (Png.ofRgba decoded.width decoded.height (decoded.rgba.extract 0 expected))

end MmfExtract.Picture
