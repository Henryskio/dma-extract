import MmfExtract.Bytes
import MmfExtract.Path
import MmfExtract.Types
import MmfExtract.V1.Metadata

/-!
MMFW resource container parsing: the first game's format and the one the
second and third games share.

This is a direct port of `src/container.rs` in the Rust extractor. Every byte
offset, every endianness choice and every failure message is the one that file
documents, so a container the Rust reader accepts is accepted here and one it
rejects is rejected with the same reason.

Layout (all multi-byte integers big-endian unless the file declares the "II"
little-endian marker):

```text
0x00  16 bytes  signature: "MMFW " + tag, NUL padded
0x10   2 bytes  endian marker: "MM" (big-endian) or "II" (little-endian)
0x12   2 bytes  u16 version
0x14   6 bytes  file id
0x1A   8 bytes  timestamp (double)
0x22   2 bytes  u16 resource count N        [0x1E in version 1]
0x24  4*(N+1)  u32 offsets                  [0x20 in version 1]
         2 bytes  padding
        32*N      resource names, NUL padded ASCII
...
```

Version 1 keeps this skeleton up to the names table and then has a metadata
region of its own, parsed by `MmfExtract.V1.Metadata`.
-/

namespace MmfExtract.Container

open MmfExtract (ContainerKind Dimension)

/-- The tag bytes at `0x05`..`0x10`, NUL trimmed, decide the kind.

The three shipped games spell the script tag three ways -- "Script",
"3 Script" and "Scripts" -- and the 3D game adds "Models" and "Textures".
-/
def kindOfTag (tag : String) : ContainerKind :=
  match tag with
  | "Pictures" => .pictures
  | "Sounds" => .sounds
  | "Films" => .films
  | "Blobs" => .blobs
  | "Scripts" | "3 Script" | "Script" => .scripts
  | "Models" => .models
  | "Textures" => .textures
  | _ => .unknown

/-- Metadata for a `MMFW Scripts` container: the serialized object database
that follows its offset table.

Each resource payload is one object; this metadata says what class each object
is and how many fixed-size elements its payload starts with.
-/
structure ScriptMeta where
  version : UInt16
  unknown1 : UInt16
  unknown2 : UInt16
  unknown3 : UInt16
  typeCount : UInt16
  /-- Per-class fixed record size in bytes. -/
  typeAux : Array UInt16
  /-- Class/type names in index order. -/
  types : Array String
  /-- Per-object element count. -/
  elementCounts : Array UInt32
  /-- Per-object index into `types`. -/
  objectTypes : Array UInt16
  /-- Per-object names, in resource order. -/
  objectNames : Array String
  metadataStart : Nat
  metadataEnd : Nat
  deriving Inhabited

/-- A parsed MMFW resource container. -/
structure Container where
  path : System.FilePath
  data : ByteArray
  kind : ContainerKind
  /-- True when the file declares big-endian ("MM") storage. -/
  bigEndian : Bool
  version : UInt16
  fileId : ByteArray
  timestamp : Float
  resourceCount : UInt16
  /-- What the header's own count field at `0x22` says, when it disagrees with
  the count the offset table holds.

  They differ only on the Italian 3D game's `DMA3D.MMX`, whose header declares
  705 while its table holds 782 entries (781 resources). The table is what the
  decode reads, so `resourceCount` is the walked count and this field keeps the
  declared one so the disagreement is reported rather than hidden. -/
  declaredCount : UInt16
  /-- `resourceCount + 1` file offsets; the last is EOF. -/
  offsets : Array UInt32
  names : Array String
  attr : ByteArray
  flags : ByteArray
  dimensions : Array Dimension
  /-- Per-resource decompressed payload size; zero means stored uncompressed. -/
  sizes : Array UInt32
  /-- Per-resource u32 array of undetermined purpose. -/
  extraU32 : Array UInt32
  /-- Per-resource "kind" byte: the palette source. -/
  kinds : ByteArray
  /-- Offset of the end of the names table. -/
  namesEnd : Nat
  /-- Version-1 picture containers only: the palette bank. -/
  palettes : Array ByteArray
  /-- Version-1 picture containers only: each resource's uncompressed size. -/
  uncompressedSizes : Array UInt32
  /-- Scripts-only object-database metadata; `none` for every other kind. -/
  scriptMeta : Option ScriptMeta
  /-- Why the metadata region could not be read, when it could not.

  The documented region length is `6*N + 2` for `Textures` and it is measured on
  the English build. The Italian `DMA3D.MMX` does not have it: walked to its
  table's real 782 entries, its names table ends at 28,158 and the region would
  have to reach 32,846, past the payload base at 29,654. So the names and the
  metadata do not both fit between the table and the payloads, and which of the
  two readings is right is not something the shipped bytes decide
  (`docs/DMA3D.md` §11). The texture decode does not use this region, so the
  container is read with it empty and the reason recorded here rather than thrown
  away. -/
  metadataReason : Option String
  deriving Inhabited

