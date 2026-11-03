import MmfExtract.Bytes
import MmfExtract.Container
import MmfExtract.Path

/-!
MMFW sound resources: decode to PCM, encode with the `flac` tool.

A port of `src/sound.rs`. A sound payload is one of three encodings, chosen by
the container's `flags` byte (bit 0-1: zero means stored PCM, anything else
means differential):

* **8-bit stored PCM** -- the payload's bytes are the samples, unsigned.
* **16-bit stored PCM** -- the payload is byte-swapped on the way out, because
  a big-endian writer stored it (measured on the first game's 228 stored-16-bit
  resources; see `docs/GAME1.md` §5.1).
* **16-bit differential PCM** -- one stored byte expands to one 16-bit sample
  through a 256-entry step table at virtual address `0x532778` /
  file offset `0x132778` in `DMA4.exe`, with an independent per-channel
  predictor reset to zero at the start of the stream (`docs/SOUNDS.md` §4).

The depth itself is checked against the payload: a format byte that says 16-bit
while the payload reads as 8-bit is overridden, because a quiet 16-bit signal
puts almost all of its energy in its low byte.
-/

namespace MmfExtract.Sound

open MmfExtract.Container (Container)

/-- The external encoder, documented as a dependency.

The Rust port chose the official `flac` tool over every crate because no Rust
crate encodes FLAC and ffmpeg's encoder cannot do 8-bit (`docs/SOUNDS.md` §6.1);
the Lean port does the same, through `IO.Process`.
-/
def flacTool : String := "flac"

/-- The `flac` compression level as the tool spells it: `-N`, 0-8.

The level is only spelled `-N` -- there is no long form, and `flac` 1.4.3 reads
a bare `8` as a second input file.
-/
def flacCompression : String := "-8"

/-- The code that represents a zero step (digital silence). -/
def dpcmSilence : UInt8 := 0xF8

/-- How much payload to measure: 20 seconds' worth of samples. -/
def measureSamples : Nat := 20 * 22050

/-- A sample within this of full scale counts as sitting on the rail. -/
def railLevel : Float := 0.99

/-- How much more of the payload must sit on the rails at the declared depth
than at the other one before the depth is overridden. -/
def railMargin : Float := 0.005

/-- The 256-entry `i16` step table, indexed by the stored byte.

Entries 0..47 and 128..175 are absolute step sizes; the remaining non-silence
codes are added to the previous sample of the same channel. The same table is
printed in `docs/SOUNDS.md` §4.
-/
def dpcmTable : Array Int16 :=
  #[
    1,     6,    19,    41,     73,    120,    180,    258,    353,    468,    603,    761,    942,   1149,   1381,   1641,
 1929,  2247,  2596,  2977,   3391,   3839,   4323,   4844,   5401,   5998,   6633,   7310,   8028,   8788,   9593,  10441,
11335, 12276, 13264, 14300,  15385,  16520,  17707,  18945,  20236,  21581,  22981,  24435,  25946,  27515,  29141,  30826,
 2048,  2304,  2560,  2816,   3072,   3328,   3584,   3840,   1024,   1152,   1280,   1408,   1536,   1664,   1792,   1920,
  512,   576,   640,   704,    768,    832,    896,    960,    256,    288,    320,    352,    384,    416,    448,   480,
  128,   144,   160,   176,    192,    208,    224,    240,     64,     72,     80,     88,     96,    104,    112,   120,
   32,    36,    40,    44,     48,     52,     56,     60,     16,     18,     20,     22,     24,     26,     28,    30,
    8,     9,    10,    11,     12,     13,    14,    15,      0,      1,      2,      3,      4,      5,      6,     7,
   -1,    -6,   -19,   -41,    -73,   -120,   -180,   -258,   -353,   -468,   -603,   -761,   -942,  -1149,  -1381,  -1641,
-1929, -2247, -2596, -2977,  -3391,  -3839,  -4323,  -4844,  -5401,  -5998,  -6633,  -7310,  -8028, -8788,  -9593, -10441,
-11335, -12276, -13264, -14300, -15385, -16520, -17707, -18945, -20236, -21581, -22981, -24435, -25946, -27515, -29141, -30826,
-2048, -2304, -2560, -2816,  -3072,  -3328,  -3584,  -3840,  -1024,  -1152,  -1280,  -1408,  -1536,  -1664,  -1792,  -1920,
 -512,  -576,  -640,  -704,   -768,   -832,   -896,   -960,   -256,   -288,   -320,   -352,   -384,   -416,   -448,  -480,
 -128,  -144,  -160,  -176,   -192,   -208,   -224,   -240,    -64,    -72,    -80,    -88,    -96,   -104,   -112,  -120,
  -32,   -36,   -40,   -44,   -48,   -52,   -56,   -60,    -16,    -18,    -20,    -22,    -24,    -26,    -28,   -30,
   -8,    -9,   -10,   -11,   -12,   -13,   -14,   -15,      0,     -1,     -2,     -3,     -4,     -5,     -6,    -7
  ]

