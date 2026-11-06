/-
Copyright (c) 2026 the mmf-extract authors. All rights reserved.
Released under the same licence as the Rust reference this mirrors.
-/
import MmfExtract.Container

/-!
The 3D game's `Models` container: reading a **drawing profile** out of a payload.

`src/dma3d/modfile.rs` is the reference implementation, and `docs/DMA3D.md`
§11.25 is the measurement this reads:

* a payload that carries no mesh chain is a 16-byte record stream of
  little-endian `f32`s;
* **one slot of every record is a constant** — `0.0`, which is what a normal of
  the mesh chain carries (`abs w < 1e-6`), or `1.0`, which is what a vertex
  carries (`abs (w - 1.0) < 1e-6`) — and the other three are the point;
* the record stream starts after a **prefix whose length is not in the payload**
  (`mod II` sits at offset 3 in one resource and at 24 in another, §11.9,
  §11.22), so the phase is measured, not assumed.

The engine tints and extrudes a profile at runtime, and that tessellation is not
in the payload, so the modern output is the point stream itself — a glTF line
loop (`docs/DMA3D.md` §11.26), never a surface it would have to invent.
-/

namespace MmfExtract.Dma3d.Modfile

open MmfExtract

/-- Bytes per record: three `f32`s and the constant slot. -/
def recordBytes : Nat := 16

/-- The two constants the slot may hold: `0.0`, a normal's `w`, and `1.0`, a
vertex's `w`. -/
def markers : Array Float := #[0.0, 1.0]

/-! ## Reading the container's little-endian values -/

/-- A little-endian `u16` at `off`, as `src/dma3d/modfile.rs::u16_at` reads it. -/
def leU16At (payload : ByteArray) (off : Nat) : Option UInt16 :=
  if off + 2 > payload.size then
    none
  else
    some ((payload[off]!.toUInt16 <<< 0) ||| (payload[off + 1]!.toUInt16 <<< 8))

/-- A little-endian `u32` at `off`, as `src/dma3d/modfile.rs::u32_at` reads it. -/
def leU32At (payload : ByteArray) (off : Nat) : Except String UInt32 := do
  if off + 4 > payload.size then
    throw s!"a little-endian u32 at {off} is past the payload's {payload.size} bytes"
  return (payload[off]!.toUInt32 <<< 0) ||| (payload[off + 1]!.toUInt32 <<< 8)
    ||| (payload[off + 2]!.toUInt32 <<< 16) ||| (payload[off + 3]!.toUInt32 <<< 24)

/-- A little-endian `f32` at `off`, or an error naming the four bytes it read.

The same reinterpretation `src/dma3d/modfile.rs::f32_at` does: the four bytes are
a bit pattern, and their value is whatever that pattern is, including a denormal
and including NaN. Exported so a check can read the payload back independently
of the reader.
-/
def leF32 (payload : ByteArray) (off : Nat) : Except String Float := do
  let u ← leU32At payload off
  -- `Float.ofBits` takes the 64-bit pattern of a *double*, so it returns a
  -- denormal for a single-precision pattern and reads `0x3f800000` as zero
  -- rather than as one. `Float32.ofBits` is the single-precision reading, and
  -- `.toFloat` widens it exactly.
  return (Float32.ofBits u).toFloat

def f32At := @leF32

/-- True when `f` reads as a coordinate this game can hold.

`src/dma3d/modfile.rs::plausible` rejects NaN, infinity, absurd magnitudes and
**denormals**: a shifted boundary lands inside a neighbouring `f32`'s mantissa
and produces about `1e-40`, which no coordinate here takes. Admitting them let a
payload of identical padding read as a profile at phase 3 with a marker of
exactly `0.0`, which is how that case was caught (`docs/DMA3D.md` §11.26).
-/
def plausible (f : Float) : Bool :=
  f.isFinite && f.abs < 1.0e7 && (f == 0.0 || f.abs >= 1.1754943508222875e-38)

/-! ## The reading -/