-------------------------------------------------------------------------------
-- Header reads
-------------------------------------------------------------------------------

private def byte (d : ByteArray) (i : Nat) : UInt8 := d[i]?.getD 0

/-- Whether `d` starts with the ASCII bytes of `lit`. -/
def hasPrefix (d : ByteArray) (lit : String) : Bool :=
  let want := lit.toUTF8
  want.size ≤ d.size && (List.range want.size).all fun i => byte d i == want[i]!

/-- The NUL-trimmed tag the container declares its kind with. -/
def tagField (d : ByteArray) : String := Id.run do
  let mut stop := 5
  while stop < 16 && d[stop]? != some 0 do
    stop := stop + 1
  return MmfExtract.Bytes.decodeLossy (d.extract 5 stop)

/-- The endian marker at `0x10`: "MM" is big-endian, "II" little-endian, and
anything else is an unmeasured writer. -/
def endianMarker (d : ByteArray) : Except String Bool := do
  let b0 ← MmfExtract.Bytes.byteAt d 0x10
  let b1 ← MmfExtract.Bytes.byteAt d 0x11
  if (b0.toNat, b1.toNat) == ('M'.toNat, 'M'.toNat) then return true
  else if (b0.toNat, b1.toNat) == ('I'.toNat, 'I'.toNat) then return false
  else throw s!"unknown endian marker [{b0}, {b1}]"

/-- The 8-byte timestamp, read in the container's byte order. -/
def timestampOf (d : ByteArray) (bigEndian : Bool) : Except String Float := do
  let positions := if bigEndian then [0, 1, 2, 3, 4, 5, 6, 7] else [7, 6, 5, 4, 3, 2, 1, 0]
  let mut bits : UInt64 := 0
  for p in positions do
    let byte ← MmfExtract.Bytes.byteAt d (0x1A + p)
    bits := UInt64.ofNat (bits.toNat * 256 + byte.toNat)
  return Float.ofBits bits

/-- Read `n` NUL-padded 32-byte name fields starting at `base`. -/
def namesTable (d : ByteArray) (base : Nat) (n : Nat) : Array String := Id.run do
  let mut names : Array String := #[]
  for i in [0:n] do
    let start := base + 32 * i
    let field := d.extract start (start + 32)
    let mut stop := 0
    while stop < field.size && field[stop]? != some 0 do
      stop := stop + 1
    names := names.push (MmfExtract.Bytes.decodeLossy (field.extract 0 stop))
  return names

