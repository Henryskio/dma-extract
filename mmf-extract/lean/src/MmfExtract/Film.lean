import MmfExtract.Bytes
import MmfExtract.Container
import MmfExtract.Lz77
import MmfExtract.Path
import MmfExtract.Picture
import MmfExtract.Png

/-!
`MMFW Films` resources (`.MMF`).

A port of `src/film.rs`. A film payload is an inner chunk stream: a full-frame
key frame, then per-timestamp deltas, plus audio blocks. The modern output is
one PNG per frame and a `manifest.json` describing the timeline; `--original`
writes the inner `.flm` payload verbatim, because the container's sizes table
is not a decompressed-size table for Films.

See `docs/FILMS.md` and `docs/SEQUEL.md` for the measured layout of the third
and second games.
-/

namespace MmfExtract.Film

open MmfExtract.Container (Container)

/-- Tag at the start of every inner chunk. -/
def chunkTag : UInt16 := 0xF789

def typeAudio : UInt16 := 0x02
def typeMarker : UInt16 := 0x03
def typeTimestamp : UInt16 := 0x06
def typeEnd : UInt16 := 0x07

/-- Key frame chunk. The third game writes type `0x0A`, the second `0x00`.

These are the only two values ever measured: `docs/FILMS.md` reads them off the
version-3 `/RawAssets/DMA.MMF` and `docs/SEQUEL.md` off the version-2 container.
The first game ships no Films container, so no version-1 chunk type is asserted.
-/
def typeFullFrame : UInt16 := 0x0A

/-- Key frame chunk of the second game. -/
def typeFullFrameV2 : UInt16 := 0x00

/-- Delta chunk. The third game writes type `0x0B`, the second `0x01`. -/
def typeDelta : UInt16 := 0x0B

/-- Delta chunk of the second game. -/
def typeDeltaV2 : UInt16 := 0x01

/-- The largest per-film table byte total the metadata solver will try. -/
def maxRecordLen : Nat := 26

/-- Whether this container's films use the second game's chunk numbering.

Everything else is the third game's, which is the only other layout that has
been measured. Version 1 is **not** folded in here: it has no Films container on
the disc, so routing it down the third game's path would be a guess, and
[`parseFilmMetadata`] rejects it instead.
-/
def isFilmV2 (container : Container) : Bool := container.version == 2

/-- The key-frame chunk type this container's film uses. -/
def fullFrameKind (container : Container) : UInt16 :=
  if isFilmV2 container then typeFullFrameV2 else typeFullFrame

/-- Zero-pad `n` to `width` digits. -/
private def pad0 (width : Nat) (n : Nat) : String :=
  let digits := toString n
  String.ofList (List.replicate (width - min width digits.length) '0') ++ digits

/-- One inner chunk: an offset, a kind, and a length. -/
structure Chunk where
  offset : Nat
  kind : UInt16
  len : Nat
  deriving Inhabited

/-- One audio block of the inner stream. The PCM bytes are not decoded;
`--original` preserves them exactly. -/
structure AudioChunk where
  chunkOffset : Nat
  chunkLen : Nat
  /-- Value of the block's first `u32` (the PCM byte count). -/
  dataLen : UInt32
  /-- Value of the block's third `u32`; `0xFFFFFFFF` marks a sync block. -/
  index : UInt32
  dataOffset : Nat
  sync : Bool
  deriving Inhabited

/-- One decoded frame in timeline order. -/
structure Frame where
  /-- Timestamp of this frame, in game ticks (the shipped film uses 83-tick
  spacing). -/
  startTick : UInt32
  /-- Time until the next frame (the last frame reuses the previous gap). -/
  durationTick : UInt32
  /-- True when the timestamp had no following delta chunk, so this frame is
  the previous frame repeated. -/
  duplicate : Bool
  /-- Raw 8bpp palette indices, `stride * height` bytes. -/
  indices : ByteArray
  timestampChunk : Option Nat
  deltaChunk : Option Nat
  fullChunk : Option Nat
  /-- Rectangles from the delta header, stored as `[left, top, right,
  bottom]`. They are advisory: the RLE stream is authoritative. -/
  deltaRects : Array (Nat × Nat × Nat × Nat)
  deriving Inhabited

/-- A fully decoded film. -/
structure Film where
  index : Nat
  name : String
  width : Nat
  height : Nat
  stride : Nat
  /-- `[top, left, bottom, right]` from the per-film record. -/
  bounds : Array UInt16
  record : ByteArray
  palette : ByteArray
  paletteCount : UInt16
  frames : Array Frame
  chunks : Array Chunk
  /-- Per chunk kind, how many. Sorted by kind, because a `BTreeMap` is what
  the Rust port iterates. -/
  chunkKinds : Array (UInt16 × Nat)
  audio : Array AudioChunk
  uniqueFrames : Nat
  deriving Inhabited

/-- Total timeline length, in ticks. -/
def Film.durationTicks (film : Film) : Nat :=
  film.frames.foldl (fun acc frame => acc + frame.durationTick.toNat) 0

/-- The films' metadata region, as the tables present it. -/
structure FilmMeta where
  record : ByteArray
  /-- `[top, left, bottom, right]`. -/
  bounds : Array UInt16
  width : Nat
  height : Nat
  stride : Nat
  palette : ByteArray
  paletteCount : UInt16
  deriving Inhabited

