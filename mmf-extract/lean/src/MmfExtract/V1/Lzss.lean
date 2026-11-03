import MmfExtract.Bytes

/-!
The first game's LZSS decompressor.

A port of `src/v1/lzss.rs`. The routine is a bit-oriented LZSS with a 1024-byte
ring window: a control byte carries eight flags, least significant bit first,
and each flag says whether a literal byte follows or a two-byte match does.

```text
u8 control    one bit per token, least significant bit first, eight tokens per
              control byte. bit 1 = a literal, bit 0 = a match.
literal:      the next input byte, verbatim.
match:        the next two input bytes, big-endian:
                 distance = word & 0x3FF        (10 bits)
                 length   = (byte0 >> 2) + 3    (3..=66, from the other 6 bits)
```

**Note which bits hold which field**: `length = (byte0 >> 2) + 3` puts the
length in the word's *upper* six bits, so the encoded word is
`((length - 3) << 10) | distance`. Reading it as `((length - 3) << 2) | distance`
collides the length into the distance and silently invents matches.

A match's `distance` is a **window slot index**, not a count of bytes back. With
the write cursor starting at 958, `n` bytes back is `(958 + p - n) mod 1024` in
the encoded word. The decoder stops as soon as it has produced the declared
number of bytes; any remaining input is padding. Only under-run errors exist --
a too-long input is silently accepted.

Measured (`docs/GAME1.md` §4.3): `pack.PIC` 2,993 + `mpApp.PIC` 110 streams with
`size > len`, 125 `pack.PIC` streams with `size == len`, 9,733 stored-plain
resources, and all 12,961 pictures decode.
-/

namespace MmfExtract.V1.Lzss

/-- Window size of the version-1 LZSS, and the initial write cursor.

`0x42e104` loads `eax` with `0x3be` (958) and every later update masks it with
`0x3ff`; the original zeroes the first 958 slots at entry (`0x42e0f7`).
-/
def window : Nat := 1024

/-- The write cursor the game starts its ring at. -/
def windowStart : Nat := 958

/-- The decompressor's state, threaded explicitly because every step can fail. -/
structure Decompressor where
  src : ByteArray
  pos : Nat
  out : ByteArray
  window : Array UInt8
  wp : Nat
  control : UInt8
  bitsLeft : Nat
  deriving Inhabited

private def takeControl (d : Decompressor) : Except String Decompressor := do
  match d.src[d.pos]? with
  | some c => return { d with pos := d.pos + 1, control := c, bitsLeft := 8 }
  | none => throw s!"v1 lzss: control byte exhausted at input offset {d.pos}"

private def takeLiteral (d : Decompressor) : Except String (UInt8 × Decompressor) := do
  match d.src[d.pos]? with
  | some b => return (b, { d with pos := d.pos + 1 })
  | none => throw s!"v1 lzss: literal byte exhausted at input offset {d.pos}"

/-- Write one byte into the ring and advance the cursor. -/
private def emit (d : Decompressor) (b : UInt8) : Decompressor :=
  let wp' := (d.wp + 1) &&& 0x3FF
  { d with
    out := d.out.push b
    window := d.window.set! (d.wp &&& 0x3FF) b
    wp := wp' }

/-- Stop once the declared number of bytes has been produced: the token that
would overshoot is cut off and the window is not written past it. -/
private def fill (d : Decompressor) (uncompressed : Nat) : Decompressor :=
  if d.out.size >= uncompressed then d else emit d 0

private partial def loop (d : Decompressor) (uncompressed : Nat) : Except String Decompressor := do
  if d.out.size >= uncompressed then
    return d
  let d ← if d.bitsLeft == 0 then takeControl d else pure d
  let isLiteral := (d.control.toNat &&& 1) == 1
  let d := { d with control := UInt8.ofNat (d.control.toNat >>> 1), bitsLeft := d.bitsLeft - 1 }
  if isLiteral then
    let (b, d) ← takeLiteral d
    loop (emit d b) uncompressed
  else
    match d.src[d.pos]?, d.src[d.pos + 1]? with
    | some b0, some b1 =>
      let d := { d with pos := d.pos + 2 }
      let word := b0.toNat * 256 + b1.toNat
      let distance := word &&& 0x3FF
      let length := (b0.toNat >>> 2) + 3
      let mut d := d
      for i in [0:length] do
        if d.out.size >= uncompressed then
          break
        let b := d.window[(distance + i) &&& 0x3FF]!
        d := emit d b
      loop d uncompressed
    | _, _ => throw s!"v1 lzss: match pair exhausted at input offset {d.pos}"

/-- Decompress `src` to exactly `uncompressed` bytes.

The window starts all-zero and the write cursor at 958. Zeroing the whole
buffer is equivalent to the original's partial zeroing, because the game's
window lives at the fixed address `0x517540` and would otherwise carry stale
bytes between calls.
-/
def decompress (src : ByteArray) (uncompressed : Nat) : Except String ByteArray := do
  let start : Decompressor := {
    src, pos := 0, out := ByteArray.emptyWithCapacity uncompressed,
    window := Array.replicate Lzss.window 0, wp := windowStart, control := 0, bitsLeft := 0
  }
  return (← loop start uncompressed).out

end MmfExtract.V1.Lzss