/-- Read one length-prefixed string list: `u16 count`, `u32 blob length`,
`count x u32 offset`, then the blob. Offsets are relative to the blob's start
and strings are NUL terminated. -/
def readStringList (d : ByteArray) (startPos : Nat) (bigEndian : Bool) :
    Except String (Array String × Nat) := do
  let (count, p1) ← MmfExtract.Bytes.takeU16 d startPos bigEndian
  let (blobLen, p2) ← MmfExtract.Bytes.takeU32 d p1 bigEndian
  -- The offsets follow the two counts, and the blob follows the offsets, so
  -- both ends are computed rather than tracked through a loop.
  let blobStart := p2 + count.toNat * 4
  let blobEnd := blobStart + blobLen.toNat
  if blobEnd > d.size then
    throw s!"string list at 0x{MmfExtract.Bytes.hex blobStart} claims {blobLen} bytes but the file ends at 0x{MmfExtract.Bytes.hex d.size}"
  let mut strings : Array String := #[]
  for j in [0:count.toNat] do
    let off ← MmfExtract.Bytes.u32At d (p2 + 4 * j) bigEndian
    if off.toNat > blobLen.toNat then
      throw s!"string offset {off} is outside a {blobLen} byte blob"
    let rest := d.extract (blobStart + off.toNat) blobEnd
    let mut stop := 0
    while stop < rest.size && rest[stop]? != some 0 do
      stop := stop + 1
    strings := strings.push (MmfExtract.Bytes.decodeLossy (rest.extract 0 stop))
  return (strings, blobEnd)

/-- Parse the serialized object database that follows a Scripts container's
offset table. See `docs/SCRIPTS.md`. -/
def parseScriptMeta (path : System.FilePath) (d : ByteArray) (metadataStart payloadStart : Nat)
    (n : Nat) (bigEndian : Bool) : Except String ScriptMeta := do
  let loc := MmfExtract.Path.str path

  let (version, p1) ← MmfExtract.Bytes.takeU16 d metadataStart bigEndian
  let (u1, p2) ← MmfExtract.Bytes.takeU16 d p1 bigEndian
  let (u2, p3) ← MmfExtract.Bytes.takeU16 d p2 bigEndian
  let (u3, p4) ← MmfExtract.Bytes.takeU16 d p3 bigEndian
  let (typeCount, p5) ← MmfExtract.Bytes.takeU16 d p4 bigEndian
  let t := typeCount.toNat

  let typeAux := (Array.mk (List.range t |>.map (fun i =>
    (MmfExtract.Bytes.u16At d (p5 + 2 * i) bigEndian).toOption.getD 0)))

  let (types, posAfterTypes) ← readStringList d (p5 + 2 * t) bigEndian
  if types.size != t then
    throw s!"{loc}: script type count says {t}, type-name list has {types.size}"

  let afterCounts := posAfterTypes + 4 * n
  let elementCounts := (Array.mk (List.range n |>.map (fun i =>
    (MmfExtract.Bytes.u32At d (posAfterTypes + 4 * i) bigEndian).toOption.getD 0)))

  let objectTypes := (Array.mk (List.range n |>.map (fun i =>
    (MmfExtract.Bytes.u16At d (afterCounts + 2 * i) bigEndian).toOption.getD 0)))
  for v in objectTypes do
    if v.toNat >= t then
      throw s!"{loc}: object type index {v} is outside the {t} entry type table"

  let (objectNames, metadataEnd) ← readStringList d (afterCounts + 2 * n) bigEndian
  if objectNames.size != n then
    throw s!"{loc}: container has {n} resources but {objectNames.size} object names"
  if metadataEnd != payloadStart then
    throw s!"{loc}: script metadata ends at 0x{MmfExtract.Bytes.hex metadataEnd}, first payload starts at 0x{MmfExtract.Bytes.hex payloadStart}"

  return {
    version, unknown1 := u1, unknown2 := u2, unknown3 := u3, typeCount
    typeAux, types, elementCounts, objectTypes, objectNames
    metadataStart, metadataEnd
  }

-------------------------------------------------------------------------------
-- Parsing
-------------------------------------------------------------------------------

/-- The count the offset table really holds, walked out from the declared one.

A table entry describes a payload while the step from the previous entry is a
plausible payload size: at least four bytes (a resource cannot be empty) and at
most `2 MiB` (the largest payload any of these containers holds). The walk stops
at the first entry that is not one, and that entry is the container's own length
-- which is the check that makes the walk right rather than merely convenient.