-------------------------------------------------------------------------------
-- Metadata
-------------------------------------------------------------------------------

/-- Read a big-endian `u16` that must be inside the file. -/
private def be16 (data : ByteArray) (off : Nat) : Except String UInt16 :=
  if off + 2 > data.size then
    throw s!"read past end of file at {MmfExtract.Bytes.hex off}"
  else
    Bytes.u16At data off true

/-- Parse the Films metadata region for one resource.

After the common names table comes the generic `4*N` u32 array, then the film
block: `u16` marker (7 in the third game, 5 in the second), the per-film
tables, `u16` palette count and that many 1024-byte palettes.

The per-film tables are **not** records, and reading them as one is what broke
the third game's 3D sibling. The loader (`MM2DFilmResourceFile`'s vtable slot 4,
`DMA3D.exe` at `0x477940`) reads, in this order:

```text
u16     marker, which must be 7
N       bytes       raw
N       bytes       raw
4*N     u16  big-endian
N       u32  big-endian
4*N     bytes       raw
N       u16  big-endian
N       u32  big-endian
N       u16  big-endian
u16     palette count, must be > 0
P * 1024 bytes      palettes
```

That is **26*N** bytes of per-film data, which is exactly what a 26-byte record
table would also total — the coincidence that hid the difference. The third and
second games ship one film each, so `2*N` precedes the rects and the rect lands
at `block + 4`, the same address a record's `+2` field would have held.
`DMA3D.MMF` ships three, so the rects sit at `block + 2 + 2*N`.

The rect array is `{top, left, bottom, right}` per film, one `u16` each,
big-endian, in resource order. That is measured, not assumed: for all three of
`DMA3D.MMF`'s films, `stride8(right - left) * (bottom - top)` equals the byte
count the film's own key-frame chunk declares it decodes to.
-/
def parseFilmMetadata (container : Container) (index : Nat) : Except String FilmMeta := do
  let n := container.resourceCount.toNat
  if index >= n then
    throw s!"film index {index} out of range"
  -- Only versions 2 and 3 have been measured. The first game ships no Films
  -- container, so it has no measured marker, record length or chunk type;
  -- assuming it looks like version 3 would be a guess.
  if container.version == 1 then
    throw s!"{MmfExtract.Path.str container.path}: version 1 has no measured Films layout (no Film container ships with the first game; see docs/GAME1.md §1)"
  let data := container.data
  let block := container.namesEnd + 4 * n
  let marker ← be16 data block
  if marker != 7 && marker != 5 then
    throw s!"{MmfExtract.Path.str container.path}: film metadata marker at {MmfExtract.Bytes.hex block} is {marker}, expected 7 or 5"

  -- Solve: the per-film table byte total and the palette count, such that
  --   block + 2 + table * n + 2 + 1024 * palette_count == payload_start
  -- The payload starting immediately after the last palette is what pins the
  -- total down, so it is solved for rather than assumed.
  let payloadStart := (container.offsets[0]?).getD 0 |>.toNat
  let found : Nat × Nat := Id.run do
    -- Every even byte total from 16 to `maxRecordLen`, the range the Rust port
    -- steps through.
    for table in List.range (maxRecordLen / 2 - 7) |>.map (· * 2 + 16) do
      let countAt := block + 2 + table * n
      let paletteCount ← match be16 data countAt with
        | .ok v => some v.toNat
        | .error _ => none
      match paletteCount with
      | some count =>
        if count == 0 || count > 8 then
          pure ()
        else
          let palettesEnd := countAt + 2 + 1024 * count
          if palettesEnd == payloadStart then
            return (table, count)
      | none => pure ()
    return (0, 0)
  let table := found.1
  let paletteCount := found.2
  if table == 0 then
    throw s!"{MmfExtract.Path.str container.path}: no film table/palette count ends exactly at the payload start {MmfExtract.Bytes.hex payloadStart}"

  let tablesStart := block + 2
  let paletteCountAt := tablesStart + table * n
  let palettesStart := paletteCountAt + 2
  if palettesStart + 1024 * paletteCount > data.size then
    throw s!"{MmfExtract.Path.str container.path}: film metadata ends at {MmfExtract.Bytes.hex (palettesStart + 1024 * paletteCount)}, past the {MmfExtract.Bytes.hex data.size}-byte file"

  -- The rect array is the third of the eight tables: two single-byte tables
  -- come first, so it starts `2*n` bytes in and holds four big-endian u16 --
  -- eight bytes -- per film. This is the address a record's `+2` field would
  -- hold when there is one film, which is why the record reading worked for
  -- the second and third games and failed on the 3D sibling.
  let rectAt := tablesStart + 2 * n + 8 * index
  if rectAt + 8 > palettesStart then
    throw s!"{MmfExtract.Path.str container.path}: film {index}'s rect at {MmfExtract.Bytes.hex rectAt} is outside the metadata region"
  let bounds := #[    (Bytes.u16At data (rectAt + 0) true).toOption.getD 0,
    (Bytes.u16At data (rectAt + 2) true).toOption.getD 0,
    (Bytes.u16At data (rectAt + 4) true).toOption.getD 0,
    (Bytes.u16At data (rectAt + 6) true).toOption.getD 0]
  let top := bounds[0]!
  let left := bounds[1]!
  let bottom := bounds[2]!
  let right := bounds[3]!
  if right.toNat <= left.toNat || bottom.toNat <= top.toNat then
    throw s!"film {index}: bad bounds top={top} left={left} bottom={bottom} right={right}"
  let width := right.toNat - left.toNat
  let height := bottom.toNat - top.toNat
  -- The third game pads 8bpp rows to a multiple of four bytes; the second game
  -- stores rows unpadded (its key frame is exactly width * height bytes).
  let stride := if container.version == 2 then width else Picture.stride8 width

  -- The per-film record as the tables present it, reassembled for the report
  -- and for tests that pin the bytes. Field order is the reader's:
  -- byte, byte, the rect, u32, four bytes, u16, u32, u16.
  let b1 := tablesStart + index
  let b2 := tablesStart + n + index
  let r4 := tablesStart + 2 * n + 8 * n + 4 * index
  let r5 := tablesStart + 6 * n + 8 * n + 4 * index
  let r6 := tablesStart + 10 * n + 8 * n + 2 * index
  let r7 := tablesStart + 12 * n + 8 * n + 4 * index
  let r8 := tablesStart + 16 * n + 8 * n + 2 * index
  let record : ByteArray :=
    if r8 + 2 <= palettesStart then
      ByteArray.mk #[data[b1]!, data[b2]!]
        |>.append (data.extract rectAt (rectAt + 8))
        |>.append (data.extract r4 (r4 + 4))
        |>.append (data.extract r5 (r5 + 4))
        |>.append (data.extract r6 (r6 + 2))
        |>.append (data.extract r7 (r7 + 4))
        |>.append (data.extract r8 (r8 + 2))
    else
      ByteArray.empty

  -- One palette per film, in resource order. All four shipped containers have
  -- `palette_count == N`, so the film's own entry is `index`; a container that
  -- shipped fewer palettes than films would need measuring.
  let paletteIndex := min index (paletteCount - 1)
  let paletteAt := palettesStart + 1024 * paletteIndex

  return {
    record, bounds, width, height, stride,
    palette := data.extract paletteAt (paletteAt + 1024),
    paletteCount := UInt16.ofNat paletteCount
  }