/-- One decoded drawing profile: where the record stream starts, which slot holds
the constant, the constant itself, and the point stream. -/
structure Profile where
  /-- Byte offset the records start at, after the prefix. One of 0..8. -/
  phase : Nat
  /-- Which of the four slots holds the constant. One of 0..4. -/
  constantSlot : Nat
  /-- The constant itself, exactly `0.0` or exactly `1.0`. -/
  marker : Float
  /-- The records, in payload order, as the three varying slots. -/
  points : Array (Float × Float × Float)
  /-- Byte offset just past the last record read, i.e. the start of the part the
  reader did not read. A payload carries its record run and then whatever else it
  holds, exactly as a mesh payload is a chain followed by a tail
  (`docs/DMA3D.md` §3.4, §11.28). -/
  tailOffset : Nat := 0
  deriving Inhabited, BEq

/-- Where the `mod II` magic sits, if the payload carries one.

`docs/DMA3D.md` §11.9 recorded that the magic is at scattered offsets behind the
header, and §11.31 that the record stream begins ten bytes past it.
-/
def offsetOfMagic (payload : ByteArray) : Option Nat := Id.run do
  let magic : ByteArray := "mod II".toUTF8
  for start in [0:payload.size] do
    if start + magic.size > payload.size then
      break
    let mut isMagic := true
    for k in [0:magic.size] do
      if payload[start + k]! != magic[k]! then
        isMagic := false
    if isMagic then
      return some start
  return none

/-- Read the record at `phase + 16 * record` as four floats, in slot order. -/
private def recordAt (payload : ByteArray) (phase record : Nat) :
    Except String (Array Float) := do
  let mut quad : Array Float := #[]
  for slot in [0:4] do
    quad := quad.push (← f32At payload (phase + 16 * record + 4 * slot))
  return quad

/-- The three slots other than `slot`, in order. -/
private def others (quad : Array Float) (slot : Nat) : Array Float := Id.run do
  let mut rest : Array Float := #[]
  for i in [0:4] do
    if i != slot then
      rest := rest.push quad[i]!
  return rest

/-! ## The mesh chain -/

/-- Bytes per vertex record: three `f32`s and the `w` of `1.0`. -/
def vertexRecordBytes : Nat := 16
/-- Bytes per normal record: three `f32`s and the `w` of `0.0`. -/
def normalRecordBytes : Nat := 16

/-- One decoded mesh chain, the same shape `src/dma3d/modfile.rs::ModelMesh`
carries. -/
structure Mesh where
  /-- Where the index count was read from, for diagnostics. -/
  indexCountOffset : Nat
  /-- The index list, zero-based exactly as stored. -/
  indices : Array UInt16
  /-- World-space positions, `[x, y, z]` per vertex. -/
  positions : Array (Float × Float × Float)
  /-- Unit-length normals, `[nx, ny, nz]` per vertex. -/
  normals : Array (Float × Float × Float)
  /-- The three `u32`s that open the tail, when they are `[0, 1, V]`. -/
  tailHeader : Option (UInt32 × UInt32 × UInt32)
  deriving Inhabited

/-- The index count at `off`, if it can start a chain that fits the payload.

`src/dma3d/modfile.rs::count_at`, and the bound is the one it uses.
-/
private def countAt (payload : ByteArray) (off : Nat) : Option UInt32 :=
  if off + 4 > payload.size then
    none
  else
    match leU32At payload off with
    | Except.error _ => none
    | Except.ok c => if c < 1 || c > 400000 then none else some c

/-- The mesh chain that starts at `off`, if the structural signature holds there.