/-- How a payload's samples are stored. -/
inductive Encoding where
  | pcm8
  | pcm16
  | dpcm16
  deriving Inhabited, DecidableEq, BEq

/-- The format the container's format byte declares. -/
structure SoundFormat where
  sampleRate : UInt32
  channels : UInt16
  bitsPerSample : UInt16
  deriving Inhabited

/-- One decoded sound resource. -/
structure DecodedSound where
  format : SoundFormat
  encoding : Encoding
  /-- The samples: 8-bit unsigned, or 16-bit signed little-endian. -/
  pcm : ByteArray
  deriving Inhabited

/-- What the container's format byte `C` declares.

| Bits | Meaning |
|---|---|
| 0-1 | sample rate: `0` = 11025, `1` = 22050, `2` or `3` = 44100 Hz |
| 2 | 16-bit samples |
| 3 | stereo (two channels); clear = mono |
| 4-7 | auxiliary; not needed to reconstruct the PCM |
-/
def formatOf (container : Container) (index : Nat) : Except String SoundFormat := do
  let attr := (container.attr[index]? |>.getD 0).toNat
  let rate : UInt32 :=
    match attr &&& 3 with
    | 0 => 11025
    | 1 => 22050
    | _ => 44100
  let channels : UInt16 := if (attr &&& 8) != 0 then 2 else 1
  let bits : UInt16 := if (attr &&& 4) != 0 then 16 else 8
  return { sampleRate := rate, channels, bitsPerSample := bits }

/-- Convert one `Int16` to its two little-endian bytes. -/
private def le16 (v : Int16) : ByteArray :=
  let bits := (Int16.toUInt16 v).toUInt64
  ByteArray.mk #[((bits >>> 8).toNat &&& 0xFF).toUInt8, (bits.toNat &&& 0xFF).toUInt8]

/-- Read two little-endian bytes back as an `Int16`. -/
private def ofLe16 (b0 b1 : UInt8) : Int16 :=
  Int16.ofInt (Int.ofNat (b1.toNat * 256 + b0.toNat) - (if b1.toNat >= 128 then 65536 else 0))

/-- Byte-swap every pair and preserve an odd trailing byte.

The versions-2/3 sound containers hold only stored 8-bit PCM and 16-bit DPCM
output, neither of which has a byte order to get wrong, so stored 16-bit PCM was
copied through on the assumption that it was "already little-endian". That
assumption is false for the first game's 228 stored-16-bit resources, which a
big-endian writer produced.
-/
def swap16 (stored : ByteArray) : ByteArray := Id.run do
  let mut out := ByteArray.emptyWithCapacity stored.size
  for i in [0:stored.size / 2] do
    out := out.push stored[i * 2 + 1]!
    out := out.push stored[i * 2]!
  if stored.size % 2 != 0 then
    out := out.push stored[stored.size - 1]!
  return out

/-- The fraction of a payload's samples that sit on the rails, at `bits` depth.

`None` when the payload is too short to measure. The 16-bit path byte-swaps on
the way in, the same way the output path swaps them, so the reading is measured
in the order the samples are actually meant to be read.

The window is the middle `min (payload.size) (measureSamples * bytesPerSample)`
bytes, `start = (size - cap) / 2` with the subtraction truncating at zero, which
is `saturating_sub` in the Rust port.
-/
def railFraction (payload : ByteArray) (bits : Nat) : Option Float :=
  if bits >= 16 then
    let bytesPerSample := 2
    let cap := measureSamples * bytesPerSample
    let start := (payload.size - cap) / 2
    let stop := start + min payload.size cap
    let window := payload.extract start stop
    -- An odd window cannot hold a whole number of 16-bit samples.
    if window.size % 2 != 0 || window.size < 4 then
      none
    else Id.run do
      let mut rails : Nat := 0
      for i in [0:window.size / 2] do
        -- The stored pair is big-endian, so the first byte is the high one.
        let sample := ofLe16 window[i * 2 + 1]! window[i * 2]!
        let v := sample.toFloat / 32768.0
        if v.abs >= railLevel then
          rails := rails + 1
      return some (rails.toFloat / (window.size / 2).toFloat)
  else
    let bytesPerSample := 1
    let cap := measureSamples * bytesPerSample
    let start := (payload.size - cap) / 2
    let stop := start + min payload.size cap
    let window := payload.extract start stop
    if window.size < 2 then
      none
    else Id.run do
      let mut rails : Nat := 0
      for i in [0:window.size] do
        -- Rails are exactly the bytes 0, 1 and 255.
        let v := (window[i]!.toFloat - 128.0) / 128.0
        if v.abs >= railLevel then
          rails := rails + 1
      return some (rails.toFloat / window.size.toFloat)