-------------------------------------------------------------------------------
-- The inner chunk stream
-------------------------------------------------------------------------------

/-- Parse the inner chunk stream, checking that the chunks tile the payload. -/
def parseChunks (data : ByteArray) : Except String (Array Chunk) := do
  let mut chunks : Array Chunk := #[]
  let mut offset : Nat := 0
  while offset + 8 <= data.size do
    let tag := (Bytes.u16At data offset true).toOption.getD 0
    if tag.toNat != chunkTag.toNat then
      throw s!"bad chunk tag {tag} at {MmfExtract.Bytes.hex offset}, expected {chunkTag}"
    let kind := (Bytes.u16At data (offset + 2) true).toOption.getD 0
    let len := (Bytes.u32At data (offset + 4) true).toOption.getD 0 |>.toNat
    let stop := offset + 8 + len
    if stop > data.size then
      throw s!"chunk type {kind} at {MmfExtract.Bytes.hex offset} length {len} runs past the payload end"
    chunks := chunks.push { offset, kind, len }
    offset := stop
  if offset != data.size then
    throw s!"{data.size - offset} trailing bytes at {MmfExtract.Bytes.hex offset} after the last chunk"
  return chunks

/-- The payload of one chunk, past its eight-byte header. -/
def chunkPayload (data : ByteArray) (chunk : Chunk) : Except String ByteArray := do
  let start := chunk.offset + 8
  let stop := start + chunk.len
  match data[start]?, data[stop - 1]? with
  | some _, some _ => return data.extract start stop
  | _, _ => throw s!"chunk at {MmfExtract.Bytes.hex chunk.offset} is outside the payload"

/-- Decode one key frame.

The second game stores the key frame as raw palette indices with no header and
no LZ77: exactly `width * height` bytes (800x600). The third game wraps an LZ77
stream in an eight-byte header carrying both lengths.
-/
def decodeFullFrame (data : ByteArray) (chunk : Chunk) (v2 : Bool) (width height : Nat) :
    Except String ByteArray := do
  let payload ← chunkPayload data chunk
  if v2 then
    if payload.size != width * height then
      throw s!"full-frame chunk at {MmfExtract.Bytes.hex chunk.offset} is {payload.size} bytes, expected {width * height} for a {width}x{height} frame"
    return payload
  if payload.size < 8 then
    throw s!"full-frame chunk at {MmfExtract.Bytes.hex chunk.offset} is too short"
  let lz77Len := (Bytes.u32At payload 0 true).toOption.getD 0 |>.toNat
  let decodedLen := (Bytes.u32At payload 4 true).toOption.getD 0 |>.toNat
  if lz77Len > payload.size - 8 then
    throw s!"full-frame chunk at {MmfExtract.Bytes.hex chunk.offset} declares {lz77Len} compressed bytes but only {payload.size - 8} follow the header"
  match Lz77.decompress (payload.extract 8 (8 + lz77Len)) with
  | .error e =>
    throw s!"full-frame chunk at {MmfExtract.Bytes.hex chunk.offset}: LZ77 stream ({lz77Len} bytes): {e}"
  | .ok decoded =>
    if decoded.size != decodedLen then
      throw s!"full-frame chunk at {MmfExtract.Bytes.hex chunk.offset}: decoded {decoded.size} bytes, header says {decodedLen}"
    return decoded