`src/dma3d/modfile.rs::parse_mesh` reads the chain **structurally**, because the counts
are not in the header: a count, that many zero-based indices all below a vertex
count, that many vertices whose `w` is `1.0`, and a second count equal to it.
-/
private def meshAt (payload : ByteArray) (off : Nat) : Option Mesh := Id.run do
  let some c := countAt payload off | return none
  let someC := c.toNat
  let indicesEnd := off + 4 + 2 * someC
  if indicesEnd + 4 > payload.size then
    return none
  let .ok vRaw := leU32At payload indicesEnd | return none
  if vRaw < 1 || vRaw > 100000 then
    return none
  let v := vRaw.toNat
  let chainAt := off
  let verticesStart := indicesEnd + 4
  let normalsCountOffset := verticesStart + vertexRecordBytes * v
  if normalsCountOffset + 4 > payload.size then
    return none
  let .ok second := leU32At payload normalsCountOffset | return none
  if second != vRaw then
    return none
  let normalsStart := normalsCountOffset + 4
  if normalsStart + normalRecordBytes * v > payload.size then
    return none
  -- Every index must name a vertex, and they must not all be the same value: a
  -- table of one repeated index is not an index list.
  let mut indices : Array UInt16 := #[]
  let mut distinct := 0
  let mut bad := false
  for k in [0:someC] do
    match leU16At payload (off + 4 + 2 * k) with
    | none => bad := true
    | some i =>
      -- Counted **before** the push: `src/dma3d/modfile.rs` collects the indices
      -- into a set and checks its size, and checking `contains` after pushing
      -- makes it always true, so `distinct` stays 0 and every chain is rejected.
      if !(indices.contains i) then
        distinct := distinct + 1
      indices := indices.push i
      if i.toNat >= v then
        bad := true
  if bad || distinct < 2 then
    return none
  -- The vertices: w == 1.0 for all but a few, and every coordinate real.
  let mut positions : Array (Float × Float × Float) := #[]
  let mut marked := 0
  let mut badVertex := false
  for k in [0:v] do
    let off := verticesStart + vertexRecordBytes * k
    match leF32 payload off, leF32 payload (off + 4), leF32 payload (off + 8), leF32 payload (off + 12) with
    | _, _, _, Except.error _ => badVertex := true
    | Except.error _, _, _, _ => badVertex := true
    | _, Except.error _, _, _ => badVertex := true
    | _, _, Except.error _, _ => badVertex := true
    | Except.ok x, Except.ok y, Except.ok z, Except.ok w =>
      if !(plausible x) || !(plausible y) || !(plausible z) then
        badVertex := true
      else
        -- Rust counts the vertices whose `w` **is** the marker, and requires
        -- nine in ten of them: `marked * 10 < v * 9` rejects. Counting the
        -- ones that are *not* inverts the test, and with `w == 1.0` everywhere
        -- `marked` stays 0 and every chain is rejected.
        if (w - 1.0).abs <= 1.0e-6 then
          marked := marked + 1
        positions := positions.push (x, y, z)
  if badVertex || marked * 10 < v * 9 then
    return none
  -- The normals: `w == 0.0` for at least nine in ten, unit length, real.
  let mut normals : Array (Float × Float × Float) := #[]
  let mut unit := 0
  let mut badN := false
  for k in [0:v] do
    let off := normalsStart + normalRecordBytes * k
    match leF32 payload off, leF32 payload (off + 4), leF32 payload (off + 8), leF32 payload (off + 12) with
    | _, _, _, Except.error _ => badN := true
    | Except.error _, _, _, _ => badN := true
    | _, Except.error _, _, _ => badN := true
    | _, _, Except.error _, _ => badN := true
    | Except.ok x, Except.ok y, Except.ok z, Except.ok w =>
      let length := (x * x + y * y + z * z).sqrt
      if !(plausible x) || !(plausible y) || !(plausible z) || (length - 1.0).abs > 1.0e-3 then
        badN := true
      else
        if w.abs <= 1.0e-6 then
          unit := unit + 1
        normals := normals.push (x, y, z)
  if badN || normals.size != v || unit * 10 < v * 9 then
    return none
  -- The tail's opening words, read when they are the shape most payloads carry.
  -- A measurement, not a requirement (`docs/DMA3D.md` §3.4).
  let tailAt := normalsStart + normalRecordBytes * v
  let mut tailHeader : Option (UInt32 × UInt32 × UInt32) := none
  if tailAt + 12 <= payload.size then
    match leU32At payload tailAt, leU32At payload (tailAt + 4), leU32At payload (tailAt + 8) with
    | .ok a, .ok b, .ok c2 =>
      if a == 0 && b == 1 && c2 == vRaw then
        tailHeader := some (a, b, c2)
    | _, _, _ => pure ()
  return some { indexCountOffset := chainAt, indices, positions, normals, tailHeader }

def parseMesh (payload : ByteArray) : Except String Mesh := do
  -- 6000 is well past every measured header (163-263) and keeps a corrupt file
  -- from being scanned end to end, exactly as the Rust reader bounds it.
  let limit := min 6000 payload.size
  for off in [6:limit] do
    if let some m := meshAt payload off then
      return m
  throw "no mesh chain in the payload"

/-! ## The `mod II` record stream -/

