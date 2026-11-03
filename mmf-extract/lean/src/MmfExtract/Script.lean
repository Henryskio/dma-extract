import MmfExtract.Bytes
import MmfExtract.Container
import MmfExtract.Path

/-!
`MMFW Scripts` resources (`.MMS`).

A port of `src/script.rs`. A Scripts container is not event bytecode: it is a
serialized **global object database** in which every resource is one named
object and the container metadata names that object's class. Each payload
starts with `element_count` fixed-size records (the record size comes from the
class table), optionally followed by a class-specific string tail.

Two output modes:

* `--original` writes each resource payload verbatim -- Scripts payloads are
  stored uncompressed, so this is the exact bytes the runtime reads.
* the default mode writes a structured, annotated corpus: `container.json`
  (metadata), `types.txt`, `strings.txt`, and one text file per object with
  every fixed record and every decoded string. Opaque objects fall back to an
  annotated hexdump.

See `docs/SCRIPTS.md` for the reverse-engineered layout and measurements.
-/

namespace MmfExtract.Script

open MmfExtract.Container (Container ScriptMeta)

/-- How a payload is split into records and a tail. -/
inductive Layout where
  /-- `count * record_size` bytes and nothing else. -/
  | records
  /-- `count * record_size` bytes, then the object name as a NUL string. -/
  | animation
  /-- `count * record_size` bytes, then one NUL-terminated text string. -/
  | creditsText
  /-- `count * record_size` bytes, then `count` NUL-terminated names. -/
  | brushNames
  /-- `count * 8` bytes of `(u16 ref, u16 name_offset, u32 flags)`, then a name
  blob. -/
  | animBrush
  /-- `count * 4` bytes of u32 string offsets, then a shared NUL-string blob. -/
  | textList
  /-- `count * 10` byte `(u16 id, u32 offset, u32 reserved)` entries plus a
  shared string blob. -/
  | stringTable
  /-- Structure not recognised; the payload is written as a hexdump. -/
  | opaque
  deriving Inhabited, DecidableEq

/-- The name the corpus uses for a layout. -/
def Layout.name : Layout → String
  | .records => "records"
  | .animation => "animation"
  | .creditsText => "credits-text"
  | .brushNames => "brush-names"
  | .animBrush => "anim-brush"
  | .textList => "text-list"
  | .stringTable => "string-table"
  | .opaque => "opaque"

-------------------------------------------------------------------------------
-- Layout detection
-------------------------------------------------------------------------------

/-- Number of fixed records and their size for object `index`. -/
def elementInfo (metaData : ScriptMeta) (index : Nat) : Nat × Nat :=
  let count := (metaData.elementCounts[index]?).getD 0 |>.toNat
  let typeIndex := (metaData.objectTypes[index]?).getD 0 |>.toNat
  let recordSize := (metaData.typeAux[typeIndex]?).getD 0 |>.toNat
  (count, recordSize)

/-- Type index, type name and record size for object `index`. -/
def typeInfo (metaData : ScriptMeta) (index : Nat) : Option (Nat × String × Nat) := do
  let typeIndex ← (metaData.objectTypes[index]?).map (·.toNat)
  let name ← metaData.types[typeIndex]?
  let recordSize ← (metaData.typeAux[typeIndex]?).map (·.toNat)
  return (typeIndex, name, recordSize)

/-- Parse exactly `expected` NUL-terminated strings that consume all of `data`. -/
def nulStringsExact (data : ByteArray) (expected : Nat) : Option (Array ByteArray) :=
  Id.run do
    let mut pos : Nat := 0
    let mut strings : Array ByteArray := #[]
    for _ in [0:expected] do
      let rest := data.extract pos data.size
      let stop := (rest.findIdx? (· == 0)).getD rest.size
      strings := strings.push (rest.extract 0 stop)
      pos := pos + stop + 1
    return if pos == data.size then some strings else none

/-- Read a string at `offset` inside a shared NUL-string blob. -/
def stringAtOffset (blob : ByteArray) (offset : Nat) : Option ByteArray :=
  if offset > blob.size then
    none
  else
    let rest := blob.extract offset blob.size
    let stop := (rest.findIdx? (· == 0)).getD rest.size
    some (rest.extract 0 stop)