/-- The RLE data of one delta chunk, and the header's rectangles. -/
def decodeDelta (data : ByteArray) (chunk : Chunk) (v2 : Bool) :
    Except String (ByteArray × Array (Nat × Nat × Nat × Nat)) := do
  let payload ← chunkPayload data chunk
  if payload.size < 168 then
    throw s!"delta chunk at {MmfExtract.Bytes.hex chunk.offset} is too short"

  -- Layout, third game: [u32 lz77_len][u32 decoded_len][168 bytes of rects][LZ77
  -- stream] whose output is the RLE data. Layout, second game: [168 bytes of
  -- rects][RLE stream] with no LZ77 wrapper.
  let (rectOff, delta) ←
    if v2 then
      pure (0, payload.extract 168 payload.size)
    else
      let lz77Len := (Bytes.u32At payload 0 true).toOption.getD 0 |>.toNat
      let decodedLen := (Bytes.u32At payload 4 true).toOption.getD 0 |>.toNat
      if lz77Len > payload.size - 168 then
        throw s!"delta chunk at {MmfExtract.Bytes.hex chunk.offset} declares {lz77Len} compressed bytes but only {payload.size - 168} follow the rect table"
      match Lz77.decompress (payload.extract 168 (168 + lz77Len)) with
      | .error e =>
        throw s!"delta chunk at {MmfExtract.Bytes.hex chunk.offset}: LZ77 stream ({lz77Len} bytes): {e}"
      | .ok expanded =>
        if expanded.size != decodedLen then
          throw s!"delta chunk at {MmfExtract.Bytes.hex chunk.offset}: decoded {expanded.size} bytes, header says {decodedLen}"
        pure (8, expanded)

  -- Each rect is four u16: top, left, bottom, right. Store the more
  -- conventional left/top/right/bottom order.
  let mut rects : Array (Nat × Nat × Nat × Nat) := #[]
  for i in [0:20] do
    let record := rectOff + i * 8
    let top := (Bytes.u16At payload record true).toOption.getD 0 |>.toNat
    let left := (Bytes.u16At payload (record + 2) true).toOption.getD 0 |>.toNat
    let bottom := (Bytes.u16At payload (record + 4) true).toOption.getD 0 |>.toNat
    let right := (Bytes.u16At payload (record + 6) true).toOption.getD 0 |>.toNat
    rects := rects.push (left, top, right, bottom)

  return (delta, rects)

/-- A signed big-endian `i16` at `off` of `data`. -/
private def Δ16 (data : ByteArray) (off : Nat) : Int :=
  let v := (Bytes.u16At data off true).toOption.getD 0 |>.toNat
  Int.ofNat v - (if v >= 0x8000 then 65536 else 0)

/-- Where a write of `count` bytes at `cursor` lands in a `frameLen` frame. -/
private def checkedSpan (cursor : Int) (count : Nat) (frameLen : Nat) : Except String Nat := do
  if cursor < 0 then
    throw s!"write at negative frame offset {cursor}"
  let start := cursor.toNat
  let stop := start + count
  if stop > frameLen then
    throw s!"write {start}..{stop} is past the {frameLen}-byte frame"
  return start

/-- Apply the cursor-relative run-length delta to an 8bpp frame, returning the
new frame.

Each record is two big-endian `i16`s: a signed cursor displacement and a signed
length. A zero length only moves the cursor, a positive length copies literals
and a negative length repeats a four-byte pattern. The pattern bytes are
byte-reversed before repetition (the game's `FUN_004c396b` swaps the 32-bit
word).
-/
def applyDelta (frame : ByteArray) (delta : ByteArray) : Except String (Array UInt8) := do
  let mut pos : Nat := 0
  let mut cursor : Int := 0
  let mut out : Array UInt8 := frame.data
  while pos + 4 <= delta.size do
    let displacement := Δ16 delta pos
    let length := Δ16 delta (pos + 2)
    pos := pos + 4
    cursor := cursor + displacement
    if length == 0 then
      continue
    if length < 0 then
      let count := (-length).toNat
      if pos + 4 > delta.size then
        throw s!"pattern record at {MmfExtract.Bytes.hex pos} has no four-byte pattern"
      -- The four bytes are byte-reversed, the way the game's `FUN_004c396b`
      -- swaps the 32-bit word.
      let pattern := #[delta[pos + 3]!, delta[pos + 2]!, delta[pos + 1]!, delta[pos]!]
      pos := pos + 4
      let start ← checkedSpan cursor count out.size
      for i in [0:count] do
        out := out.set! (start + i) pattern[i % 4]!
      cursor := cursor + Int.ofNat count
    else
      let count := length.toNat
      if pos + count > delta.size then
        throw s!"literal record at {MmfExtract.Bytes.hex pos} runs past the delta data"
      let start ← checkedSpan cursor count out.size
      for i in [0:count] do
        out := out.set! (start + i) delta[pos + i]!
      pos := pos + count
      cursor := cursor + Int.ofNat count
  if pos != delta.size then
    throw s!"{delta.size - pos} trailing delta bytes at {MmfExtract.Bytes.hex pos}"
  return out

