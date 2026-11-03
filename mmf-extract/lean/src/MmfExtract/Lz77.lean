import MmfExtract.Bytes

/-!
The game's LZ77 decompressor.

A port of `src/lz77.rs`, which was itself transliterated from `FUN_004c557a` in
`DMA4.exe` (the engine's shared "uncompress bytes" routine). `eax` walks the
input, `ecx` walks the output, `esi` carries the current control/length value
and `edi` addresses the back-reference; here those roles are the fields of
`Decompressor`, threaded explicitly because every step can fail.

## Control byte dispatch (the match table at `0x4c562d`)

| control        | meaning                                                   |
|----------------|-----------------------------------------------------------|
| `0x00..0x0F`   | 2-byte match, distance `(O0 << 2) + (C >> 2) + 1`         |
| `0x10..0x1F`   | 3-byte match, distance `(O0 >> 2) + (O1 << 6) + 0x4000+`  |
| `0x20..0x3F`   | match, length `C & 0x1F` (0 = extended), 2-byte offset    |
| `0x40..0xFF`   | match, length `(C >> 5) + 1`, 1-byte offset                |

For the `0x10..0x1F` form the distance gains a further `0x4000` when bit 3 of
the control byte is set, giving a 32 KiB window instead of 16 KiB. A zero base
offset with bit 3 clear is the end-of-stream marker (`0x4c56bd`).

## Other forms

* `0x4c5593` -- a control byte above `0x11` means "copy `C - 0x11` literals
  first"; the following control byte then selects a match.
* `0x4c55ef` -- after a literal run, a control byte below `0x10` selects a
  3-byte match with distance `(O0 << 2) + (C >> 2) + 0x801`.
* `0x4c5620` -- after a match, the low two bits of the byte two back give the
  number of trailing literal bytes.

Extended lengths are a run of `0x00` bytes (each contributing `0xFF`) followed
by a final byte.
-/

namespace MmfExtract.Lz77

/-- The decompressor's state: the input, the output, and where each stands. -/
structure Decompressor where
  src : ByteArray
  pos : Nat
  out : ByteArray
  deriving Inhabited

/-- The control-flow states of the original routine. -/
inductive State
  | literalControl
  | afterLiteralRun
  | afterMatch
  deriving Inhabited

/-- Peek at the input without consuming it. -/
private def dpeek (d : Decompressor) : Except String UInt8 := do
  match d.src[d.pos]? with
  | some b => return b
  | none => throw s!"lz77: input exhausted at {d.pos}"

/-- Consume one input byte. -/
private def dbyte (d : Decompressor) : Except String (UInt8 × Decompressor) := do
  match d.src[d.pos]? with
  | some b => return (b, { d with pos := d.pos + 1 })
  | none => throw s!"lz77: input exhausted at {d.pos}"

/-- Copy `n` literal bytes from the input to the output. -/
private def literals (d : Decompressor) (n : Nat) : Except String Decompressor := do
  let stop := d.pos + n
  if stop > d.src.size then
    throw s!"lz77: literal run of {n} bytes at {d.pos} runs past end of input"
  let mut out := d.out
  for i in [d.pos:stop] do
    out := out.push d.src[i]!
  return { d with pos := stop, out }

/-- Read an extended length: `0x00` bytes each add `0xFF`, then a final byte,
added to `base`. -/
private def extendedLen (d : Decompressor) (base : UInt32) :
    Except String (UInt32 × Decompressor) := do
  let mut d' := d
  let mut acc : UInt32 := 0
  while (← dpeek d') == 0 do
    acc := UInt32.ofNat (acc.toNat + 0xFF)
    d' := { d' with pos := d'.pos + 1 }
  let (b, d'') ← dbyte d'
  return (UInt32.ofNat (acc.toNat + base.toNat + b.toNat), d'')

/-- Copy `length` bytes from `srcOff`, which may lie inside the region already
written, so overlapping matches are copied byte by byte. -/
private def copyMatch (d : Decompressor) (srcOff : Nat) (length : Nat) :
    Except String Decompressor := do
  if srcOff > d.out.size then
    throw s!"lz77: back-reference before start of output ({srcOff} > {d.out.size})"
  let mut out := d.out
  for i in [0:length] do
    out := out.push out[srcOff + i]!
  return { d with out }

/-- A back-reference distance is measured back from the output cursor. -/
private def back (d : Decompressor) (distance : Nat) : Except String Nat :=
  if distance > d.out.size then
    throw s!"lz77: back-reference underflow (distance {distance})"
  else
    pure (d.out.size - distance)

/-- The selector byte at `0x4c5616`: the byte two positions back in the input,
which the original reads as `[eax - 2]`. -/
private def selector (d : Decompressor) : UInt8 :=
  if d.pos < 2 then 0
  else (Option.map (fun b => (b.toNat &&& 3).toUInt8) (d.src[d.pos - 2]?)).getD 0

/-- The match length the control byte names directly, extended when it is
zero, as both offset forms do. -/
private def matchLen (d : Decompressor) (direct : Nat) (extendedBase : UInt32) :
    Except String (UInt32 × Decompressor) :=
  if direct == 0 then extendedLen d extendedBase else pure (UInt32.ofNat direct, d)

/-- The 2-byte offset form at `0x4c564e`: length `C & 0x1F`, then O0 and O1. -/
private def twoByteOffset (d : Decompressor) (c : UInt8) :
    Except String (Bool × Decompressor) := do
  let (length, d) ← matchLen d (c.toNat &&& 0x1F) 0x1F
  let (o0, d) ← dbyte d
  let (o1, d) ← dbyte d
  let distance := (o0.toNat >>> 2) + (o1.toNat <<< 6) + 1
  let srcOff ← back d distance
  return (true, ← copyMatch d srcOff (length.toNat + 2))

/-- The 3-byte offset form at `0x4c5689`: length `C & 7`, a 16 KiB or 32 KiB
window, and the end-of-stream marker on a zero base offset. -/
private def threeByteOffset (d : Decompressor) (c : UInt8) :
    Except String (Bool × Decompressor) := do
  let (length, d) ← matchLen d (c.toNat &&& 7) 0x07
  let (o0, d) ← dbyte d
  let (o1, d) ← dbyte d
  let base := (o0.toNat >>> 2) + (o1.toNat <<< 6)
  if base == 0 && (c.toNat &&& 8) == 0 then
    -- `0x4c56bd`: end-of-stream marker.
    return (false, d)
  -- `0x4c56c1` subtracts a further `0x4000`.
  let distance := base + 0x4000 + (if (c.toNat &&& 8) != 0 then 0x4000 else 0)
  let srcOff ← back d distance
  return (true, ← copyMatch d srcOff (length.toNat + 2))

/-- The match table at `0x4c562d`.

Returns `false` when the end-of-stream marker was reached.
-/
private def dispatchMatch (d : Decompressor) (c : UInt8) :
    Except String (Bool × Decompressor) := do
  if c.toNat >= 0x40 then
    -- `0x4c5632`: 1-byte offset.
    let (o, d) ← dbyte d
    let distance := ((c.toNat >>> 2) &&& 7) + (o.toNat <<< 3) + 1
    let length := (c.toNat >>> 5) + 1
    let srcOff ← back d distance
    return (true, ← copyMatch d srcOff length)
  else if c.toNat >= 0x20 then
    twoByteOffset d c
  else if c.toNat >= 0x10 then
    threeByteOffset d c
  else
    -- `0x4c56e6`: a 2-byte match with a short distance.
    let (o0, d) ← dbyte d
    let distance := (o0.toNat <<< 2) + (c.toNat >>> 2) + 1
    let srcOff ← back d distance
    return (true, ← copyMatch d srcOff 2)

/-- The literal run at `0x4c55b5`: three bytes are always copied, then `len`
more. -/
private def literalRun (d : Decompressor) (c : UInt8) : Except String Decompressor := do
  let (len, d) ← if c == 0 then extendedLen d 0x0F else pure (UInt32.ofNat c.toNat, d)
  literals d (3 + len.toNat)

/-- The main loop.

It is a `partial def` for the same reason the original is a `loop`: the stream
ends when the end-of-stream marker arrives, not when a structural bound runs
out, and no measure of progress is available to a totality checker.
-/
private partial def loop (d : Decompressor) (state : State) : Except String Decompressor := do
  match state with
  | .literalControl =>
    let (c, d) ← dbyte d
    if c.toNat >= 0x10 then
      let (cont, d) ← dispatchMatch d c
      if cont then loop d .afterMatch else pure d
    else
      loop (← literalRun d c) .afterLiteralRun
  | .afterLiteralRun =>
    let (c, d) ← dbyte d
    if c.toNat >= 0x10 then
      let (cont, d) ← dispatchMatch d c
      if cont then loop d .afterMatch else pure d
    else
      -- `0x4c55ef`: the special 3-byte match.
      let (o0, d) ← dbyte d
      let distance := (o0.toNat <<< 2) + (c.toNat >>> 2) + 0x801
      let srcOff ← back d distance
      loop (← copyMatch d srcOff 3) .afterMatch
  | .afterMatch =>
    -- `0x4c5616`
    let sel := selector d
    if sel == 0 then
      loop d .literalControl
    else
      let d ← literals d sel.toNat
      let (c, d) ← dbyte d
      let (cont, d) ← dispatchMatch d c
      if cont then loop d .afterMatch else pure d

/-- Run the decompressor to completion, from the entry sequence at `0x4c558c`. -/
def run (d : Decompressor) : Except String ByteArray := do
  -- The first byte is read before the loop, mirroring the entry sequence.
  let (first, d) ← dbyte d
  if first.toNat > 0x11 then
    -- `0x4c5593`: copy `C - 0x11` literals first.
    let count := first.toNat - 0x11
    let d ← literals d count
    -- `0x4c5620` falls through to `0x4c5629` when fewer than four literals
    -- were copied: read a control byte and dispatch straight to the match
    -- table, with no selector read.
    if count < 4 then
      let (c, d) ← dbyte d
      let (cont, d) ← dispatchMatch d c
      if !cont then return d.out
      return (← loop d .afterMatch).out
    else
      -- `0x4c55a1` then `0x4c55e6`.
      return (← loop d .afterLiteralRun).out
  else if first.toNat >= 0x10 then
    let (cont, d) ← dispatchMatch d first
    if !cont then return d.out
    return (← loop d .afterMatch).out
  else
    let d ← literalRun d first
    return (← loop d .afterLiteralRun).out

/-- Decompress an LZ77 stream. -/
def decompress (src : ByteArray) : Except String ByteArray :=
  run { src, pos := 0, out := ByteArray.emptyWithCapacity (src.size * 4) }

/-- Compress `src` into a stream that [`decompress`] reproduces exactly.

The Rust extractor keeps this so its round-trip tests exercise the decoder with
its own encoder; it emits the game's own literal-run forms and the
end-of-stream marker, and deliberately does not search for back-references.

* A first byte above `0x11` means "copy `C - 0x11` literals" and so can encode
  any run of 1 to 238 bytes on its own.
* A first byte of `0x00` selects the extended literal run, which can encode an
  arbitrarily long run.
* The stream is closed with the end-of-stream marker `0x11 0x00 0x00`.
-/
def compress (src : ByteArray) : ByteArray := Id.run do
  if src.size == 0 then
    -- The end-of-stream marker on its own decodes to nothing.
    return ByteArray.mk #[0x11, 0x00, 0x00]
  let mut out := ByteArray.emptyWithCapacity (src.size + src.size / 64 + 16)
  if src.size <= 238 then
    out := out.push (UInt8.ofNat (0x11 + src.size))
  else
    -- `0x4c55b5`: a literal run of `3 + len` bytes, with the length extended by
    -- a run of `0x00` bytes each worth `0xFF` plus a final byte. We need
    -- `3 + len == src.size()`, so `target == 0xFF * k + 0x0F + b`.
    let target := src.size - 3
    let t := target - 0x0F
    let k := (t - 1) / 0xFF
    let b := t - 0xFF * k
    out := out.push 0
    for _ in [0:k] do
      out := out.push 0
    out := out.push (UInt8.ofNat b)
  for i in [0:src.size] do
    out := out.push src[i]!
  for b in [0x11, 0x00, 0x00] do
    out := out.push b
  return out

end MmfExtract.Lz77