/-- One `(id, offset, reserved)` StringTable record plus the shared blob. -/
def stringTableEntries (payload : ByteArray) (count : Nat) (bigEndian : Bool) :
    Option (Array (UInt16 × UInt32 × UInt32) × ByteArray) :=
  Id.run do
    -- The Rust port's `checked_mul` has no Lean counterpart worth a name: a
    -- `Nat` product is exact, and the bound check below is the real gate.
    let fixed := count * 10
    if count == 0 || fixed > payload.size then
      return none
    let mut entries : Array (UInt16 × UInt32 × UInt32) := #[]
    for j in [0:count] do
      let record := payload.extract (j * 10) (j * 10 + 10)
      entries := entries.push (
        (Bytes.u16At record 0 bigEndian).toOption.getD 0,
        (Bytes.u32At record 2 bigEndian).toOption.getD 0,
        (Bytes.u32At record 6 bigEndian).toOption.getD 0)
    return some (entries, payload.extract fixed payload.size)

/-- AnimBrush payload: `count` entries of `(u16 ref, u16 name_offset, u32
flags)` followed by a shared string blob. Every name offset must resolve inside
the blob. -/
def animBrushEntries (payload : ByteArray) (count : Nat) (bigEndian : Bool) :
    Option (Array (UInt16 × UInt16 × UInt32) × ByteArray) :=
  Id.run do
    let fixed := count * 8
    if count == 0 || fixed > payload.size then
      return none
    let blob := payload.extract fixed payload.size
    let mut entries : Array (UInt16 × UInt16 × UInt32) := #[]
    for j in [0:count] do
      let record := payload.extract (j * 8) (j * 8 + 8)
      let nameOffset := (Bytes.u16At record 2 bigEndian).toOption.getD 0
      if (stringAtOffset blob nameOffset.toNat).isNone then
        return none
      entries := entries.push (
        (Bytes.u16At record 0 bigEndian).toOption.getD 0,
        nameOffset,
        (Bytes.u32At record 4 bigEndian).toOption.getD 0)
    return some (entries, blob)

/-- TextList payload: `count` u32 offsets relative to a shared NUL-string blob
that follows the offset array. Every offset must resolve inside the blob. -/
def textListEntries (payload : ByteArray) (count : Nat) (bigEndian : Bool) :
    Option (Array UInt32 × ByteArray) :=
  Id.run do
    let fixed := count * 4
    if count == 0 || fixed > payload.size then
      return none
    let blob := payload.extract fixed payload.size
    let mut offsets : Array UInt32 := #[]
    for j in [0:count] do
      let offset := (Bytes.u32At payload (j * 4) bigEndian).toOption.getD 0
      if (stringAtOffset blob offset.toNat).isNone then
        return none
      offsets := offsets.push offset
    return some (offsets, blob)

/-- Decide how an object's payload is laid out. Anything unexpected becomes
`.opaque` so extraction can still continue. -/
def classify (container : Container) (metaData : ScriptMeta) (index : Nat) : Layout :=
  match typeInfo metaData index with
  | none => Layout.opaque
  | some (_, typeName, recordSize) =>
    let (count, _) := elementInfo metaData index
    let payload := (container.payload index).toOption.getD ByteArray.empty
    if recordSize == 0 then
      if payload.size == 0 then Layout.records else Layout.opaque
    else
      let fixed := count * recordSize
      if fixed > payload.size then
        Layout.opaque
      else
        let tail := payload.extract fixed payload.size
        match typeName with
        | "Animation" =>
          if (nulStringsExact tail 1).isSome then Layout.animation else Layout.opaque
        | "creditsTextEntry" =>
          if (nulStringsExact tail 1).isSome then Layout.creditsText else Layout.opaque
        | "StdBrush" =>
          if (nulStringsExact tail count).isSome then Layout.brushNames else Layout.opaque
        | "AnimBrush" =>
          if recordSize == 8
              && (animBrushEntries payload count container.bigEndian).isSome
              && (nulStringsExact tail count).isSome then
            Layout.animBrush
          else
            Layout.opaque
        | "TextList" =>
          if (textListEntries payload count container.bigEndian).isSome then Layout.textList
          else Layout.opaque
        | "StringTable" =>
          if tail.size != 0 && count > 0 then Layout.stringTable else Layout.opaque
        | _ => if fixed == payload.size then Layout.records else Layout.opaque

-------------------------------------------------------------------------------
-- Formatting helpers
-------------------------------------------------------------------------------

/-- Right-align `s` in `width` columns. -/
def padLeft (width : Nat) (s : String) : String :=
  if s.length >= width then s
  else String.ofList (List.replicate (width - s.length) ' ') ++ s

/-- Zero-pad `n` to `width` digits. -/
def pad0 (width : Nat) (n : Nat) : String :=
  let digits := toString n
  String.ofList (List.replicate (width - min width digits.length) '0') ++ digits