-------------------------------------------------------------------------------
-- Decoding one film
-------------------------------------------------------------------------------

/-- Decode one film resource into its frame timeline and audio map. -/
def decodeFilm (container : Container) (index : Nat) : Except String (Film × Array String) := do
  if container.kind != ContainerKind.films then
    throw s!"{MmfExtract.Path.str container.path}: expected a Films container"
  if index >= container.resourceCount.toNat then
    throw s!"{MmfExtract.Path.str container.path}: film index {index} out of range (resource count {container.resourceCount})"

  let name := container.name index
  let loc := MmfExtract.Path.str container.path
  let info ← parseFilmMetadata container index
  let data ← container.payload index
  let v2 := isFilmV2 container

  let chunks ←
    match parseChunks data with
    | .error e => throw s!"parsing film {index} ('{name}') chunk stream: {e}"
    | .ok chunks => pure chunks
  -- Per chunk kind, how many; sorted by kind, as the Rust port's `BTreeMap`
  -- iteration is.
  let chunkKinds := Id.run do
    let mut counts : Array (UInt16 × Nat) := #[]
    for chunk in chunks.toList do
      let existing := (counts.findIdx? (fun p => p.1 == chunk.kind)).getD counts.size
      if existing < counts.size then
        counts := counts.set! existing (chunk.kind, counts[existing]!.2 + 1)
      else
        counts := counts.push (chunk.kind, 1)
    return counts.qsort (·.1 ≤ ·.1)

  let fullKind := fullFrameKind container
  let fullOffsets := chunks.foldl (fun acc chunk =>
    if chunk.kind == fullKind then acc.push chunk.offset else acc) #[]
  let firstFullOffset ←
    match fullOffsets[0]? with
    | none => throw s!"film {index} ('{name}'): no full-frame chunk (type {MmfExtract.Bytes.hex fullKind.toNat}) in the stream"
    | some offset => pure offset
  let firstChunk ←
    match chunks.find? (fun chunk => chunk.offset == firstFullOffset) with
    | none => throw s!"film {index} ('{name}'): no full-frame chunk in the stream"
    | some chunk => pure chunk
  let mut buffer ← decodeFullFrame data firstChunk v2 info.width info.height
  let frameSize := info.stride * info.height
  if buffer.size != frameSize then
    throw s!"film {index} ('{name}'): full frame is {buffer.size} bytes, expected {info.width}x{info.height} (stride {info.stride}) = {frameSize}"

  -- Build the timeline before decoding so malformed chunk order is reported
  -- clearly.
  -- The timeline is built before any decoding so that malformed chunk order is
  -- reported clearly. It is built in the same `Except String` monad the rest of
  -- this function uses, because reading a chunk's payload can fail.
  let mut events : Array (UInt32 × Nat × Option Nat × Option Nat) := #[]
  for chunk in chunks.toList do
      if chunk.kind == typeTimestamp then
        let payload ← chunkPayload data chunk
        if payload.size < 4 then
          throw s!"film {index} ('{name}'): timestamp chunk at {MmfExtract.Bytes.hex chunk.offset} is {payload.size} bytes"
        events := events.push ((Bytes.u32At payload 0 true).toOption.getD 0, chunk.offset, none, none)
      else if chunk.kind == fullKind then
        -- A key frame in mid-stream. It belongs to the frame whose timestamp
        -- precedes it: the stream reads `timestamp, key frame` or
        -- `timestamp, delta`, so the chunk after a timestamp is what that
        -- frame is built from.
        if events.isEmpty then
          -- Before the first timestamp: that is the seeding key frame, already
          -- decoded above.
          if chunk.offset != firstFullOffset then
            throw s!"film {index} ('{name}'): key frame at {MmfExtract.Bytes.hex chunk.offset} precedes the first timestamp and is not the first key frame"
        else
          let last := events.size - 1
          let (tick, ts, delta, full) := events[last]!
          if full.isSome || delta.isSome then
            throw s!"film {index} ('{name}'): key frame at {MmfExtract.Bytes.hex chunk.offset} follows a frame that already has one"
          events := events.set! last (tick, ts, delta, some chunk.offset)
      else if chunk.kind == typeDelta || chunk.kind == typeDeltaV2 then
        if events.isEmpty then
          throw s!"film {index} ('{name}'): delta chunk at {MmfExtract.Bytes.hex chunk.offset} has no preceding timestamp"
        let last := events.size - 1
        let (tick, ts, delta, full) := events[last]!
        if delta.isSome then
          throw s!"film {index} ('{name}'): more than one delta chunk follows timestamp at {MmfExtract.Bytes.hex ts}"
        events := events.set! last (tick, ts, some chunk.offset, full)

  let ticks := events.toList.map (·.1)
  for (a, b) in ticks.zip ticks.tail do
    if b < a then
      throw s!"film {index} ('{name}'): timestamps go backwards ({a} then {b})"

  let mut frames : Array Frame := #[]
  -- Notes the decoder would print on the way out; they travel back as data
  -- because this function is pure.
  let mut notes : Array String := #[]
  -- `events` continues to be read below; the frame loop indexes it.
  let tickOf (i : Nat) : UInt32 := (events[i]?).map (·.1) |>.getD 0
  -- Frame zero is the full frame at time zero; the first timestamp is the
  -- moment the next frame becomes visible.
  let frame0Duration := tickOf 0
  frames := frames.push {
    startTick := 0, durationTick := frame0Duration, duplicate := false,
    indices := buffer, timestampChunk := none, deltaChunk := none,
    fullChunk := some firstFullOffset, deltaRects := #[]
  }

  let eventCount := events.size
  for i in [0:eventCount] do
    let (tick, tsOffset, deltaOffset, fullOffset) := events[i]!
    let previous := tickOf (i - 1)
    let durationTick :=
      if i + 1 < eventCount then (events[i + 1]!.1) - tick
      else if i > 0 then tick - previous
      else frame0Duration
    let mut rects : Array (Nat × Nat × Nat × Nat) := #[]
    let mut wrote := true
    let mut keyOffset := none
    match fullOffset with
    | some offset =>
      -- This frame starts from a key frame rather than from the previous
      -- frame, so the buffer is replaced. Its size must match the same frame
      -- geometry the seeding key frame does.
      let chunk ←
        match chunks.find? (fun chunk => chunk.offset == offset) with
        | none => throw s!"film {index} ('{name}'): no chunk at {MmfExtract.Bytes.hex offset}"
        | some chunk => pure chunk
      let decoded ←
        match decodeFullFrame data chunk v2 info.width info.height with
        | .error e => throw s!"film {index} ('{name}'): decoding full frame at {MmfExtract.Bytes.hex offset}: {e}"
        | .ok decoded => pure decoded
      if decoded.size != frameSize then
        throw s!"film {index} ('{name}'): full frame at {MmfExtract.Bytes.hex offset} is {decoded.size} bytes, expected {info.width}x{info.height} (stride {info.stride}) = {frameSize}"
      buffer := decoded
      keyOffset := some offset
    | none =>
      match deltaOffset with
      | some offset =>
        let chunk ←
          match chunks.find? (fun chunk => chunk.offset == offset) with
          | none => throw s!"film {index} ('{name}'): no chunk at {MmfExtract.Bytes.hex offset}"
          | some chunk => pure chunk
        match decodeDelta data chunk v2 with
        | .ok (delta, headerRects) =>
          rects := headerRects
          match applyDelta buffer delta with
          | .error e =>
            -- The sequel's delta grammar is not fully recovered: a few chunks
            -- drive the cursor negative. Keep the previous frame instead of
            -- losing the whole film.
            if !v2 then
              throw s!"film {index} ('{name}'): decoding delta chunk at {MmfExtract.Bytes.hex offset}: {e}"
            notes := notes.push s!"warning: {loc}: film {index} ('{name}'): delta chunk at {MmfExtract.Bytes.hex offset} not decoded ({e}); reusing the previous frame"
            wrote := false
          | .ok updated => buffer := ByteArray.mk updated
        | .error e =>
          if !v2 then
            throw s!"film {index} ('{name}'): decoding delta chunk at {MmfExtract.Bytes.hex offset}: {e}"
          notes := notes.push s!"warning: {loc}: film {index} ('{name}'): delta chunk at {MmfExtract.Bytes.hex offset} not decoded ({e}); reusing the previous frame"
          wrote := false
      | none => pure ()

    frames := frames.push {
      startTick := tick, durationTick,
      -- A frame is a repeat when nothing was sent for it: neither a key frame
      -- nor a delta.
      duplicate := (deltaOffset.isNone && fullOffset.isNone) || !wrote,
      indices := buffer, timestampChunk := some tsOffset, deltaChunk := deltaOffset,
      fullChunk := keyOffset, deltaRects := rects
    }

  -- How many frames are distinct images: the same indices repeated are one.
  let mut seen : Array ByteArray := #[]
  let mut unique : Nat := 0
  for frame in frames.toList do
    if !seen.contains frame.indices then
      seen := seen.push frame.indices
      unique := unique + 1

  let mut audio : Array AudioChunk := #[]
  for chunk in chunks.toList do
    if chunk.kind != typeAudio then
      continue
    let payload ← chunkPayload data chunk
    if payload.size < 12 then
      throw s!"film {index} ('{name}'): audio chunk at {MmfExtract.Bytes.hex chunk.offset} is {payload.size} bytes"
    let dataLen := (Bytes.u32At payload 0 true).toOption.getD 0
    let blockIndex := (Bytes.u32At payload 8 true).toOption.getD 0
    if 12 + dataLen.toNat > payload.size then
      throw s!"film {index} ('{name}'): audio chunk at {MmfExtract.Bytes.hex chunk.offset} declares {dataLen} data bytes but holds {payload.size}"
    audio := audio.push {
      chunkOffset := chunk.offset, chunkLen := chunk.len, dataLen,
      index := blockIndex, dataOffset := chunk.offset + 8 + 12,
      sync := blockIndex == 0xFFFFFFFF
    }

  return ({
    index, name, width := info.width, height := info.height, stride := info.stride,
    bounds := info.bounds, record := info.record, palette := info.palette,
    paletteCount := info.paletteCount, frames, chunks, chunkKinds, audio,
    uniqueFrames := unique
  }, notes)

