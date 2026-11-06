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

/-- How close a float must be to a marker to count as it. -/
def markerEpsilon : Float := 1.0e-6

/-! ## Reading the container's little-endian values -/

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
  f.isFinite && f.abs < 1.0e7 && (f == 0.0 || f.abs >= 1.1754944e-38)

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
  deriving Inhabited, BEq

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
  for phase in [0:8] do
    if phase + 4 * recordBytes > payload.size then
      continue
    let records := (payload.size - phase) / recordBytes
    for slot in [0:4] do
      let mut points : Array (Float × Float × Float) := #[]
      let mut marker : Float := Float.ofBits 0x7f800001  -- a NaN, so it never matches
      let mut holds := true
      let mut plausibleRecords := 0
      let mut nonconstantRecords := 0
      for record in [0:records] do
        let quad ← recordAt payload phase record
        if record == 0 then
          marker := quad[slot]!
          if !(markers.any (fun m => (marker - m).abs < markerEpsilon)) then
            holds := false
        else if (quad[slot]! - marker).abs > markerEpsilon then
          holds := false
        let rest := others quad slot
        let (x, y, z) := (rest[0]!, rest[1]!, rest[2]!)
        points := points.push (x, y, z)
        if plausible x && plausible y && plausible z then
          plausibleRecords := plausibleRecords + 1
          if x != 0.0 || y != 0.0 || z != 0.0 then
            nonconstantRecords := nonconstantRecords + 1
      if !holds then
        continue
      -- 70% plausible, and at least two records carrying a real coordinate.
      if plausibleRecords * 10 < records * 7 || nonconstantRecords < 2 then
        continue
      match best with
      | some (bestRecords, _, _, _, _) =>
        if records > bestRecords then
          best := some (records, phase, slot, marker, points)
      | none => best := some (records, phase, slot, marker, points)
  match best with
  | some (_, phase, slot, marker, points) =>
    return { phase, constantSlot := slot, marker, points }
  | none =>
    throw "no f32 point stream fits this payload (`docs/DMA3D.md` §11.25)"

end MmfExtract.Dma3d.Modfile