/-- A double quote, for the `s!` interpolation strings: Lean's interpolation
syntax does not accept `\"` inside an interpolated string, so the quote is a
term the interpolation can carry. -/
private def q : String := "\""

/-- Add a line, with the newline the `writeln!` of the Rust port adds. -/
private def addLine (text : String) (line : String) : String := text ++ line ++ "\n"

/-- Hex bytes joined by a space, as the object dumps print them. -/
def hexBytes (data : ByteArray) : String :=
  String.intercalate " " ((List.range data.size).map (fun i =>
    MmfExtract.Bytes.hexPad 2 data[i]!.toNat))

/-- Escape arbitrary bytes for a human-readable one-line string. -/
def escapeBytes (data : ByteArray) : String := Id.run do
  let mut out := ""
  for i in [0:data.size] do
    let b := data[i]!
    let byte := b.toNat
    if byte == 0x5c then
      out := out ++ "\\\\"
    else if byte == 0x22 then
      out := out ++ "\\\""
    else if byte == 0 then
      out := out ++ "\\0"
    else if byte == 0x0a then
      out := out ++ "\\n"
    else if byte == 0x0d then
      out := out ++ "\\r"
    else if byte == 0x09 then
      out := out ++ "\\t"
    else if byte >= 0x20 && byte <= 0x7e then
      out := out.push (Char.ofNat byte)
    else
      -- `\xNN` is hexadecimal, as Rust's `{b:02x}` writes it.
      out := out ++ "\\x" ++ MmfExtract.Bytes.hexPad 2 byte
  return out

/-- Escape a string for JSON output. -/
def jsonEscape (s : String) : String := Id.run do
  let mut out := ""
  for c in s.toList do
    if c == '"' then
      out := out ++ "\\\""
    else if c == '\\' then
      out := out ++ "\\\\"
    else if c == '\n' then
      out := out ++ "\\n"
    else if c == '\r' then
      out := out ++ "\\r"
    else if c == '\t' then
      out := out ++ "\\t"
    else if c.toNat < 0x20 then
      out := out ++ "\\u" ++ MmfExtract.Bytes.hexPad 4 c.toNat
    else
      out := out.push c
  return out

/-- An annotated hexdump: 16 bytes per line, ASCII gutter, `base` as the
address of the first byte. -/
def hexdump (data : ByteArray) (base : Nat) : String := Id.run do
  if data.size == 0 then
    return ""
  let mut out := ""
  for line in [0:(data.size + 15) / 16] do
    let start := line * 16
    let stop := min data.size (start + 16)
    -- The address and every byte are hexadecimal, as Rust's `{:08x}` and
    -- `{:02x}` write them.
    out := out ++ "  " ++ MmfExtract.Bytes.hexPad 8 (base + start) ++ "  "
    for i in [0:16] do
      if start + i < stop then
        out := out ++ MmfExtract.Bytes.hexPad 2 data[start + i]!.toNat ++ " "
      else
        out := out ++ "   "
    out := out ++ " |"
    for i in [start:stop] do
      let byte := data[i]!.toNat
      out := out.push (if byte >= 0x20 && byte < 0x7f then Char.ofNat byte else '.')
    for _ in [stop - start:16] do
      out := out.push ' '
    out := out ++ "|\n"
  return out

-------------------------------------------------------------------------------
-- container.json
-------------------------------------------------------------------------------

/-- The container's metadata as JSON, with the coverage numbers the corpus
measures.