/-- One `mod II` record stream that is neither a mesh nor a drawing profile.

`docs/DMA3D.md` §11.31, §11.32, §11.38: a payload is a header, a list of texture
names, the `mod II` magic with a `u32` after it, and a record stream. The mesh reader
and the profile reader each claim the payloads whose records are the shape they read;
**48 of the shipped payloads carry the magic and the `u32` and neither reader's
shape**. This is the reader for those, and it deliberately does not guess what the
records mean: §11.32 measured that no position in the stream holds a constant across
payloads, so the record schema is not derivable from the bytes.
-/
structure MagicRecord where
  /-- Byte offset of the `mod II` magic. -/
  magicOffset : Nat
  /-- The `u32` after the magic: the record type, as the game's own reader treats it
  (`docs/DMA3D.md` §11.9, §11.20). -/
  recordType : UInt32
  /-- The `u16` stream after the magic, in payload order, little-endian. -/
  pairs : Array UInt16
  /-- The `f32` values the same bytes hold at four-byte steps from the stream's
  start, **only** those that read as numbers: a denormal is the signature of a
  misaligned read (§11.26), so including them would report noise as data. -/
  floats : Array Float
  deriving Inhabited

/-- Byte offset just past the last record this read. -/
def MagicRecord.end (r : MagicRecord) : Nat :=
  r.magicOffset + 10 + 2 * r.pairs.size

/-- Read a `mod II` record stream that is neither a mesh nor a drawing profile.

The payload must carry the magic and the `u32` after it; a payload that carries
neither is an error, so the caller reports it rather than writing a document that
describes nothing.
-/
def parseMagicRecord (payload : ByteArray) : Except String MagicRecord := do
  let some magic := offsetOfMagic payload
    | throw "no `mod II` magic, so there is no record stream to describe"
  if magic + 10 > payload.size then
    throw "the `mod II` magic is too close to the payload's end to carry its u32"
  let recordType ← leU32At payload (magic + 6)
  let stream := magic + 10
  -- Every pair the payload leaves room for: "payload length is ground truth"
  -- (`AGENTS.md`), the same rule the other readers apply.
  let mut pairs : Array UInt16 := #[]
  for k in [0:(payload.size - stream) / 2] do
    match leU16At payload (stream + 2 * k) with
    | none => break
    | some v => pairs := pairs.push v
  let mut floats : Array Float := #[]
  for k in [0:(payload.size - stream) / 4] do
    match leF32 payload (stream + 4 * k) with
    | Except.error _ => break
    | Except.ok f => if plausible f then floats := floats.push f
  return { magicOffset := magic, recordType, pairs, floats }

/-! ## The parameter table -/

/-- How much of the fill half must be zero for a payload to be this shape. -/
def fillZeroPercent : Nat := 80
/-- How much of the carrying half must be below 128 for it to be small values. -/
def carrySmallPercent : Nat := 95
/-- How many values a payload must carry for the reading to be a table at all. -/
def minTableValues : Nat := 8

/-- One parameter table: small values at alternating byte positions with zero fill.

`docs/DMA3D.md` §11.37, §11.39 measured this shape in the shipped container and
named it what it is: a table of small values stored at alternating byte positions,
with zero fill — a parameter table, an index table, or a packed enum, which the game
reads to configure a UI element or a material rather than to describe its outline.
`createModBtSize02`, `athCircle`, `athFig8R` are all of it, which is what their
names already said.

This reads it and deliberately does not guess what the values mean: the payload
carries no field names and no lengths, so the table is recorded and the reason each
value means anything is left to the executable's reader, which this repository
cannot reach (`docs/DMA3D.md` §11.23, §11.24, §11.36).
-/
structure ParamTable where
  /-- Whether the even-indexed bytes are the zero fill and the odd ones the values.
  It is measured, because a payload's fill is not at a fixed parity — §11.37 found
  both. -/
  evenIsFill : Bool
  /-- The values, in payload order, as the carrying half's bytes. -/
  values : Array UInt8
  /-- What fraction of the fill half is zero, in percent. Recorded so the reading's
  own strength is visible rather than assumed. -/
  zeroPercent : Nat
  /-- What fraction of the carrying half is below 128, in percent. -/
  smallPercent : Nat
  deriving Inhabited

/-- Read a parameter table out of a payload that fits no other reader.

