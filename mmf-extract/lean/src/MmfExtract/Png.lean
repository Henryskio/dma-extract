import MmfExtract.Bytes

/-!
Writing a decoded picture as PNG.

The Rust extractor hands this to the `png` crate, which emits a *compressed*
zlib stream. Lean has no zlib encoder, so this module builds the zlib stream
from **stored (uncompressed) deflate blocks** instead: `CMF=0x78`, `FLG=0x01`,
then as many final/non-final stored blocks of up to 65535 bytes as the picture
needs, then the Adler-32 of the raw scanlines. Every decoder accepts it, the
pixels are exact, and the file is larger by blocks' 5-byte headers only.

The CRC-32 and Adler-32 are the standard ones, over the chunk type and data
(`crc32`) and the scanline stream (`adler32`), exactly as the PNG spec says.
-/


namespace MmfExtract.Png

/-- The 8-byte PNG signature. -/
def signature : ByteArray := ByteArray.mk #[137, 80, 78, 71, 13, 10, 26, 10]

/-- The CRC-32 polynomial table, built the way PNG's spec describes.

`CRC32(data) = 0xFFFFFFFF XOR stream`, where the stream is the message bytes
followed by the running CRC, all reflected. The table is the usual reflected
`0xEDB88320` one.
-/
def crcTable : Array UInt32 := Id.run do
  let mut table := Array.mkEmpty 256
  for n in [0:256] do
    let mut c := UInt32.ofNat n
    for _ in [0:8] do
      -- `0xEDB88320` reflected: test bit 0, shift right, conditionally xor.
      c := if (c.toNat &&& 1) != 0 then (c >>> 1) ^^^ 0xEDB88320 else c >>> 1
    table := table.push c
  return table

/-- The CRC-32 of `data`, as PNG checks every chunk with. -/
def crc32 (data : ByteArray) : UInt32 := Id.run do
  let mut c : UInt32 := 0xFFFFFFFF
  for i in [0:data.size] do
    let byte := data[i]!
    let idx := (c.toNat ^^^ byte.toNat) &&& 0xFF
    c := (c >>> 8) ^^^ (crcTable.getD idx 0)
  return c ^^^ 0xFFFFFFFF

/-- The Adler-32 checksum the zlib wrapper ends with. -/
def adler32 (data : ByteArray) : UInt32 := Id.run do
  let mut a : UInt64 := 1
  let mut b : UInt64 := 0
  for i in [0:data.size] do
    a := (a + UInt64.ofNat data[i]!.toNat) % 65521
    b := (b + a) % 65521
  return UInt32.ofNat (b.toNat * 65536 + a.toNat)

/-- One big-endian u32 as four bytes. -/
def beU32 (v : UInt32) : ByteArray :=
  ByteArray.mk #[((v >>> 24).toNat &&& 0xFF).toUInt8, ((v >>> 16).toNat &&& 0xFF).toUInt8,
    ((v >>> 8).toNat &&& 0xFF).toUInt8, (v.toNat &&& 0xFF).toUInt8]

/-- A length-prefixed zlib stored block: the header byte, LEN, NLEN, then the
raw bytes. At most 65535 bytes fit in one. -/
private def storedBlock (final : Bool) (data : ByteArray) : ByteArray :=
  let header : UInt8 := if final then 1 else 0
  let len := data.size.toUInt16
  ByteArray.mk #[header, (len.toNat &&& 0xFF).toUInt8, ((len.toNat >>> 8) &&& 0xFF).toUInt8,
    ((~~~ len).toNat &&& 0xFF).toUInt8, (((~~~ len).toNat >>> 8) &&& 0xFF).toUInt8]
    |>.append data

/-- The zlib wrapper around the scanline stream: header, stored blocks, checksum. -/
def zlibStored (raw : ByteArray) : ByteArray := Id.run do
  let mut out := ByteArray.mk #[0x78, 0x01]
  -- The whole stream has to fit in blocks of at most 65535 bytes each, and the
  -- last one is the one that carries BFINAL.
  let blockSize := 65535
  let blocks := (raw.size + blockSize - 1) / max 1 blockSize
  for b in [0:blocks] do
    let start := b * blockSize
    let stop := min raw.size (start + blockSize)
    out := out.append (storedBlock (b == blocks - 1) (raw.extract start stop))
  return out.append (beU32 (adler32 raw))

/-- `data` with one filter byte of 0 before every row. -/
private def scanlines (width height : Nat) (rgba : ByteArray) : ByteArray := Id.run do
  let rowBytes := width * 4
  let mut out := ByteArray.emptyWithCapacity (height * (rowBytes + 1))
  for y in [0:height] do
    out := out.push 0
    for i in [0:rowBytes] do
      out := out.push (rgba[y * rowBytes + i]?.getD 0)
  return out

/-- One PNG chunk: length, type, data, CRC-32 over type and data. -/
private def chunk (kind : ByteArray) (data : ByteArray) : ByteArray :=
  beU32 data.size.toUInt32 |>.append kind |>.append data |>.append (beU32 (crc32 (kind.append data)))

/-- The complete PNG file for `rgba`, an 8-bit straight-RGBA image. -/
def ofRgba (width height : Nat) (rgba : ByteArray) : ByteArray :=
  let ihdr := (beU32 width.toUInt32).append (beU32 height.toUInt32)
    |>.append (ByteArray.mk #[8, 6, 0, 0, 0])  -- 8 bits, RGBA, deflate, no interlace
  signature
  |>.append (chunk "IHDR".toUTF8 ihdr)
  |>.append (chunk "IDAT".toUTF8 (zlibStored (scanlines width height rgba)))
  |>.append (chunk "IEND".toUTF8 ByteArray.empty)

end MmfExtract.Png