`layoutCounts` is sorted by layout name, because a `BTreeMap` is what the Rust
port iterates.
-/
def containerJson (container : Container) (metaData : ScriptMeta) (layouts : Array Layout)
    (typeCounts : Array Nat) (layoutCounts : List (String × Nat))
    (payloadBytes recordBytes tailBytes opaqueBytes : Nat) : String := Id.run do
  let n := container.resourceCount.toNat
  let mut out := ""
  out := addLine out "{"
  out := addLine out "  \"format\": \"MMFW Scripts\","
  out := addLine out s!"  {q}source{q}: {q}{jsonEscape (MmfExtract.Path.str container.path)}{q},"
  out := addLine out (if container.bigEndian then "  \"endian\": \"big\","
    else "  \"endian\": \"little\",")
  out := addLine out s!"  {q}container_version{q}: {container.version},"
  out := addLine out s!"  {q}resource_count{q}: {container.resourceCount},"
  out := addLine out "  \"script_metadata\": {"
  out := addLine out s!"    {q}version{q}: {metaData.version},"
  out := addLine out s!"    {q}unknown_fields{q}: [{metaData.unknown1}, {metaData.unknown2}, {metaData.unknown3}],"
  out := addLine out s!"    {q}type_count{q}: {metaData.typeCount},"
  out := addLine out s!"    {q}offset{q}: {metaData.metadataStart},"
  out := addLine out s!"    {q}end{q}: {metaData.metadataEnd},"
  out := addLine out s!"    {q}bytes{q}: {metaData.metadataEnd - metaData.metadataStart}"
  out := addLine out "  },"
  out := addLine out "  \"coverage\": {"
  out := addLine out s!"    {q}payload_bytes{q}: {payloadBytes},"
  out := addLine out s!"    {q}record_bytes{q}: {recordBytes},"
  out := addLine out s!"    {q}tail_bytes{q}: {tailBytes},"
  out := addLine out s!"    {q}opaque_bytes{q}: {opaqueBytes},"
  out := addLine out "    \"layouts\": {"
  -- A comma follows every line but the last, which is how the Rust port's
  -- `enumerate`-driven comma decision reads.
  for ((name, count), position) in layoutCounts.zipIdx do
    let comma := if position + 1 == layoutCounts.length then "" else ","
    out := addLine out s!"      {q}{name}{q}: {count}{comma}"
  out := addLine out "    }"
  out := addLine out "  },"
  out := addLine out ("  " ++ q ++ "types" ++ q ++ ": [")
  for (name, index) in metaData.types.toList.zipIdx do
    let comma := if index + 1 == metaData.types.size then "" else ","
    let recordSize := (metaData.typeAux[index]?).getD 0 |>.toNat
    let objects := (typeCounts[index]?).getD 0
    out := addLine out ("    {" ++ q ++ "index" ++ q ++ ": " ++ toString index
      ++ ", " ++ q ++ "name" ++ q ++ ": " ++ q ++ jsonEscape name ++ q
      ++ ", " ++ q ++ "record_size" ++ q ++ ": " ++ toString recordSize
      ++ ", " ++ q ++ "object_count" ++ q ++ ": " ++ toString objects ++ "}" ++ comma)
  out := addLine out "  ],"
  out := addLine out ("  " ++ q ++ "objects" ++ q ++ ": [")
  for index in [0:n] do
    let (typeIndex, typeName, recordSize) := (typeInfo metaData index).getD (0, "", 0)
    let (count, _) := elementInfo metaData index
    let safe := Path.sanitize (container.name index) index
    let comma := if index + 1 == n then "" else ","
    let layout := (layouts[index]?).getD Layout.opaque
    let offset := (container.offsets[index]?).getD 0
    let payloadLen := (container.payloadLen index).toOption.getD 0
    out := addLine out ("    {" ++ q ++ "index" ++ q ++ ": " ++ toString index
      ++ ", " ++ q ++ "name" ++ q ++ ": " ++ q ++ jsonEscape (container.name index) ++ q
      ++ ", " ++ q ++ "type_index" ++ q ++ ": " ++ toString typeIndex
      ++ ", " ++ q ++ "type" ++ q ++ ": " ++ q ++ jsonEscape typeName ++ q
      ++ ", " ++ q ++ "record_size" ++ q ++ ": " ++ toString recordSize
      ++ ", " ++ q ++ "elements" ++ q ++ ": " ++ toString count
      ++ ", " ++ q ++ "payload_offset" ++ q ++ ": " ++ toString offset
      ++ ", " ++ q ++ "payload_size" ++ q ++ ": " ++ toString payloadLen
      ++ ", " ++ q ++ "layout" ++ q ++ ": " ++ q ++ layout.name ++ q
      ++ ", " ++ q ++ "file" ++ q ++ ": " ++ q ++ "objects/" ++ pad0 5 index ++ "_" ++ safe ++ ".txt" ++ q
      ++ "}" ++ comma)
  out := addLine out "  ]"
  out := addLine out "}"
  return out

-------------------------------------------------------------------------------
-- types.txt / strings.txt
-------------------------------------------------------------------------------

/-- The type table as a column-aligned text file. -/
def typesTxt (container : Container) (metaData : ScriptMeta) (typeCounts : Array Nat) : String :=
  Id.run do
    let mut out := ""
    out := addLine out s!"# Scripts type table for {MmfExtract.Path.str container.path}"
    out := addLine out "# idx  record_size  objects  name"
    for (name, index) in metaData.types.toList.zipIdx do
      let recordSize := (metaData.typeAux[index]?).getD 0 |>.toNat
      let objects := (typeCounts[index]?).getD 0
      out := addLine out s!"{padLeft 3 (toString index)}  {padLeft 11 (toString recordSize)}  {padLeft 7 (toString objects)}  {name}"
    return out

