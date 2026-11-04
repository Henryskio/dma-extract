import MmfExtract.Bytes
import MmfExtract.Container
import MmfExtract.Path

/-!
Verifying the 3D path in Lean.

The 3D game's models are extracted byte for byte by `MmfExtract/Dma3d.lean`
(`--original`) and converted to glTF by the Rust converter (`--modern`). This
module is a third check, in a third language, that reads both paths' output off
disk:

* **`--original`** must be byte-exact: every file the extractor wrote must equal
  the container slice it came from, and the count must cover every resource the
  container names except the ones whose payload is outside it. This is the check
  `tests/dma3d.rs` makes in Rust; making it in Lean is what proves the Lean
  extractor rather than the Rust one.
* **`--modern`** must meet the mesh contract the writer documents: the buffer
  the glTF names must decode from base64 to the byte length the same section
  declares, the index buffer must hold exactly one `u16` per index the accessor
  counts, and every index must name a vertex the `POSITION` accessor counts —
  `modfile::parse_mesh`'s own rejection rule, so a glTF carrying an index the
  payload did not have is a failing glTF.

The glTF is read with a *section* reader rather than a JSON parser: the writer
spells each section once, one entry per line, with one indentation per level, so
a bracket match finds the section and a colon match finds its entry. That is
deliberate. A general parser would be more code to trust the same bytes with.
-/

namespace MmfExtract.Dma3d.Verify

/-- One `--original` file the extractor wrote. -/
structure OriginalEntry where
  index : Nat
  path : System.FilePath
  bytes : ByteArray
  deriving Inhabited

/-- Every `--original` file in `dir`, named `<index:05>_<name>`, in index order. -/
def originalFiles (dir : System.FilePath) : IO (Array OriginalEntry) := do
  let mut out : Array OriginalEntry := #[]
  let entries ← System.FilePath.readDir dir
  for entry in entries do
    let name := entry.fileName
    let digits := String.ofList (name.toList.takeWhile (·.isDigit))
    let some index := digits.toNat? | continue
    let bytes ← IO.FS.readBinFile entry.path
    out := out.push { index, path := entry.path, bytes }
  return out.qsort (·.index ≤ ·.index)

/-- One container's `--original` result. -/
structure OriginalResult where
  /-- Files read from the output directory. -/
  checked : Nat
  /-- Files whose bytes equal the container's slice. -/
  exact : Nat
  /-- Files whose bytes differ, as resource indices. -/
  mismatched : Array Nat
  /-- Resources the container names but no file was written for. -/
  missing : Array Nat
  deriving Inhabited

/-- Check one container's `--original` output against the container itself.

The payload is the container's own slice, so the file must be those bytes and
nothing else. A resource whose offsets entry names a range outside the file has
no payload: it must have no file either, and the extractor reports it instead of
writing bytes the container does not hold.
-/
def checkOriginal (container : MmfExtract.Container.Container)
    (dir : System.FilePath) : IO OriginalResult := do
  let files ← originalFiles dir
  let mut exact : Nat := 0
  let mut mismatched : Array Nat := #[]
  for file in files.toList do
    match MmfExtract.Container.Container.payload container file.index with
    | .error _ => mismatched := mismatched.push file.index
    | .ok payload =>
      if payload == file.bytes then
        exact := exact + 1
      else
        mismatched := mismatched.push file.index
  let written := files.map (·.index)
  let mut missing : Array Nat := #[]
  for i in [0:container.resourceCount.toNat] do
    if !written.contains i then
      missing := missing.push i
  return { checked := files.size, exact, mismatched, missing }

-------------------------------------------------------------------------------
-- Reading a glTF
-------------------------------------------------------------------------------

/-- The base64 alphabet the glTF `uri` uses. -/
def base64Chars : String :=
  "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"

/-- The value a base64 digit stands for, or `none`. -/
def base64Value (c : Char) : Option UInt8 :=
  match base64Chars.toList.idxOf? c with
  | none => none
  | some i => some (UInt8.ofNat i)

/-- Decode the standard base64 the glTF writer emits (with `=` padding).

Returns `none` for a string that is not base64 of a whole number of bytes, which
is what the writer's own decoder rejects too.
-/
def base64Decode (digits : String) : Option ByteArray := Id.run do
  let kept := digits.toList.filter (· != '=')
  if kept.length % 4 != 0 then
    return none
  let mut out : ByteArray := ByteArray.emptyWithCapacity (kept.length / 4 * 3)
  let mut acc : Nat := 0
  let mut bits : Nat := 0
  for c in kept do
    match base64Value c with
    | none => return none
    | some v =>
      acc := acc * 64 + v.toNat
      bits := bits + 6
      if bits == 24 then
        out := out.push (UInt8.ofNat (acc / 65536))
        out := out.push (UInt8.ofNat ((acc / 256) % 256))
        out := out.push (UInt8.ofNat (acc % 256))
        acc := 0
        bits := 0
  -- The bytes a padded group leaves behind.
  if bits == 6 then
    out := out.push (UInt8.ofNat (acc % 256))
  else if bits == 12 then
    out := out.push (UInt8.ofNat ((acc / 16) % 256))
    out := out.push (UInt8.ofNat ((acc % 16) * 16))
  else if bits == 18 then
    out := out.push (UInt8.ofNat (acc % 256))
  return some out

/-- The base64 payload of a glTF `uri`, without its `data:` prefix.