/-- The depth the payload implies, when it disagrees with the declared one.

Returns the *other* depth only when the declared reading puts far more of the
payload on the rails than the other one. Returns `none` when the payload agrees
with the byte, when it is too short to say, or when the other depth would not
even fit.
-/
def impliedBits (payload : ByteArray) (declared : Nat) : Option Nat :=
  let other := if declared >= 16 then 8 else 16
  if other == 16 && payload.size % 2 != 0 then
    none
  else
    match railFraction payload declared, railFraction payload other with
    | some declaredRail, some otherRail =>
      if declaredRail - otherRail > railMargin then some other else none
    | _, _ => none

/-- Expand one stored byte per 16-bit sample, with a per-channel predictor.

A direct transliteration of `FUN_004c6b80` in `DMA4.exe`. Predictors are
independent per channel and are reset to zero at the start of the stream; the
addition wraps at 16 bits; the output is 2 * input bytes of signed 16-bit
little-endian samples, channels interleaved round-robin.
-/
def decodeDpcm (input : ByteArray) (channels : UInt16) : Except String ByteArray := do
  if channels == 0 then
    throw "differential PCM has zero channels"
  let channelCount := channels.toNat
  let mut previous : Array Int16 := Array.replicate channelCount 0
  let mut out := ByteArray.emptyWithCapacity (input.size * 2)
  let mut channel : Nat := 0
  for i in [0:input.size] do
    let code := input[i]!
    let mut sample := (dpcmTable.getD code.toNat 0)
    if (code.toNat &&& 0x7f) > 0x2f && code != dpcmSilence then
      -- Wrapping addition at 16 bits, the way the original's `add` does.
      sample := Int16.ofInt <| (sample + previous[channel]!).toInt
    previous := previous.set! channel sample
    out := out.append (le16 sample)
    channel := (channel + 1) % channelCount
  return out

/-- The intermediate WAV bytes the `flac` tool reads from stdin.

An intermediate WAV is the one format its encoder reads from stdin, and writing
it once here keeps the 8-bit unsigned convention -- the only place WAV and FLAC
differ -- in a single place. The data chunk is `decoded.pcm` byte for byte: the
encoder's own 8-bit path is unsigned, so the samples go through unchanged.
-/
def wavBytes (decoded : DecodedSound) : ByteArray := Id.run do
  let dataBytes := decoded.pcm.size
  let bytesPerSample := (decoded.format.bitsPerSample.toNat + 7) / 8
  let rate := decoded.format.sampleRate.toNat
  let byteRate := rate * bytesPerSample * decoded.format.channels.toNat
  let blockAlign := byteRate / rate
  let mut header := ByteArray.emptyWithCapacity 44
  for c in "RIFF" do
    header := header.push (c.toNat.toUInt8)
  header := header.append (le32 (UInt32.ofNat (dataBytes + 36)))
  for c in "WAVE" do
    header := header.push (c.toNat.toUInt8)
  for c in "fmt " do
    header := header.push (c.toNat.toUInt8)
  header := header.append (le32 16)              -- the fmt chunk's byte length
  header := header.append (le16 1)               -- WAVE_FORMAT_PCM
  header := header.append (le16 decoded.format.channels)
  header := header.append (le32 (UInt32.ofNat rate))
  header := header.append (le32 (UInt32.ofNat byteRate))
  header := header.append (le16 (UInt16.ofNat blockAlign))
  header := header.append (le16 decoded.format.bitsPerSample)
  for c in "data" do
    header := header.push (c.toNat.toUInt8)
  header := header.append (le32 (UInt32.ofNat dataBytes))
  return header.append decoded.pcm