/-- Every string the corpus decodes, in one file. -/
def stringsTxt (container : Container) (metaData : ScriptMeta) (layouts : Array Layout) : String :=
  Id.run do
    let n := container.resourceCount.toNat
    let mut out := ""
    out := addLine out s!"# Strings decoded from {MmfExtract.Path.str container.path}"
    out := addLine out "# Escapes: \\\" \\\\ \\0 \\n \\r \\t, other bytes as \\xNN"
    out := addLine out ""
    out := addLine out "[type names]"
    for (name, index) in metaData.types.toList.zipIdx do
      out := addLine out s!"type[{padLeft 3 (toString index)}] {q}{escapeBytes name.toUTF8}{q}"
    out := addLine out ""
    out := addLine out "[object names]"
    for index in [0:n] do
      out := addLine out s!"object[{pad0 4 index}] {q}{escapeBytes (container.name index).toUTF8}{q}"
    out := addLine out ""
    out := addLine out "[payload strings]"
    let mut total : Nat := 0
    for index in [0:n] do
      let payload := (container.payload index).toOption.getD ByteArray.empty
      let name := escapeBytes (container.name index).toUTF8
      let (count, _) := elementInfo metaData index
      match (layouts[index]?).getD Layout.opaque with
      | Layout.stringTable =>
        match stringTableEntries payload count container.bigEndian with
        | some (entries, blob) =>
          for (id, offset, _) in entries.toList do
            -- The sentinels 0xfffd, 0xfffe and 0xffff mean "absent".
            if offset.toNat < 0xfffd then
              match stringAtOffset blob offset.toNat with
              | some s =>
                out := addLine out s!"StringTable[{pad0 4 index}] {q}{name}{q} id={id} offset=0x{MmfExtract.Bytes.hexPad 4 offset.toNat} {q}{escapeBytes s}{q}"
                total := total + 1
              | none => pure ()
        | none => pure ()
      | Layout.creditsText =>
        let (count, recordSize) := elementInfo metaData index
        let fixed := count * recordSize
        match nulStringsExact (payload.extract fixed payload.size) 1 with
        | some strings =>
          out := addLine out s!"creditsTextEntry[{pad0 4 index}] {q}{name}{q} {q}{escapeBytes (strings.getD 0 ByteArray.empty)}{q}"
          total := total + 1
        | none => pure ()
      | Layout.textList =>
        match textListEntries payload count container.bigEndian with
        | some (offsets, blob) =>
          for (offset, k) in offsets.toList.zipIdx do
            match stringAtOffset blob offset.toNat with
            | some s =>
              out := addLine out s!"TextList[{pad0 4 index}] {q}{name}{q} [{k}] offset=0x{MmfExtract.Bytes.hexPad 4 offset.toNat} {q}{escapeBytes s}{q}"
              total := total + 1
            | none => pure ()
        | none => pure ()
      | Layout.brushNames =>
        let (count, recordSize) := elementInfo metaData index
        let fixed := count * recordSize
        match nulStringsExact (payload.extract fixed payload.size) count with
        | some strings =>
          for (s, k) in strings.toList.zipIdx do
            out := addLine out s!"StdBrush[{pad0 4 index}] {q}{name}{q} [{k}] {q}{escapeBytes s}{q}"
            total := total + 1
        | none => pure ()
      | Layout.animBrush =>
        match animBrushEntries payload count container.bigEndian with
        | some (entries, blob) =>
          for ((_, offset, _), k) in entries.toList.zipIdx do
            match stringAtOffset blob offset.toNat with
            | some s =>
              out := addLine out s!"AnimBrush[{pad0 4 index}] {q}{name}{q} [{k}] offset=0x{MmfExtract.Bytes.hexPad 4 offset.toNat} {q}{escapeBytes s}{q}"
              total := total + 1
            | none => pure ()
        | none => pure ()
      | _ => pure ()
    out := addLine out ""
    out := addLine out s!"# {total} payload strings decoded"
    return out

-------------------------------------------------------------------------------
-- Per-object dump
-------------------------------------------------------------------------------

/-- A signed reading of an unsigned 16- or 32-bit field. -/
private def signed16 (v : UInt16) : Int :=
  Int.ofNat v.toNat - (if v.toNat >= 0x8000 then 65536 else 0)