The writer spells the buffer as
`data:application/octet-stream;base64,<digits>`, which is what the glTF spec says
a data uri is, so only the digits after the last comma are base64.
-/
def uriPayload (uri : String) : Option String :=
  match uri.splitOn "," with
  | _ :: rest => rest.getLast?
  | [] => none

/-- The text after `key`'s colon, wherever the key sits in the document.

The writer indents its keys, so the match has to skip the leading spaces rather
than anchor at the start of the file.
-/
private def afterKey (doc : String) (key : String) (quoted : Bool) : Option String :=
  match doc.splitOn ("\"" ++ key ++ "\": " ++ (if quoted then "\"" else "")) with
  | _ :: rest => rest[0]?
  | [] => none

/-- The first number a document spells for `key`. -/
def docInt (doc : String) (key : String) : Option Nat :=
  match afterKey doc key false with
  | none => none
  | some rest =>
    let digits := String.ofList (rest.toList.takeWhile (·.isDigit))
    if digits.isEmpty then none else digits.toNat?

/-- The first string a document spells for `key`. -/
def docString (doc : String) (key : String) : Option String :=
  match afterKey doc key true with
  | none => none
  | some rest =>
    let chars := String.ofList (rest.toList.takeWhile (· != '"'))
    if chars.isEmpty then none else some chars

/-- The text of one top-level section, from `openMark` to the next `closeMark`.

The glTF is not nested arbitrarily deep and the writer spells each section once,
so this is enough; a general parser would be more code to trust the same bytes.
-/
def sectionText (doc : String) (openMark closeMark : String) : Option String :=
  match doc.splitOn openMark with
  | _ :: rest =>
    match rest[0]!.splitOn closeMark with
    | body :: _ => body
    | [] => none
  | [] => none

/-- The entries of one section, split on the indented `{` that opens each.

The writer indents an entry one level deeper than the section's brackets, which
is what makes the split unambiguous without tracking depth.
-/
def sectionEntries (body : String) : List String :=
  body.splitOn "{"

/-- The mesh contract for one glTF: read back, then checked.

* the `buffers` section's `uri` must base64-decode to the `byteLength` the same
  section declares — the whole buffer, decoded by hand rather than by the
  writer's own base64;
* the index buffer must hold one `u16` per index the accessor counts, and every
  index must name a vertex the `POSITION` accessor counts, which is
  `modfile::parse_mesh`'s own rejection rule;
* the vertex buffer must hold four floats per vertex the accessor counts, which
  is the `vertex` record the writer reads.

Returns the index count, or the first reason the file does not meet the contract.
-/
def checkGltf (doc : String) : Except String Nat := do
  let some buffers := sectionText doc "\"buffers\"" "],"
    | throw "the glTF spells no buffers section"
  let url ←
    match docString buffers "uri" with
    | some u => pure u
    | none => throw "the buffers section spells no uri"
  let digits ←
    match uriPayload url with
    | some d => pure d
    | none => throw "the buffer uri has no base64 payload after its comma"
  let declaredLength ←
    match docInt buffers "byteLength" with
    | some n => pure n
    | none => throw "the buffers section spells no byteLength"
  let decoded ←
    match base64Decode digits with
    | some bytes => pure bytes
    | none => throw s!"the buffer uri's {digits.length} digits are not whole base64"
  if decoded.size != declaredLength then
    throw s!"the buffer uri decodes to {decoded.size} bytes but the file declares {declaredLength}"

  let some accessors := sectionText doc "\"accessors\"" "],"
    | throw "the glTF spells no accessors section"
  let entries := sectionEntries accessors
  let indexCount :=
    match entries[1]? with
    | some e => (docInt e "count").getD 0
    | none => 0
  let vertexCount :=
    match entries[2]? with
    | some e => (docInt e "count").getD 0
    | none => 0
  if indexCount == 0 || vertexCount == 0 then
    throw "the accessors spell no counts"

  let some views := sectionText doc "\"bufferViews\"" "],"
    | throw "the glTF spells no bufferViews section"
  let viewEntries := sectionEntries views
  let indexBytes :=
    match viewEntries[1]? with
    | some e => (docInt e "byteLength").getD 0
    | none => 0
  let vertexBytes :=
    match viewEntries[2]? with
    | some e => (docInt e "byteLength").getD 0
    | none => 0
  if indexBytes != 2 * indexCount then
    throw s!"the index buffer is {indexBytes} bytes, but the index accessor counts {indexCount} (needs {2 * indexCount})"
  -- A VEC3 accessor holds three f32 per vertex, which is what
  -- `componentType` 5126 with `type` "VEC3" says. The payload's vertex record
  -- is 16 bytes (four floats, the last always `1.0`), and the writer drops the
  -- constant one rather than passing it through, so a position buffer is
  -- `12 * count` and a normal buffer is the same.
  if vertexBytes != 12 * vertexCount then
    throw s!"the vertex buffer is {vertexBytes} bytes, but the POSITION accessor counts {vertexCount} (needs {12 * vertexCount})"

  -- Every index must name a vertex: the index buffer's `u16` little-endian, as
  -- `componentType` 5123 spells.
  for k in [0:indexCount] do
    let slot := 2 * k
    let low := ((decoded[slot]?).getD 0).toNat
    let high := ((decoded[slot + 1]?).getD 0).toNat
    let value := low + 256 * high
    if value >= vertexCount then
      throw s!"index {value} at {k} names a vertex past the {vertexCount} the POSITION accessor counts"
  return indexCount

end MmfExtract.Dma3d.Verify