-------------------------------------------------------------------------------
-- Output
-------------------------------------------------------------------------------

/-- Hex bytes, no separators, as the manifest prints them. -/
def hex (data : ByteArray) : String :=
  String.ofList (data.data.toList.flatMap (fun b => (MmfExtract.Bytes.hexPad 2 b.toNat).toList))

/-- Escape a string for JSON output, including the two escapes the Rust port
does not reach. -/
def jsonEscape (text : String) : String := Id.run do
  let mut out := ""
  for c in text.toList do
    if c == '"' then out := out ++ "\\\""
    else if c == '\\' then out := out ++ "\\\\"
    else if c == '\n' then out := out ++ "\\n"
    else if c == '\r' then out := out ++ "\\r"
    else if c == '\t' then out := out ++ "\\t"
    else if c == '\u0008' then out := out ++ "\\b"
    else if c == '\u000c' then out := out ++ "\\f"
    else if c.toNat < 0x20 then out := out ++ "\\u" ++ MmfExtract.Bytes.hexPad 4 c.toNat
    else out := out.push c
  return out

/-- A double quote, for the `s!` interpolation strings, which do not accept
`\"` inside them. -/
private def q : String := "\""

/-- One JSON `"name": value` field. -/
private def jf (name : String) (value : String) : String :=
  q ++ name ++ q ++ ": " ++ value