This is a correction, not a convenience: `docs/DMA3D.md` §11 records that the
Italian `DMA3D.MMX`'s header declares 705 while its table holds 782 entries, and
that reading only 706 of them truncated the container to a third of its size and
lost most of its textures.
-/
def walkCount (data : ByteArray) (offBase : Nat) (bigEndian : Bool) (declared : Nat) : UInt16 := Id.run do
  -- A plain integer read: the walk's own bound keeps it inside the file, and a
  -- byte past the end reads as zero, which the step test rejects before the
  -- entry is used for anything.
  let entry := fun i =>
    match MmfExtract.Bytes.u32At data (offBase + 4 * i) bigEndian with
    | .ok v => v.toNat
    | .error _ => 0
  let mut n := declared
  let mut prev := entry n
  -- The table cannot run past the file, and it cannot describe a payload that
  -- starts before it ends.
  let limit := (data.size - offBase) / 4 - 1
  while n < limit do
    let next := entry (n + 1)
    let step := next - prev
    if step < 4 || step > 2 * 1024 * 1024 then
      break
    n := n + 1
    prev := next
  return UInt16.ofNat n

/-- Parse a container from an in-memory image. -/
def parse (path : System.FilePath) (data : ByteArray) : Except String Container := do
  let loc := MmfExtract.Path.str path
  if data.size < 0x24 then
    throw s!"{loc}: file too short ({data.size})"
  if !hasPrefix data "MMFW" then
    throw s!"{loc}: not an MMFW container"
  let kind := kindOfTag (tagField data)
  let bigEndian ← endianMarker data
  let version ← MmfExtract.Bytes.u16At data 0x12 bigEndian
  -- Version 3 = the third game, 2 = the second, 1 = the first. An unknown
  -- version means an unmeasured writer, so it is rejected rather than guessed.
  if version != 3 && version != 2 && version != 1 then
    throw s!"{loc}: unexpected version {version}"
  let fileId := data.extract 0x14 0x1A
  let timestamp ← timestampOf data bigEndian

  -- Version 1 stores the resource count four bytes earlier than the versions
  -- 2 and 3 table, which is what pins the base down.
  let declaredCount ←
    if version == 1 then MmfExtract.Bytes.u16At data 0x1E bigEndian
    else MmfExtract.Bytes.u16At data 0x22 bigEndian
  let offBase := if version == 1 then 0x20 else 0x24
  -- The count the offset table really holds, walked out from the declared one.
  -- A table entry is only describing a payload while the step from the previous
  -- entry is a plausible payload size, so the walk stops where the table stops
  -- describing payloads -- and that last entry is the container's own length.
  -- The Italian `DMA3D.MMX` is why this exists: its header declares 705, its
  -- table holds 782 entries, and reading only 706 of them truncated the file to
  -- a third of its size and lost 704 of its textures (`docs/DMA3D.md` §11).
  let resourceCount := walkCount data offBase bigEndian (declaredCount.toNat)
  let n := resourceCount.toNat
  let need := offBase + 4 * (n + 1)
  if data.size < need then
    throw s!"{loc}: truncated offset table (need {need}, have {data.size})"
  let mut offsets : Array UInt32 := #[]
  for i in [0:n + 1] do
    offsets := offsets.push (← MmfExtract.Bytes.u32At data (offBase + 4 * i) bigEndian)
  let payloadStart := (offsets.getD 0 0).toNat

  -- Scripts containers have no names table and no picture metadata.
  if kind == .scripts && version != 1 then
    let metadataStart := offBase + 4 * (n + 1)
    let script ← parseScriptMeta path data metadataStart payloadStart n bigEndian
    return {
      path, data, kind, bigEndian, version, fileId, timestamp, resourceCount, declaredCount, offsets,
      names := script.objectNames, attr := ByteArray.empty, flags := ByteArray.empty,
      dimensions := #[], sizes := Array.replicate n 0, extraU32 := #[], kinds := ByteArray.empty,
      namesEnd := metadataStart, palettes := #[], uncompressedSizes := #[],
      scriptMeta := some script, metadataReason := none
    }
  -- A version-1 Scripts container has no names table either: its payloads start
  -- two bytes after the offsets table and its names are read from the payloads.
  if kind == .scripts && version == 1 then
    return {
      path, data, kind, bigEndian, version, fileId, timestamp, resourceCount, declaredCount, offsets,
      names := Array.replicate n "", attr := ByteArray.empty, flags := ByteArray.empty,
      dimensions := #[], sizes := Array.replicate n 0, extraU32 := #[], kinds := ByteArray.empty,
      namesEnd := payloadStart, palettes := #[], uncompressedSizes := #[], scriptMeta := none
      metadataReason := none
    }

  -- 2 bytes of padding after the offset table, then the 32-byte name fields.
  let namesBase := offBase + 4 * (n + 1) + 2
  if namesBase + 32 * n > data.size then
    throw s!"{loc}: names table does not fit (need {namesBase + 32 * n}, have {data.size})"
  let names := namesTable data namesBase n
  let namesEnd := namesBase + 32 * n

  if version == 1 then
    -- A `4*N` u32 array every kind has, then the first game's own region.
    if namesEnd + 4 * n > data.size then
      throw s!"{loc}: version 1 metadata does not fit (need {namesEnd + 4 * n}, have {data.size})"
    let mut extraU32 : Array UInt32 := #[]
    for i in [0:n] do
      extraU32 := extraU32.push (← MmfExtract.Bytes.u32At data (namesEnd + 4 * i) bigEndian)
    let region ← (if kind == .scripts then pure MmfExtract.V1.Metadata.default
                else MmfExtract.V1.Metadata.parse data kind n namesEnd payloadStart bigEndian path)
    return {
      path, data, kind, bigEndian, version, fileId, timestamp, resourceCount, declaredCount, offsets, names,
      attr := region.attr, flags := region.flags, dimensions := region.dimensions,
      -- Version-1 containers have no decoded-size table: the uncompressed-size
      -- array is the equivalent field, and a zero there means "stored as-is".
      sizes := Array.replicate n 0, extraU32, kinds := region.kinds, namesEnd,
      palettes := region.palettes, uncompressedSizes := region.uncompressedSizes,
      scriptMeta := none
      metadataReason := none
    }

  -- The shared (versions 2 and 3) metadata region: `15*N + 4` bytes for
  -- pictures, `14*N + 2` for sounds, the 3D game's own lengths for its kinds.
  let metaLen :=
    match kind with
    | .sounds => 14 * n + 2
    | .models => 4 * n + 100
    | .textures => 6 * n + 2
    | _ => 15 * n + 4
  let metaEnd := namesEnd + metaLen
  -- The documented region length is measured on the English build. The Italian
  -- `DMA3D.MMX` does not have one at all: walked to the 782 entries its offset
  -- table really holds, its names table ends at 28,158 and the region would have
  -- to reach 32,846, past the payload base at 29,654. The payload region is what
  -- the decode reads, so a region that overruns it is left unread with the
  -- reason recorded, rather than made to fit by shortening the names table or
  -- discarding the payload base (`docs/DMA3D.md` §11).
  let metaDoesNotFit := metaEnd > payloadStart || metaEnd > data.size
  let metadataReason :=
    if metaDoesNotFit then
      some s!"the {metaLen}-byte metadata region does not fit between the names table (ends {namesEnd}) and the first payload ({payloadStart})"
    else none

  let mut extraU32 : Array UInt32 := #[]
  let mut attr : ByteArray := ByteArray.empty
  let mut flags : ByteArray := ByteArray.empty
  let mut dimensions : Array Dimension := #[]
  let mut sizes : Array UInt32 := #[]
  let mut kinds : ByteArray := ByteArray.empty
  unless metaDoesNotFit do
    for i in [0:n] do
      extraU32 := extraU32.push (← MmfExtract.Bytes.u32At data (namesEnd + 4 * i) bigEndian)

    -- The attribute byte array sits after the extra 2-byte word in a sound
    -- container instead of directly after the u32 array.
    let byteArrays := if kind == .sounds then namesEnd + 4 * n + 2 else namesEnd + 4 * n
    let attrBase := byteArrays
    for i in [0:n] do
      attr := attr.push (byte data (attrBase + i))
    let flagsBase := attrBase + n
    for i in [0:n] do
      flags := flags.push (byte data (flagsBase + i))

    -- 2 bytes of padding, then the dimension table (pictures only).
    let dimBase := if kind == .sounds then flagsBase + n else flagsBase + n + 2
    for i in [0:n] do
      -- The pair is {height, width}: the SECOND u16 is the width.
      let h ← MmfExtract.Bytes.u16At data (dimBase + 4 * i) bigEndian
      let w ← MmfExtract.Bytes.u16At data (dimBase + 4 * i + 2) bigEndian
      dimensions := dimensions.push { width := w, height := h }

    let sizeBase := dimBase + 4 * n
    for i in [0:n] do
      sizes := sizes.push (← MmfExtract.Bytes.u32At data (sizeBase + 4 * i) bigEndian)

    -- The kind byte array (object offset 0x2a8); sounds and films have none.
    if metaLen >= 15 * n + 4 then
      let kindBase := sizeBase + 4 * n
      for i in [0:n] do
        kinds := kinds.push (byte data (kindBase + i))


  return {
    path, data, kind, bigEndian, version, fileId, timestamp, resourceCount, declaredCount, offsets, names,
    attr, flags, dimensions, sizes, extraU32, kinds, namesEnd, palettes := #[],
    uncompressedSizes := #[], scriptMeta := none, metadataReason
  }

