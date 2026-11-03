/-!
Byte-level helpers shared by every MMFW decoder.

The containers are big-endian on the wire unless the file declares the "II"
little-endian marker at `0x10`, so every multi-byte read goes through here
rather than through an inline shift.
-/

namespace MmfExtract.Bytes

/-- One byte of `d`, or an error naming the offset. -/
def byteAt (d : ByteArray) (off : Nat) : Except String UInt8 := do
  match d[off]? with
  | some b => return b
  | none => throw s!"byte offset {off} outside a {d.size} byte buffer"

/-- A big- or little-endian `u16` at `off` of `d`. -/
def u16At (d : ByteArray) (off : Nat) (bigEndian : Bool) : Except String UInt16 := do
  let b0 ← byteAt d off
  let b1 ← byteAt d (off + 1)
  let v ← if bigEndian then pure (UInt16.ofNat (b0.toNat * 256 + b1.toNat))
    else pure (UInt16.ofNat (b1.toNat * 256 + b0.toNat))
  return v

/-- A big- or little-endian `u32` at `off` of `d`. -/
def u32At (d : ByteArray) (off : Nat) (bigEndian : Bool) : Except String UInt32 := do
  let b0 ← byteAt d off
  let b1 ← byteAt d (off + 1)
  let b2 ← byteAt d (off + 2)
  let b3 ← byteAt d (off + 3)
  let digits := if bigEndian then [b0, b1, b2, b3] else [b3, b2, b1, b0]
  let mut acc : UInt32 := 0
  for b in digits do
    acc := UInt32.ofNat (acc.toNat * 256 + b.toNat)
  return acc

/-- A big- or little-endian `u16` at `pos`, plus the position after it. -/
def takeU16 (d : ByteArray) (pos : Nat) (bigEndian : Bool) : Except String (UInt16 × Nat) := do
  let v ← u16At d pos bigEndian
  return (v, pos + 2)

/-- A big- or little-endian `u32` at `pos`, plus the position after it. -/
def takeU32 (d : ByteArray) (pos : Nat) (bigEndian : Bool) : Except String (UInt32 × Nat) := do
  let v ← u32At d pos bigEndian
  return (v, pos + 4)

/-- The bytes of `d` from `start` up to `start + len`, or an error. -/
def slice (d : ByteArray) (start : Nat) (len : Nat) : Except String ByteArray := do
  if start + len > d.size then
    throw s!"range [{start}, {start + len}) outside a {d.size} byte buffer"
  return d.extract start (start + len)

/-- Decode `b` as UTF-8, replacing undecodable bytes the way the Rust port's
`String::from_utf8_lossy` does for every name and string it reads. -/
def decodeLossy (b : ByteArray) : String :=
  match String.fromUTF8? b with
  | some s => s
  | none => String.ofList (b.toList.map (fun byte => Char.ofNat byte.toNat))


/-- `n` in lowercase hexadecimal, without a prefix.

The Rust port's failure messages print offsets as `0x{:x}`; this is the same
digits, so the two tools report the same reason in the same words.
-/
def hex (n : Nat) : String :=
  let digits := "0123456789abcdef".toList
  if n == 0 then "0"
  else Id.run do
    let mut rest := n
    let mut out : List Char := []
    while rest != 0 do
      out := (digits.getD (rest % 16) '0') :: out
      rest := rest / 16
    return String.ofList out

/-- `n` in lowercase hexadecimal, zero-padded to `width` digits.

The Rust port formats offsets as `0x{:04x}`; this is those digits, so both
tools print the same offset the same way.
-/
def hexPad (width : Nat) (n : Nat) : String :=
  -- `hex` grows with the number, so pad only when it is shorter than `width`.
  let padded := hex n
  if padded.length >= width then padded
  else String.ofList (List.replicate (width - padded.length) '0') ++ padded

end MmfExtract.Bytes