/-- A JSON object on one line. -/
private def jObj (fields : List String) : String :=
  "{" ++ String.intercalate ", " fields ++ "}"

/-- The manifest describing one film's timeline and audio. -/
def manifestJson (container : Container) (film : Film) : String := Id.run do
  let mut s := ""
  s := s ++ "{\n"
  s := s ++ s!"  {q}container{q}: {q}{jsonEscape (MmfExtract.Path.str container.path)}{q},\n"
  s := s ++ s!"  {q}film_index{q}: {film.index},\n"
  s := s ++ s!"  {q}name{q}: {q}{jsonEscape film.name}{q},\n"
  s := s ++ s!"  {q}width{q}: {film.width},\n"
  s := s ++ s!"  {q}height{q}: {film.height},\n"
  s := s ++ s!"  {q}stride{q}: {film.stride},\n"
  s := s ++ s!"  {q}frame_size_bytes{q}: {film.stride * film.height},\n"
  s := s ++ "  \"pixel_format\": \"8bpp palette indices; RGBA = palette[transparency, red, green, blue], alpha = 255 - transparency\",\n"
  s := s ++ s!"  {q}record_hex{q}: {q}{hex film.record}{q},\n"
  s := s ++ s!"  {q}record_bounds{q}: [{film.bounds[0]!}, {film.bounds[1]!}, {film.bounds[2]!}, {film.bounds[3]!}],\n"
  s := s ++ "  \"record_note\": \"bytes +2..+10 are u16 BE {top, left, bottom, right}; the rest is preserved verbatim\",\n"
  s := s ++ s!"  {q}palette_count{q}: {film.paletteCount},\n"
  s := s ++ s!"  {q}palette_hex{q}: {q}{hex film.palette}{q},\n"
  s := s ++ s!"  {q}frame_count{q}: {film.frames.size},\n"
  s := s ++ s!"  {q}unique_frame_count{q}: {film.uniqueFrames},\n"
  s := s ++ "  \"timebase\": \"ticks treated as milliseconds (83-tick spacing = 12.05 fps)\",\n"
  s := s ++ s!"  {q}duration_ms{q}: {film.durationTicks},\n"
  s := s ++ s!"  {q}chunk_count{q}: {film.chunks.size},\n"
  let kinds := String.intercalate ", " (film.chunkKinds.toList.map (fun (kind, count) =>
    jf (toString kind) (toString count)))
  s := s ++ "  " ++ jf "chunk_types" ("{" ++ kinds ++ "}") ++ ",\n"
  s := s ++ "  \"offset_note\": \"all chunk offsets are relative to the start of the inner .flm payload\",\n"

  s := s ++ "  \"frames\": [\n"
  for (frame, i) in film.frames.toList.zipIdx do
    s := s ++ "    {\n"
    s := s ++ s!"      {q}index{q}: {i},\n"
    s := s ++ s!"      {q}start_ms{q}: {frame.startTick},\n"
    s := s ++ s!"      {q}duration_ms{q}: {frame.durationTick},\n"
    s := s ++ s!"      {q}duplicate{q}: {frame.duplicate},\n"
    s := s ++ s!"      {q}png{q}: {q}frame_{MmfExtract.Path.padIndex i}{q},\n"
    let source :=
      match frame.fullChunk, frame.deltaChunk, frame.timestampChunk with
      | some offset, _, _ =>
        jObj [jf "kind" (q ++ "full" ++ q), jf "chunk_offset" (toString offset)]
      | _, some delta, some timestamp =>
        jObj [jf "kind" (q ++ "delta" ++ q), jf "timestamp_chunk_offset" (toString timestamp),
          jf "delta_chunk_offset" (toString delta)]
      | _, none, some timestamp =>
        jObj [jf "kind" (q ++ "duplicate" ++ q), jf "timestamp_chunk_offset" (toString timestamp)]
      | _, none, none => jObj [jf "kind" (q ++ "unknown" ++ q)]
      | _, some _, none => jObj [jf "kind" (q ++ "unknown" ++ q)]
    s := s ++ s!"      {q}source{q}: {source},\n"
    let rects := String.intercalate ", " (frame.deltaRects.toList.map (fun (left, top, right, bottom) =>
      s!"[{left},{top},{right},{bottom}]"))
    s := s ++ s!"      {q}delta_rects{q}: [{rects}]\n"
    if i + 1 == film.frames.size then
      s := s ++ "    }\n"
    else
      s := s ++ "    },\n"
  s := s ++ "  ],\n"

  let dataBlocks := film.audio.foldl (fun acc a => if !a.sync then acc + 1 else acc) 0
  let syncBlocks := film.audio.foldl (fun acc a => if a.sync then acc + 1 else acc) 0
  s := s ++ "  \"audio\": {\n"
  s := s ++ s!"    {q}chunk_count{q}: {film.audio.size},\n"
  s := s ++ s!"    {q}data_blocks{q}: {dataBlocks},\n"
  s := s ++ s!"    {q}sync_blocks{q}: {syncBlocks},\n"
  s := s ++ "    \"note\": \"12-byte header [data_len, zero, block_index], then PCM bytes; sync blocks use block_index 4294967295. Audio is preserved verbatim by --original.\",\n"
  s := s ++ "    \"chunks\": [\n"
  for (audio, i) in film.audio.toList.zipIdx do
    let comma := if i + 1 == film.audio.size then "" else ","
    s := s ++ "      " ++ jObj [jf "chunk_offset" (toString audio.chunkOffset),
      jf "chunk_len" (toString audio.chunkLen), jf "data_offset" (toString audio.dataOffset),
      jf "data_len" (toString audio.dataLen), jf "index" (toString audio.index),
      jf "sync" (toString audio.sync)] ++ comma ++ "\n"
  s := s ++ "    ]\n"
  s := s ++ "  }\n"
  s := s ++ "}\n"
  return s

