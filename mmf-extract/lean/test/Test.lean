import MmfExtract.Bytes
import MmfExtract.Container
import MmfExtract.Lz77
import MmfExtract.Sound
import MmfExtract.Dma3d.Texfile


/-!
Checks run against the ported Lean decoders.

Run with:

```sh
cd mmf-extract/lean
lake build
LEAN_PATH=.lake/build/lib/lean lean --run test/Test.lean
```
-/

open MmfExtract
open MmfExtract.Dma3d.Texfile
open MmfExtract.Container

/-- A payload long enough to need the extended literal-run form. -/
def longPayload : ByteArray := Id.run do
  let mut a := ByteArray.emptyWithCapacity 1000
  for i in [0:1000] do
    a := a.push (UInt8.ofNat (i % 251))
  return a

def checkLz77RoundTrip : IO Bool := do
  let mut ok := true
  -- The same vectors `tests/roundtrip.rs` uses for the Rust compressor: the
  -- encoder is an exact inverse of the decoder, and the Lean side proves it
  -- with its own round trip.
  let vectors : List ByteArray := [
    ByteArray.mk #[],
    ByteArray.mk #[0],
    ByteArray.mk #[1, 2, 3],
    ByteArray.mk #[0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB,
      0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB,
      0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB,
      0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB,
      0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB,
      0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB,
      0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB,
      0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB,
      0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB,
      0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB,
      0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB,
      0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB,
      0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB,
      0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB,
      0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB,
      0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB,
      0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB,
      0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB,
      0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB,
      0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB,
      0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB,
      0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB,
      0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB,
      0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB,
      0xAB, 0xAB, 0xAB],
    (List.range 300 |>.map (fun i => (i % 7).toUInt8)).toByteArray,
    (List.range 1000 |>.map (fun i => (i % 251).toUInt8)).toByteArray,
    (List.range 5000 |>.map (fun i => (i % 251).toUInt8)).toByteArray,
    longPayload,
    (longPayload.extract 0 600).append longPayload]
  for payload in vectors do
    match Lz77.decompress (Lz77.compress payload) with
    | .error e => IO.eprintln s!"lz77 round-trip failed for {payload.size} bytes: {e}"; ok := false
    | .ok out =>
    if out != payload then
      IO.eprintln s!"lz77 round-trip mismatch for {payload.size} bytes"; ok := false
  return ok

