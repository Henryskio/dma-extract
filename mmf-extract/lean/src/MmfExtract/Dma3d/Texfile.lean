import MmfExtract.Bytes

/-!
The 3D game's texture payloads.

A port of `src/dma3d/texfile.rs`. A payload is a palette followed by index
bytes; the split is the smallest `K` for which every byte past `4*K` is a valid
index, which is the only self-consistent reading: a palette of `K` entries has
to hold every value the indices can name and cannot be shorter than the largest
one. The payload's length confirms it, because `payload_len - 4*K` then lands
on a power of two in all 705 shipped textures.

The width and the height come from the index area, which is a power of two in
every shipped texture: a perfect square reads as its root, and a 1:2 rectangle
is settled by a statistic with two vetoes on top of it (`docs/DMA3D.md` §2.1).
-/

namespace MmfExtract.Dma3d.Texfile

/-- Bytes per palette entry: straight RGBA, no premultiplication. -/
def paletteEntryBytes : Nat := 4

/-- The widths a row-structure test considers.

One byte holds 256 values, so a texture wider than 256 bytes is impossible; the
set is the powers of two an engine of this era would allocate.
-/
def widthCandidates : Array Nat := #[8, 16, 32, 64, 128, 256]

/-- Which of the two readings of a 1:2 rectangle was taken.

Named `TextureAxis` so it does not collide with the field it lands in.
-/
inductive TextureAxis where
  /-- The area is a perfect square and its root was read as both dimensions. -/
  | square
  /-- The area is a 1:2 rectangle; the longer side is the width. -/
  | wide
  /-- The area is a 1:2 rectangle; the longer side is the height. -/
  | tall
  deriving Inhabited, DecidableEq, Repr

/-- One decoded texture. -/
structure Texture where
  /-- The palette, in file order: entry `i` is four bytes. -/
  palette : Array (UInt8 × UInt8 × UInt8 × UInt8)
  /-- The index pixels, row-major, `width` of them to a row. -/
  indices : ByteArray
  width : Nat
  height : Nat
  /-- How the width and the height were decided, for the report. -/
  axis : TextureAxis
  /-- True when the index data carries no orientation information at all, so
  the reading is a default rather than a measurement.

  `createBtFileIo.TGA` is the one: `K == 1`, a single palette entry and 2048
  identical index bytes. Every statistic is degenerate at every width, so no
  reading differs from any other and the only honest things to do are to report
  it and to say why. -/
  undecidable : Bool
  deriving Inhabited