/-- The file name the game's own `.flm` payload is written under. -/
def originalFilename (index : Nat) (safe : String) : String :=
  if safe.toLower.endsWith ".flm" then
    pad0 5 index ++ "_" ++ safe
  else
    pad0 5 index ++ "_" ++ safe ++ ".flm"

/-- The output directory's name for a film: the resource name without its
`.flm` extension, so the extension belongs to the output format. -/
def stripFlm (safe : String) : String :=
  if safe.toLower.endsWith ".flm" then
    let stem := String.ofList (safe.toList.take (safe.length - 4))
    if !stem.isEmpty then stem else safe
  else
    safe

/-- One film's modern output: a PNG per frame and a `manifest.json`. -/
def writeModern (container : Container) (film : Film) (dir : System.FilePath) : IO Unit := do
  for (frame, i) in film.frames.toList.zipIdx do
    let rgba := Picture.indexedRgbaStride film.palette frame.indices
      film.width film.height film.stride
    Picture.writePng (MmfExtract.Path.join dir (s!"frame_{MmfExtract.Path.padIndex i}.png"))
      { width := film.width, height := film.height,
        format := MmfExtract.Picture.PixelFormat.paletted8, rgba }
  IO.FS.writeBinFile (MmfExtract.Path.join dir "manifest.json") ((manifestJson container film).toUTF8)

/-- Extract every film in a container.

With `original`, write each resource's inner `.flm` payload verbatim. Otherwise
decode the frames and write per-film directories containing `frame_NNN.png`
files plus a `manifest.json`. Returns the number of films handled, not the
number of frames.
-/
def extract (container : Container) (outDir : System.FilePath) (original : Bool) :
    IO (Except String Nat) := do
  if container.kind != ContainerKind.films then
    return .error s!"{MmfExtract.Path.str container.path}: expected a Films container"
  IO.FS.createDirAll outDir
  let mut handled : Nat := 0
  if original then
    for index in [0:container.resourceCount.toNat] do
      let safe := Path.sanitize (container.name index) index
      let file := MmfExtract.Path.join outDir (originalFilename index safe)
      match container.payload index with
      | .error e => return .error e
      | .ok data =>
        IO.FS.writeBinFile file data
        handled := handled + 1
    return .ok handled

  for index in [0:container.resourceCount.toNat] do
    match decodeFilm container index with
    | .error e =>
      -- A film that cannot be fully decoded is reported and skipped rather
      -- than aborting the whole run.
      return .error e
    | .ok (film, notes) =>
      for note in notes do
        IO.eprintln note
      let safe := Path.sanitize film.name index
      let dir := MmfExtract.Path.join outDir (pad0 5 index ++ "_" ++ stripFlm safe)
      IO.FS.createDirAll dir
      writeModern container film dir
      handled := handled + 1
  return .ok handled

end MmfExtract.Film