/-- The differential decoder's three vectors, from `src/sound.rs`'s own unit
tests: the silence code produces zero samples, and the predictor is independent
per channel. -/
def checkDpcm : IO Bool := do
  let mut ok := true
  let wanted : List ByteArray := [
    ByteArray.mk #[0, 0, 0, 0, 0, 0, 0, 0],                          -- the silence code
    ByteArray.mk #[1, 0, 1, 8, 1, 16, 0, 0],                         -- 1, 2049, 4097, 0
    ByteArray.mk #[1, 0, 1, 0, 1, 8, 1, 8]]                          -- 1, 1, 2049, 2049
  let inputs : List (ByteArray × UInt16) := [
    (ByteArray.mk #[0xF8, 0xF8, 0xF8, 0xF8], 1),
    (ByteArray.mk #[0, 0x30, 0x30, 0xF8], 1),
    (ByteArray.mk #[0, 0, 0x30, 0x30], 2)]
  for ((input, channels), want) in inputs.zip wanted do
    match Sound.decodeDpcm input channels with
    | .error e => IO.eprintln s!"dpcm failed: {e}"; ok := false
    | .ok got =>
      if got != want then
        IO.eprintln s!"dpcm produced {got}, expected {want}"
        ok := false
  -- A zero-channel stream is rejected rather than dividing by zero.
  match Sound.decodeDpcm ByteArray.empty 0 with
  | .ok _ => IO.eprintln "a zero-channel stream was accepted"; ok := false
  | .error _ => pure ()
  return ok

/-- The differential codec, exercised on a real shipped resource.

`HD.MMA` and `app.MMA` both hold resources whose flags byte is `9` — the
differential codec — so the path is not a synthetic-only one; the parity test
compares the resulting FLACs against the Rust reference byte for byte.
-/
def checkDifferentialSounds : IO Bool := do
  let mut ok := true
  let mut found := 0
  for name in ["/RawAssets/app.MMA", "/RawAssets/HD.MMA"] do
    let opened ← Container.read (System.FilePath.mk name)
    match opened with
    | .error e => IO.eprintln s!"{name}: {e}"; ok := false
    | .ok container =>
      for i in [0:container.resourceCount.toNat] do
        -- `flags & 3 != 0` is the differential codec (`docs/SOUNDS.md` §2).
        let flags := container.flags[i]?.getD 0 |>.toNat
        if flags &&& 3 != 0 then
          found := found + 1
          match Sound.decode container i with
          | .error e =>
            IO.eprintln s!"{name} resource {i}: {e}"
            ok := false
          | .ok result =>
            let decoded := result.sound
            -- One stored byte expands to one 16-bit sample, so the output is
            -- exactly twice the payload, in whole frames of one channel.
            let stored := (Container.payload container i).toOption.getD ByteArray.empty
            if decoded.pcm.size != stored.size * 2 then
              IO.eprintln s!"{name} resource {i}: {stored.size} stored bytes expanded to {decoded.pcm.size}"
              ok := false
            if decoded.encoding != Sound.Encoding.dpcm16 then
              IO.eprintln s!"{name} resource {i} is not the differential encoding"
              ok := false
            if decoded.format.bitsPerSample != 16 then
              IO.eprintln s!"{name} resource {i}: differential output is not 16-bit"
              ok := false
  if found == 0 then
    IO.eprintln "no differential sound was found in the shipped containers"
    ok := false
  else
    IO.println s!"{found} differential sounds decoded"
  return ok

/-- A synthetic texture: a four-entry palette and 64 index bytes, which is a
perfect square at every candidate width.

The real `Textures` container ships inside `Disney Creazioni in 3D (Italy).zip`,
so this exercises the measured decode (`docs/DMA3D.md` §2.1) without them.
-/
def syntheticTexture : ByteArray := Id.run do
  let mut payload := ByteArray.emptyWithCapacity (4 * 4 + 64)
  for entry in [0:4] do
    payload := payload.append (ByteArray.mk #[
      if entry == 0 then 255 else 0, 0, UInt8.ofNat (entry * 60), 255])
  -- A 8x8 square of four colours, so the area is a power of two and the shape
  -- statistics have something to work with.
  for i in [0:64] do
    payload := payload.push (UInt8.ofNat ((i / 8 + i % 8) % 4))
  return payload

def checkTextures : IO Bool := do
  let mut ok := true
  match (Dma3d.Texfile.parse syntheticTexture : Except String Dma3d.Texfile.Texture) with
  | .error e => IO.eprintln s!"texture decode failed: {e}"; ok := false
  | .ok texture =>
    if texture.width != 8 || texture.height != 8 then
      IO.eprintln s!"texture is {texture.width}x{texture.height}, expected 8x8"
      ok := false
    if texture.axis != Dma3d.Texfile.TextureAxis.square then
      IO.eprintln s!"texture axis is {reprStr texture.axis}, expected square"
      ok := false
    -- A payload whose index bytes are not a power of two must be reported, not
    -- factored into a width no texture in this engine can have.
    let mut odd := ByteArray.emptyWithCapacity (4 * 4 + 63)
    for _entry in [0:4] do
      odd := odd.append (ByteArray.mk #[255, 0, 0, 255])
    for _ in [0:63] do
      odd := odd.push 1
    match (Dma3d.Texfile.parse odd : Except String Dma3d.Texfile.Texture) with
    | .ok _ => IO.eprintln "a 63-byte index area was accepted"; ok := false
    | .error _ => pure ()
  return ok

def checkContainers (dir : String) : IO Bool := do
  let d ← System.FilePath.readDir (System.FilePath.mk dir)
  let mut ok := true
  for entry in d do
    let path := System.FilePath.mk (Path.str entry.root ++ "/" ++ entry.fileName)
    let c : Except String Container ← Container.read path
    match c with
    | .error _ => pure ()  -- not one of ours: skipped, exactly as the driver does
    | .ok cont =>
      IO.println s!"{MmfExtract.Path.str path}: kind={reprStr cont.kind} version={cont.version} \
        N={cont.resourceCount} payloads=[{cont.offsets[0]?.getD 0}, {cont.data.size}]\
        first={if cont.names.size > 0 then cont.names[cont.names.size - 1]! else "\"\""}"
  return ok

def main (_args : List String) : IO UInt32 := do
  let mut ok := true
  if !(← checkLz77RoundTrip) then ok := false
  if !(← checkTextures) then ok := false
  if !(← checkDpcm) then ok := false
  if !(← checkDifferentialSounds) then ok := false
  if !(← checkContainers "/RawAssets") then ok := false
  if ok then IO.println "all checks passed"; return 0 else return 1