/-- Expand to straight RGBA, four bytes per pixel. -/
def Texture.toRgba (texture : Texture) : ByteArray := Id.run do
  let mut out := ByteArray.emptyWithCapacity (texture.indices.size * 4)
  for i in [0:texture.indices.size] do
    let index := texture.indices[i]!
    let entry := (texture.palette[index.toNat]?).getD (0, 0, 0, 0)
    out := out.append (ByteArray.mk #[entry.1, entry.2.1, entry.2.2.1, entry.2.2.2])
  return out

/-- The palette entry for an index, or transparent black if the index names an
entry the palette does not have. -/
def Texture.entry (texture : Texture) (index : UInt8) : UInt8 × UInt8 × UInt8 × UInt8 :=
  (texture.palette[index.toNat]?).getD (0, 0, 0, 0)

/-- The palette size: the smallest `K` for which every byte past `4*K` is a
valid index. -/
def paletteSize (payload : ByteArray) : Except String Nat := do
  for k in [1:payload.size / paletteEntryBytes + 1] do
    let start := k * paletteEntryBytes
    if start >= payload.size then
      throw s!"payload is all palette: {k} entries with no index bytes"
    let mut largest : Nat := 0
    for i in [start:payload.size] do
      largest := max largest payload[i]!.toNat
    if largest < k then
      return k
  throw "no palette size makes the trailing bytes valid indices"

/-- The 1:2 pair an area of `2^(2k+1)` is made of.

Taking `sqrt` and halving it does not work: it lands on an odd number and would
print a reading like `45x22` that no texture in this engine has. The area is a
power of two, so the two factors are powers of two too.
-/
def wideTall (area : Nat) : Nat × Nat :=
  let bits := Nat.log2 area
  (1 <<< (bits / 2 + 1), 1 <<< (bits / 2))

/-- Mean absolute index difference across a vertical step. -/
private def verticalDelta (indices : ByteArray) (w h : Nat) : Float :=
  if h < 2 then
    0.0
  else Id.run do
    let mut total : Nat := 0
    let mut pairs : Nat := 0
    for row in [0:h - 1] do
      let base := row * w
      for col in [0:w] do
        let a := indices[base + col]!.toNat
        let b := indices[base + w + col]!.toNat
        total := total + (if a > b then a - b else b - a)
        pairs := pairs + 1
    return total.toFloat / pairs.toFloat

/-- The background index: the palette's transparent colour key when it has one,
otherwise the index the data uses most.

Without a key the second choice is the only "empty" a texture has, and it is the
one an engine would leave the untouched part of the buffer holding.
-/
private def background (indices : ByteArray) (paletteLen : Nat) : Nat :=
  if paletteLen == 1 then
    0
  else Id.run do
    let mut counts : Array Nat := Array.replicate paletteLen 0
    for i in [0:indices.size] do
      let index := indices[i]!.toNat
      if index < paletteLen then
        counts := counts.set! index (counts[index]! + 1)
    let mut best : Nat := 0
    for i in [0:counts.size] do
      if counts[i]! > counts[best]! then
        best := i
    return best

/-- The shape of the non-background pixels at one reading. -/
private structure Shape where
  /-- 4-connected components of non-background pixels. -/
  components : Nat
  /-- Non-background pixels sitting on the frame's border: an object the reading
  has cut in half. -/
  cut : Nat

/-- Count components and cut pixels for one reading. -/
private def shape (indices : ByteArray) (w h : Nat) : Shape := Id.run do
  let mut largest : Nat := 0
  for i in [0:indices.size] do
    largest := max largest indices[i]!.toNat
  let paletteLen := largest + 1
  let bg := background indices paletteLen
  let mut seen := Array.replicate (w * h) false
  let mut components : Nat := 0
  let mut cut : Nat := 0
  for row in [0:h] do
    for col in [0:w] do
      let here0 := row * w + col
      if seen[here0]! || indices[here0]!.toNat == bg then
        continue
      -- A new component: flood fill it, so the count does not depend on the
      -- scan order.
      if row == 0 || row + 1 == h || col == 0 || col + 1 == w then
        cut := cut + 1
      components := components + 1
      seen := seen.set! here0 true
      let mut stack : Array Nat := #[here0]
      while !stack.isEmpty do
        let here := stack.back!
        stack := stack.pop
        let r := here / w
        let c := here % w
        for (dr, dc) in [(-1, 0), (1, 0), (0, -1), (0, 1)] do
          let nr : Int := r + dr
          let nc : Int := c + dc
          if nr < 0 || nc < 0 || nr >= h || nc >= w then
            continue
          let next := nr.toNat * w + nc.toNat
          if seen[next]! || indices[next]!.toNat == bg then
            continue
          seen := seen.set! next true
          if nr == 0 || nr + 1 == h || nc == 0 || nc + 1 == w then
            cut := cut + 1
          stack := stack.push next
  return { components, cut }

/-- True when the `wide` reading is the right one.

The rule is **the mean index difference across a vertical step, with two vetoes
on top of it**. At the correct width a vertical step is one true row, so a
smooth image is smooth; at twice the width it is two true rows, so it is not.
Validated on the 678 square textures, where this statistic lands on the root
659/678 against 649/678 for the row-continuity it replaces.

The vetoes, both of which are about the *shape* of the picture and both strong
enough to override the statistic:

1. **one connected component.** At the correct width an icon is one object; at
   the wrong width its rows have been concatenated and it is sliced into two or
   more. A reading that is uniquely `components == 1` wins even when the
   statistic disagrees — `createModMickeyPartHeadS.TGA` is exactly that case,
   and it is the reason the vetoes are applied first rather than the statistic.
2. **zero pixels cut on the frame border.** A non-background pixel on the
   border is an object the reading has cut in half.

Returns `false` for both when the index data cannot tell the two apart — every
byte identical — so the caller reports the texture as undecidable rather than as
decided.
-/
def decide (indices : ByteArray) (wide tall : Nat) : Bool × Bool :=
  let wideShape := shape indices wide tall
  let tallShape := shape indices tall wide
  let wideDelta := verticalDelta indices wide tall
  let tallDelta := verticalDelta indices tall wide

  -- The vetoes, strongest first. Each is applied only when it is *unique*, so
  -- two readings that both cut the border or both hold several objects fall
  -- through to the statistic.
  if wideShape.components == 1 && tallShape.components != 1 then
    (true, true)
  else if tallShape.components == 1 && wideShape.components != 1 then
    (false, true)
  else if wideShape.cut == 0 && tallShape.cut != 0 then
    (true, true)
  else if tallShape.cut == 0 && wideShape.cut != 0 then
    (false, true)
  else
    -- The statistic, then. A tie is a tie: the pixel data says nothing, and a
    -- coin flip dressed as a measurement is what this whole decision exists to
    -- avoid.
    if wideDelta < tallDelta then
      (true, true)
    else if wideDelta > tallDelta then
      (false, true)
    else
      (false, false)

/-- Split an index count into a width and a height. -/
def splitArea (indices : ByteArray) (area : Nat) : Nat × Nat × TextureAxis :=
  let root := Nat.sqrt area
  if root * root == area && (widthCandidates.contains root) then
    (root, root, TextureAxis.square)
  else
    let (w, h) := wideTall area
    if (decide indices w h).1 then
      (w, h, TextureAxis.wide)
    else
      (h, w, TextureAxis.tall)

/-- Whether `n` is a power of two. `0` is not, because no texture has no pixels
and a zero area would make every width fit. -/
def isPowerOfTwo (n : Nat) : Bool := n > 0 && (n &&& (n - 1)) == 0

/-- Read a texture out of a `Textures` container payload.

The payload is the whole resource as stored, so the byte-exact `--original`
path never comes through here.
-/
def parse (payload : ByteArray) : Except String Texture := do
  let k ← paletteSize payload
  let paletteBytes := k * paletteEntryBytes
  let mut palette : Array (UInt8 × UInt8 × UInt8 × UInt8) := #[]
  for entry in [0:k] do
    let slot := entry * paletteEntryBytes
    palette := palette.push (
      (payload[slot]!), (payload[slot + 1]!), (payload[slot + 2]!), (payload[slot + 3]!))
  let indices := payload.extract paletteBytes payload.size
  let area := indices.size

  -- The area is a power of two in all 705 shipped textures, and that is what
  -- makes the root and the 1:2 pair exact. A payload whose area is not one does
  -- not fit this format, and is reported rather than factored into a width no
  -- texture in this engine can have.
  if !isPowerOfTwo area then
    throw s!"{area} index bytes is not a power of two, so no width and height fit"

  let (width, height, axis) := splitArea indices area
  if width * height != area then
    throw s!"{area} index bytes do not factor into {width}x{height}"

  -- Whether the pixel data could tell the two readings apart at all. It is
  -- recorded rather than acted on: an undecidable texture is still written,
  -- because a solid rectangle is a usable placeholder, and it is reported so
  -- nobody mistakes the default for a measurement.
  let undecidable := area.log2 % 2 == 1 && !(decide indices (wideTall area).1 (wideTall area).2).2

  return { palette, indices, width, height, axis, undecidable }

end MmfExtract.Dma3d.Texfile