The thresholds are stated rather than tuned: §11.37 measured the shape as "small
values at alternating byte positions with zero fill", and a payload is accepted when
at least 80% of one parity is zero, at least 95% of the other is below 128, and there
are at least eight values. A payload outside all three is an error, so the caller
reports it rather than writing a document that describes nothing.
-/
def parseParamTable (payload : ByteArray) : Except String ParamTable := do
  if payload.size < 2 * minTableValues then
    throw s!"{payload.size} bytes cannot hold the {minTableValues} values a table needs"
  let half := payload.size / 2
  let mut evenZero := 0
  let mut oddZero := 0
  for k in [0:half] do
    if payload[2 * k]! == 0 then
      evenZero := evenZero + 1
    if payload[2 * k + 1]! == 0 then
      oddZero := oddZero + 1
  let mut evenIsFill := false
  if evenZero * 100 >= half * fillZeroPercent then
    evenIsFill := true
  else if !(oddZero * 100 >= half * fillZeroPercent) then
    throw s!"neither byte parity is {fillZeroPercent}% zero, so this is not the alternating-fill shape (docs/DMA3D.md 11.37)"
  let zeroPercent := if evenIsFill then evenZero * 100 / half else oddZero * 100 / half
  let mut values : Array UInt8 := #[]
  for k in [0:half] do
    let off := if evenIsFill then 2 * k + 1 else 2 * k
    values := values.push payload[off]!
  let mut small := 0
  for v in values do
    if v < 128 then
      small := small + 1
  if small * 100 < values.size * carrySmallPercent then
    throw s!"only {small} of {values.size} values are below 128, so the carrying half is not small values"
  return { evenIsFill, values, zeroPercent, smallPercent := small * 100 / values.size }

/-! ## The texture-name table -/

/-- The shortest run of printable characters that counts as a name.

Eight, not four: at four the reading picks up noise -- `DDD?`, `fff?`, `>1:A?` are
all four printable characters terminated by a NUL, and 20 of the 39 payloads a
four-character threshold accepts carry nothing else. At eight it takes the payloads
carrying real ones, `createBtFill.00.TGA`, `ParentalBtCheckboxY.mod`
(`docs/DMA3D.md` §11.40).
-/
def minNameLen : Nat := 8

/-- One table of texture names: the payload's list of the textures it references.

`docs/DMA3D.md` §11.40: 19 of the shipped payloads that no other reader claims carry
a table of NUL-terminated ASCII names in 32-byte records, each padded with `0xcc`
fill. `createBtFill.00.TGA`, `createBtShape.09.TGA`, `createBtSelect.25.tga` -- a
payload's own list of what it references. It is the same field
`src/dma3d.rs::texture_names_in` already reads out of mesh payloads to embed their
materials, so this is not a new layout: it is the field the reader already knows, in
the payloads it did not look at.
-/
structure NameTable where
  /-- The names, in payload order. -/
  names : Array String
  deriving Inhabited

/-- Read a table of texture names out of a payload.

A name is a run of printable ASCII terminated by a NUL, at least `minNameLen`
characters long so a stray byte inside padding is not a name. A payload carrying
fewer than three names is an error, so the caller reports it rather than writing a
document that describes nothing.

No `firstIsTruncated` field, and that is deliberate: some payloads do begin inside
a name -- `teBtFill.18.TGA` for `createBtFill.18.TGA`, because the container's
slice starts three bytes into a record -- but a payload's first byte is the name's
first byte either way, so the bytes cannot tell the two apart. `AGENTS.md` sets a
payload aside rather than guessing it.
-/
def parseNameTable (payload : ByteArray) : Except String NameTable := Id.run do
  let mut names : Array String := #[]
  let mut k := 0
  while k < payload.size do
    let mut j := k
    let mut printable := true
    while j < payload.size && printable do
      let b := payload[j]!
      if b == 0 || b < 0x21 || b > 0x7e then
        printable := false
      else
        j := j + 1
    if j >= k + minNameLen && j < payload.size && payload[j]! == 0 then
      let mut s := ""
      for m in [k:j] do
        s := s.push (Char.ofNat (payload[m]!).toNat)
      names := names.push s
      k := j + 1
    else
      k := if j > k then j else k + 1
  if names.size < 3 then
    return .error s!"{names.size} names is fewer than the 3 a table needs"
  return .ok { names }