private def signed32 (v : UInt32) : Int :=
  Int.ofNat v.toNat - (if v.toNat >= 0x80000000 then 4294967296 else 0)

/-- Decode one fixed record's fields, by record size. -/
def recordFields (record : ByteArray) (typeName : String) (bigEndian : Bool) : String :=
  Id.run do
    let u16At off := (Bytes.u16At record off bigEndian).toOption.getD 0
    let u32At off := (Bytes.u32At record off bigEndian).toOption.getD 0
    match record.size with
    | 2 =>
      let a := u16At 0
      s!" | u16 {a} (i16 {signed16 a})"
    | 4 =>
      let a := u32At 0
      s!" | u32 {a} (i32 {signed32 a})"
    | 8 =>
      s!" | u16 {u16At 0} {u16At 2} {u16At 4} {u16At 6} | u32 {u32At 0} {u32At 4}"
    | 10 =>
      if typeName == "Animation" then
        s!" | op {u16At 0} args {u16At 2} {u16At 4} {u16At 6} {u16At 8}"
      else
        s!" | u16 {u16At 0} {u16At 2} {u16At 4} {u16At 6} {u16At 8}"
    | 12 =>
      s!" | u16 {u16At 0} {u16At 2} {u16At 4} {u16At 6} {u16At 8} {u16At 10} | u32 {u32At 0} {u32At 4} {u32At 8}"
    | len =>
      if len % 2 == 0 then
        Id.run do
          let mut out := " | u16"
          for k in [0:len / 2] do
            out := out ++ s!" {u16At (2 * k)}"
          return out
      else
        ""

/-- The fixed records of a dump: the header lines and one line per record. -/
private def recordsBlock (out : String) (payload : ByteArray) (count recordSize : Nat)
    (typeName : String) (bigEndian : Bool) : String := Id.run do
  let mut text := addLine out s!"## Fixed records ({count} x {recordSize} bytes)"
  text := addLine text "# [index] raw bytes | decoded fields"
  for j in [0:count] do
    let record := payload.extract (j * recordSize) (j * recordSize + recordSize)
    text := text ++ s!"  [{padLeft 4 (toString j)}] {hexBytes record}"
    text := text ++ recordFields record typeName bigEndian
    text := text ++ "\n"
  return text

/-- A NUL-terminated string tail, and any raw bytes after it. -/
def nulTail (out : String) (tail : ByteArray) (expected : Nat) (label : String) (base : Nat) :
    String := Id.run do
  let mut text := addLine out s!"## Tail: {label} ({expected} NUL-terminated string(s), {tail.size} bytes)"
  let mut pos : Nat := 0
  for k in [0:expected] do
    let rest := tail.extract pos tail.size
    let stop := (rest.findIdx? (· == 0)).getD rest.size
    text := addLine text s!"  [{k}] +0x{MmfExtract.Bytes.hexPad 4 pos} len {padLeft 3 (toString stop)} {q}{escapeBytes (rest.extract 0 stop)}{q}"
    pos := pos + stop + 1
  if pos < tail.size then
    text := text ++ s!"  trailing {tail.size - pos} raw bytes:\n"
    text := text ++ hexdump (tail.extract pos tail.size) (base + pos)
  return text