where
  /-- A little-endian u32, least significant byte first: every header integer
  is little-endian even though the container is big-endian. -/
  le32 (v : UInt32) : ByteArray :=
    ByteArray.mk #[(v.toNat &&& 0xFF).toUInt8, ((v >>> 8).toNat &&& 0xFF).toUInt8,
      ((v >>> 16).toNat &&& 0xFF).toUInt8, ((v >>> 24).toNat &&& 0xFF).toUInt8]
  /-- A little-endian u16, least significant byte first. -/
  le16 (v : UInt16) : ByteArray :=
    ByteArray.mk #[(v.toNat &&& 0xFF).toUInt8, ((v >>> 8).toNat &&& 0xFF).toUInt8]

/-- One decoded sound resource, and the notes its decoding produced.

`decode` is a pure function, so the notes it would print on the way (a payload
that disagrees with its format byte) travel back as data for the extract loop to
print, in the same order the Rust port prints them.
-/
structure DecodeResult where
  sound : DecodedSound
  notes : Array String
  deriving Inhabited

/-- Decode one sound resource into samples.

The encoding is selected by the `flags` byte, the depth is cross-checked
against the payload, and the decoded sample count must break into whole frames.
-/
def decode (container : Container) (index : Nat) : Except String DecodeResult := do
  let stored ← container.payload index
  let flags := (container.flags[index]? |>.getD 0).toNat
  let attr := (container.attr[index]? |>.getD 0).toNat
  let format ← formatOf container index
  -- The codec needs the 16-bit flag; a container without it is rejected rather
  -- than guessed at.
  let missingFormatFlag : Except String Encoding :=
    throw s!"resource {index} uses the differential codec but its format byte 0x{MmfExtract.Bytes.hex attr} does not set the 16-bit flag"
  let encoding ←
    if flags &&& 3 == 0 then
      -- Stored PCM: the format byte's 16-bit flag decides the depth.
      pure (if (attr &&& 4) != 0 then Encoding.pcm16 else Encoding.pcm8)
    else if (attr &&& 4) == 0 then
      missingFormatFlag
    else
      pure Encoding.dpcm16
  -- The differential decoder expands one byte to one 16-bit sample, regardless
  -- of what the raw format byte says.
  let format :=
    if encoding == .dpcm16 then { format with bitsPerSample := 16 } else format
  let mut bits := format.bitsPerSample.toNat
  let mut notes : Array String := #[]

  -- The payload-driven depth override, only for the stored PCM encodings.
  if encoding == .pcm8 || encoding == .pcm16 then
    if bits >= 16 && stored.size % 2 != 0 then
      -- An odd payload cannot hold a whole number of 16-bit samples, so the
      -- payload decides. This is what `mpApp.SND` resource 119 needs.
      notes := notes.push s!"note: {MmfExtract.Path.str container.path}: resource {index} '{container.name index}': the format byte says 16-bit but the payload is {stored.size} bytes and cannot hold a whole number of 16-bit samples; using the payload"
      bits := 8
    match impliedBits stored bits with
    | some implied =>
      -- A quiet 16-bit signal puts almost all of its energy in its low byte, so
      -- whichever byte position is nearly always 00/ff is the high one.
      notes := notes.push s!"note: {MmfExtract.Path.str container.path}: resource {index} '{container.name index}': the format byte says {bits}-bit but the payload reads as {implied}-bit; using the payload"
      bits := implied
    | none => pure ()
  let format := if notes.isEmpty then format else { format with bitsPerSample := UInt16.ofNat bits }

  let expected ← container.plainLen index
  let pcm ← do
    match encoding with
    | .pcm8 | .pcm16 =>
      if expected != stored.size then
        throw s!"resource {index} is stored PCM but the size table says {expected} bytes and the payload has {stored.size}"
      let sampleBytes := bits / 8
      if stored.size % sampleBytes != 0 then
        throw s!"resource {index} has {stored.size} bytes, not a whole number of {bits}-bit samples"
      if bits >= 16 then pure (swap16 stored) else pure stored
    | .dpcm16 =>
      if expected != stored.size * 2 then
        throw s!"resource {index} is differential but the size table says {expected} bytes, while {stored.size} stored bytes expand to {stored.size * 2}"
      decodeDpcm stored format.channels

  let frameBytes := format.channels.toNat * (format.bitsPerSample.toNat / 8)
  if frameBytes == 0 || pcm.size % frameBytes != 0 then
    throw s!"resource {index}: {pcm.size} decoded bytes do not form whole {format.channels}x{format.bitsPerSample}-bit frames"

  return { sound := { format, encoding, pcm }, notes }