/-- Read the drawing profile out of a `Models` payload.

The reader measures the phase and the constant slot rather than assuming them. For
each pair it asks whether one slot is constant across every record the payload
leaves room for and is exactly `0.0` or exactly `1.0`; whether the other three
slots are plausible in at least 70% of the records; and whether at least two
records carry a varying value that is not zero — which last rule is what keeps a
payload of padding from reading as a profile.

A payload that fits no pair is an error, so the caller reports it per resource
rather than skipping it (`AGENTS.md`).
-/
def parse (payload : ByteArray) : Except String Profile := do
  if payload.size < 4 + 5 * recordBytes then
    throw s!"{payload.size} bytes cannot hold the records a profile needs"
  -- The best pair found, as (records explained, phase, slot, marker, points).
  let mut best : Option (Nat × Nat × Nat × Float × Array (Float × Float × Float)) := none
  -- Where the record stream starts. The prefix (`docs/DMA3D.md` §11.31, §11.32) is
  -- a header, a list of texture names, and the `mod II` magic with the `u32` after
  -- it, and the records begin ten bytes past the magic -- so the magic is an
  -- **anchor** for the start, which the 0..8 phase scan cannot be: a phase scan
  -- finds an alignment, and the records begin at `magic + 10`, an offset that scan
  -- never reaches.
  let mut starts : Array Nat := #[]
  if let some magic := offsetOfMagic payload then
    for step in [0:4] do
      starts := starts.push (magic + 10 + 16 * step)
  for phase in (starts ++ (List.range 8).toArray) do
    if phase + 3 * recordBytes > payload.size then
      continue
    let records := (payload.size - phase) / recordBytes
    for slot in [0:4] do
      -- The marker is a stored constant, so it is *bit-identical* across the
      -- records, and it is exactly `0.0` or exactly `1.0` — the `w` a normal and
      -- a vertex carry in the mesh chain. Comparing within an epsilon let a
      -- denormal pass as zero, and the payloads that carry those are
      -- unreadable-name records whose bytes are not a point stream at all.
      -- Reading the raw `u32` and comparing bits is what keeps the two halves
      -- identical to `src/dma3d/modfile.rs`.
      let markerBits ← leU32At payload (phase + 4 * slot)
      if markerBits != 0 && markerBits != 0x3f800000 then
        continue
      let marker ← f32At payload (phase + 4 * slot)
      -- The run is the **prefix** in which the marker is bit-identical and the
      -- other three slots are plausible coordinates. A payload carries its point
      -- stream and then whatever else it holds, so requiring the constant across
      -- every record the payload leaves room for rejects payloads that carry a
      -- clean run followed by something else (`docs/DMA3D.md` §11.28).
      let mut points : Array (Float × Float × Float) := #[]
      let mut nonconstantRecords := 0
      for record in [0:records] do
        let base := phase + 16 * record
        let heldBits ← leU32At payload (base + 4 * slot)
        if heldBits != markerBits then
          break
        let mut rest : Array Float := #[]
        let mut plausibleHere := true
        let mut anyNonzero := false
        for other in [0:4] do
          if other != slot then
            let v ← f32At payload (base + 4 * other)
            if !(plausible v) then
              plausibleHere := false
            if v != 0.0 then
              anyNonzero := true
            rest := rest.push v
        if !plausibleHere then
          break
        if anyNonzero then
          nonconstantRecords := nonconstantRecords + 1
        points := points.push (rest[0]!, rest[1]!, rest[2]!)
      -- Three records to call it a run, two of them carrying a real coordinate
      -- so a payload of padding is not a profile.
      if points.size < 3 || nonconstantRecords < 2 then
        continue
      let score := points.size * 100 + nonconstantRecords
      match best with
      | some (bestRecords, _, _, _, _) =>
        if score > bestRecords then
          best := some (score, phase, slot, marker, points)
      | none => best := some (score, phase, slot, marker, points)
  match best with
  | some (_, phase, slot, marker, points) =>
    return { phase, constantSlot := slot, marker, points,
             tailOffset := phase + 16 * points.size }
  | none =>
    throw "no f32 point stream fits this payload (`docs/DMA3D.md` §11.25)"

end MmfExtract.Dma3d.Modfile