/-- Read and parse a container from disk.

A container that cannot even be read is reported, not aborted over: the Rust
driver skips a file `Container::open` fails on, and the Lean driver does too.
-/
def Container.read (path : System.FilePath) : IO (Except String Container) := do
  try
    let data ← IO.FS.readBinFile path
    return parse path data
  catch e =>
    return .error s!"reading {MmfExtract.Path.str path}: {e}"

-------------------------------------------------------------------------------
-- Accessors
-------------------------------------------------------------------------------

/-- Raw (still compressed) bytes of resource `i`. -/
def Container.payload (c : Container) (i : Nat) : Except String ByteArray :=
  match c.offsets[i]?, c.offsets[i + 1]? with
  | some s, some e => MmfExtract.Bytes.slice c.data s.toNat (e.toNat - s.toNat)
  | _, _ => throw s!"resource {i} has no offset pair"

/-- Compressed size of resource `i`. -/
def Container.payloadLen (c : Container) (i : Nat) : Except String Nat := do
  let a := (Option.map (fun x => x.toNat) (c.offsets[i]?)).getD 0
  let b := (Option.map (fun x => x.toNat) (c.offsets[i + 1]?)).getD 0
  return b - a

/-- Name of resource `i`. -/
def Container.name (c : Container) (i : Nat) : String := c.names.getD i ""

/-- Pixel dimensions of picture resource `i`. -/
def Container.dimension (c : Container) (i : Nat) : Dimension :=
  c.dimensions.getD i { width := 0, height := 0 }

/-- Flags byte of resource `i`: bit 0 selects 8 bits per pixel. -/
def Container.flagsOf (c : Container) (i : Nat) : UInt8 := c.flags[i]?.getD 0

/-- Attribute byte of resource `i`. -/
def Container.attrOf (c : Container) (i : Nat) : UInt8 := c.attr[i]?.getD 0

/-- Size of resource `i`'s decompressed payload as the container records it; a
zero means stored uncompressed, in which case the stored size is the answer. -/
def Container.plainLen (c : Container) (i : Nat) : Except String Nat := do
  let recorded := (c.sizes[i]?.getD 0).toNat
  if recorded == 0 then return ← Container.payloadLen c i else return recorded

/-- Kind byte of resource `i`. -/
def Container.kindOf (c : Container) (i : Nat) : UInt8 := c.kinds[i]?.getD 0

end MmfExtract.Container