/-- Build a filename stem from the resource name, dropping the source audio
extension so the output extension is unambiguous. -/
def outputStem (container : Container) (index : Nat) : String := Id.run do
  let safe := Path.sanitize (container.name index) index
  let lowered := safe.toLower
  for extension in [".aif", ".aiff", ".wav"] do
    if lowered.endsWith extension && safe.length > extension.length then
      let chars := safe.toList
      return String.ofList (chars.take (chars.length - extension.length))
  return safe

/-- Whether the `flac` tool is runnable, probed once per extraction.

A missing encoder fails loudly and once, instead of once per resource, and never
falls back to WAV: silently changing the output format is exactly what this
program is not allowed to do.
-/
def flacAvailable : IO Bool := do
  let child ←
    IO.Process.spawn {
      cmd := flacTool
      args := #["--version"]
      stdin := .null
      stdout := .null
      stderr := .null
    }
  let exitCode ← child.wait
  pure (exitCode == 0)

/-- Write decoded PCM as a FLAC file, encoded by the `flac` tool.

The WAV bytes go in on stdin -- `flac -` auto-detects the format -- so nothing
touches disk twice.
-/
def writeFlac (path : System.FilePath) (decoded : DecodedSound) : IO Unit := do
  let child ←
    IO.Process.spawn {
      cmd := flacTool
      args := #["--silent", "--force", flacCompression, "--output-name", MmfExtract.Path.str path, "-"]
      stdin := .piped
      stdout := .null
      stderr := .piped
    }
  -- The bytes go in on the pipe; `putStr` would re-encode them as UTF-8 and
  -- change them, so the WAV is written through the byte-oriented writer.
  child.stdin.write (wavBytes decoded)
  child.stdin.flush
  let stderr ← child.stderr.readBinToEnd
  let exitCode ← child.wait
  if exitCode != 0 then
    -- The tool ends its complaint with a newline, which would tear the report's
    -- line protocol, so it is trimmed away.
    let complaint := MmfExtract.Bytes.decodeLossy stderr |>.trimAscii |>.toString
    throw (IO.userError s!"flac failed on {MmfExtract.Path.str path}: {complaint}")

/-- One sound written by [`extract`]. -/
structure ExtractedSound where
  index : Nat
  name : String
  path : System.FilePath
  bytes : Nat
  deriving Inhabited

/-- What happened while extracting one container's sounds. -/
structure ExtractReport where
  written : Nat := 0
  failed : Array (Nat × String × String) := #[]
  resources : Array ExtractedSound := #[]
  deriving Inhabited

/-- Extract every sound resource of a container.

The `flac` tool is probed only when a modern resource is about to be written,
so a container decoded entirely in `--original` mode never needs it; if it is
missing the extraction fails rather than writing WAV.
-/
def extract (container : Container) (outDir : System.FilePath) (original : Bool) :
    IO (Except String ExtractReport) := do
  IO.FS.createDirAll outDir
  let mut report : ExtractReport := {}
  let mut flac : Option Bool := none
  for index in [0:container.resourceCount.toNat] do
    let name := container.name index
    let decoded ←
      match decode container index with
      | .ok d =>
        -- The notes the decoder produced, printed before anything is written,
        -- exactly where the Rust port prints them.
        for note in d.notes do
          IO.eprintln note
        pure d.sound
      | .error e =>
        report := { report with failed := report.failed.push (index, name, e) }
        continue
    let stem := outputStem container index
    if !original then
      -- The probe is deferred until the first modern resource rather than run
      -- on every container, so a `--original` never needs the encoder.
      let available ←
        match flac with
        | some known => pure known
        | none => do
          let probed ← flacAvailable
          flac := some probed
          pure probed
      if !available then
        return .error "the 'flac' tool is required to write the modern output and was not found on PATH; install it (apt install flac / brew install flac), or use --original for the raw PCM"
    let extension := if original then "pcm" else "flac"
    let path := MmfExtract.Path.join outDir (Path.padIndex index ++ "_" ++ stem ++ "." ++ extension)
    let written ←
      if original then
        try
          IO.FS.writeBinFile path decoded.pcm
          pure (.ok ())
        catch e =>
          pure (.error s!"writing {MmfExtract.Path.str path}: {e}")
      else
        try
          writeFlac path decoded
          pure (.ok ())
        catch e =>
          pure (.error s!"writing {MmfExtract.Path.str path}: {e}")
    match (written : Except String Unit) with
    | .error e =>
      report := { report with failed := report.failed.push (index, name, e) }
    | .ok () =>
      report := { report with
        written := report.written + 1,
        resources := report.resources.push { index, name, path, bytes := decoded.pcm.size } }
  return .ok report

end MmfExtract.Sound