/-- One object's annotated dump. -/
def objectDump (container : Container) (metaData : ScriptMeta) (index : Nat) (layout : Layout) :
    String := Id.run do
  let payload := (container.payload index).toOption.getD ByteArray.empty
  let (typeIndex, typeName, recordSize) := (typeInfo metaData index).getD (0, "?", 0)
  let (count, _) := elementInfo metaData index
  let fixed := count * recordSize
  let offset := (container.offsets[index]?).getD 0
  let mut out := ""
  out := addLine out s!"# Object {index}: {container.name index}"
  out := addLine out s!"# Type: {typeName} (index {typeIndex})"
  out := addLine out s!"# Elements: {count} x {recordSize} bytes = {fixed}"
  out := addLine out s!"# Payload: {payload.size} bytes at container offset 0x{MmfExtract.Bytes.hex offset.toNat}"
  out := addLine out s!"# Layout: {layout.name}"

  if layout == Layout.opaque then
    out := addLine out "# No structural decode; full verbatim hexdump follows."
    return out ++ hexdump payload 0

  match layout with
  | Layout.records =>
    out := recordsBlock out payload count recordSize typeName container.bigEndian
  | Layout.animation =>
    out := recordsBlock out payload count recordSize typeName container.bigEndian
    out := nulTail out (payload.extract fixed payload.size) 1 "object name" fixed
  | Layout.creditsText =>
    out := recordsBlock out payload count recordSize typeName container.bigEndian
    out := nulTail out (payload.extract fixed payload.size) 1 "credits text" fixed
  | Layout.brushNames =>
    out := recordsBlock out payload count recordSize typeName container.bigEndian
    out := nulTail out (payload.extract fixed payload.size) count "brush names" fixed
  | Layout.animBrush =>
    out := addLine out s!"## AnimBrush entries ({count} x 8 bytes)"
    out := addLine out "# [index] raw bytes | ref, name_offset and flags"
    match animBrushEntries payload count container.bigEndian with
    | some (entries, blob) =>
      for ((reference, nameOffset, flags), j) in entries.toList.zipIdx do
        let record := payload.extract (j * 8) (j * 8 + 8)
        out := out ++ s!"  [{padLeft 4 (toString j)}] {hexBytes record} | ref={reference} name_offset=0x{MmfExtract.Bytes.hexPad 4 nameOffset.toNat} flags=0x{MmfExtract.Bytes.hex flags.toNat}"
        let named := match stringAtOffset blob nameOffset.toNat with
          | some name => " -> \"" ++ escapeBytes name ++ "\""
          | none => " -> <outside name blob>"
        out := out ++ named ++ "\n"
    | none =>
      out := addLine out "# could not decode AnimBrush entries"
      out := out ++ hexdump payload 0
    out := addLine out s!"## Name blob ({fixed} = 0x{MmfExtract.Bytes.hex fixed}, {payload.size - fixed} bytes)"
    out := out ++ hexdump (payload.extract fixed payload.size) fixed
  | Layout.textList =>
    match textListEntries payload count container.bigEndian with
    | some (offsets, blob) =>
      out := addLine out s!"## String offsets ({count} x 4 bytes; relative to the string blob)"
      for (offset, k) in offsets.toList.zipIdx do
        let record := payload.extract (k * 4) (k * 4 + 4)
        out := out ++ s!"  [{k}] {hexBytes record} | offset=0x{MmfExtract.Bytes.hexPad 4 offset.toNat}"
        let found := match stringAtOffset blob offset.toNat with
          | some s => " -> \"" ++ escapeBytes s ++ "\""
          | none => " -> <outside string blob>"
        out := out ++ found ++ "\n"
      let fixed := count * 4
      out := addLine out s!"## Shared string blob ({fixed} = 0x{MmfExtract.Bytes.hex fixed}, {blob.size} bytes; raw hexdump)"
      out := out ++ hexdump blob fixed
    | none =>
      out := addLine out "# could not decode TextList"
      out := out ++ hexdump payload 0
  | Layout.stringTable =>
    match stringTableEntries payload count container.bigEndian with
    | some (entries, blob) =>
      out := addLine out s!"## StringTable entries ({count} x 10 bytes; id, offset, reserved)"
      for ((id, entryOffset, reserved), j) in entries.toList.zipIdx do
        let record := payload.extract (j * 10) (j * 10 + 10)
        out := out ++ s!"  [{padLeft 3 (toString j)}] {hexBytes record} | id={id} offset=0x{MmfExtract.Bytes.hexPad 4 entryOffset.toNat} reserved={reserved}"
        out := out ++
          match entryOffset.toNat with
          | 0xffff => " -> <sentinel 0xffff: absent>"
          | 0xfffe => " -> <sentinel 0xfffe: absent>"
          | 0xfffd => " -> <sentinel 0xfffd: absent>"
          | found =>
            match stringAtOffset blob found with
            | some s => " -> \"" ++ escapeBytes s ++ "\""
            | none => " -> <offset outside blob>"
        out := out ++ "\n"
      out := addLine out s!"## Shared string blob ({blob.size} bytes, raw hexdump)"
      out := out ++ hexdump blob (count * 10)
    | none =>
      out := addLine out "# could not decode StringTable"
      out := out ++ hexdump payload 0
  | Layout.opaque => pure ()

  return addLine out s!"# All {payload.size} payload bytes are represented above."
-------------------------------------------------------------------------------
-- Extraction
-------------------------------------------------------------------------------

/-- Write every payload byte-for-byte. -/
private def writeOriginals (container : Container) (outDir : System.FilePath) : IO Nat := do
  IO.FS.createDirAll outDir
  let mut written : Nat := 0
  let mut failed : Nat := 0
  for index in [0:container.resourceCount.toNat] do
    let name := Path.sanitize (container.name index) index
    let file := MmfExtract.Path.join outDir (pad0 5 index ++ "_" ++ name ++ ".bin")
    match container.payload index with
    | .ok data =>
      IO.FS.writeBinFile file data
      written := written + 1
    | .error e =>
      failed := failed + 1
      IO.eprintln s!"warning: could not write {MmfExtract.Path.str file} (object {index} '{container.name index}'): {e}"
  if failed > 0 then
    IO.eprintln s!"warning: {failed} of {container.resourceCount} script object payloads could not be written"
  return written

/-- Add one to the count for `name`, keeping the list sorted by name. -/
private def bump (counts : List (String × Nat)) (name : String) : List (String × Nat) :=
  match counts with
  | [] => [(name, 1)]
  | (other, count) :: rest =>
    if name < other then (name, 1) :: (other, count) :: rest
    else if name == other then (other, count + 1) :: rest
    else (other, count) :: bump rest name

/-- Write the structured corpus: metadata, tables, strings and per-object dumps. -/
private def writeModern (container : Container) (metaData : ScriptMeta) (outDir : System.FilePath) :
    IO Nat := do
  let objectsDir := MmfExtract.Path.join outDir "objects"
  IO.FS.createDirAll objectsDir

  let n := container.resourceCount.toNat
  -- How many objects each type has, in type-table order.
  let mut typeCounts : Array Nat := Array.replicate metaData.types.size 0
  for t in metaData.objectTypes do
    if t.toNat < typeCounts.size then
      typeCounts := typeCounts.set! t.toNat (typeCounts.getD t.toNat 0 + 1)

  -- Classify every object once; the same decision is used for the JSON coverage
  -- block and for the per-object writer.
  let mut layouts : Array Layout := #[]
  let mut layoutCounts : List (String × Nat) := []
  let mut payloadBytes : Nat := 0
  let mut recordBytes : Nat := 0
  let mut tailBytes : Nat := 0
  let mut opaqueBytes : Nat := 0
  for index in [0:n] do
    let layout := classify container metaData index
    layouts := layouts.push layout
    layoutCounts := bump layoutCounts layout.name
    let len := (container.payloadLen index).toOption.getD 0
    payloadBytes := payloadBytes + len
    if layout == Layout.opaque then
      opaqueBytes := opaqueBytes + len
    else
      let (count, recordSize) := elementInfo metaData index
      let fixed := count * recordSize
      recordBytes := recordBytes + fixed
      tailBytes := tailBytes + (len - fixed)

  IO.FS.writeBinFile (MmfExtract.Path.join outDir "container.json")
    ((containerJson container metaData layouts typeCounts layoutCounts payloadBytes recordBytes
      tailBytes opaqueBytes).toUTF8)
  IO.FS.writeBinFile (MmfExtract.Path.join outDir "types.txt")
    ((typesTxt container metaData typeCounts).toUTF8)
  IO.FS.writeBinFile (MmfExtract.Path.join outDir "strings.txt")
    ((stringsTxt container metaData layouts).toUTF8)

  let mut written : Nat := 0
  let mut failed : Nat := 0
  for index in [0:n] do
    let name := Path.sanitize (container.name index) index
    let file := MmfExtract.Path.join objectsDir (pad0 5 index ++ "_" ++ name ++ ".txt")
    match container.payload index with
    | .error e =>
      failed := failed + 1
      IO.eprintln s!"warning: script object {index} ('{container.name index}'): {e}"
    | .ok _ =>
      IO.FS.writeBinFile file
        ((objectDump container metaData index ((layouts[index]?).getD Layout.opaque)).toUTF8)
      written := written + 1
  if failed > 0 then
    IO.eprintln s!"warning: {failed} of {n} script object dumps could not be written"
  return written

/-- Extract the scripts of a container, returning how many were written.

Individual write failures are reported on stderr and skipped; the return value
is the number of objects that were actually written.
-/
def extract (container : Container) (outDir : System.FilePath) (original : Bool) : IO (Except String Nat) := do
  if container.kind != ContainerKind.scripts then
    return .error s!"{MmfExtract.Path.str container.path}: expected a Scripts container"
  let some metaData := container.scriptMeta
    | return .error s!"{MmfExtract.Path.str container.path}: missing script metadata"
  IO.FS.createDirAll outDir
  if original then
    return .ok (← writeOriginals container outDir)
  else
    return .ok (← writeModern container metaData outDir)

end MmfExtract.Script
