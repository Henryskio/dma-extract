import MmfExtract.Bytes
import MmfExtract.Container
import MmfExtract.Lz77
import MmfExtract.Sound
import MmfExtract.Dma3d.Texfile
import MmfExtract.Dma3d.Verify


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
    -- A payload whose index bytes are not a power of two must not be read as a
    -- palette image, factored into a width no texture in this engine can have.
    -- It may still be a coverage mask, so the negative belongs to the paletted
    -- reading and the router takes both (`docs/DMA3D.md` 11.14).
    let mut odd := ByteArray.emptyWithCapacity (4 * 4 + 63)
    for _entry in [0:4] do
      odd := odd.append (ByteArray.mk #[255, 0, 0, 255])
    for _ in [0:63] do
      odd := odd.push 1
    match (Dma3d.Texfile.parsePaletted odd : Except String Dma3d.Texfile.Texture) with
    | .ok _ => IO.eprintln "a 63-byte index area was accepted as a palette image"; ok := false
    | .error _ => pure ()
    -- And the mask reading of the same bytes is the 64-byte area behind a
    -- 15-byte header, which is what the router returns.
    match (Dma3d.Texfile.parse odd : Except String Dma3d.Texfile.Texture) with
    | .ok texture =>
      if texture.indices.size != 64 || !texture.palette.isEmpty then
        IO.eprintln s!"the mask reading of the odd payload is {texture.indices.size} bytes with {texture.palette.size} palette entries"
        ok := false
    | .error e => IO.eprintln s!"the mask reading of the odd payload was rejected: {e}"; ok := false
  return ok

/-- Whether `dir` can be read, so "nothing to check yet" can be told apart from
"the check failed".
-/
def dirExists (dir : String) : IO Bool := do
  let read := do
    let _ ← System.FilePath.readDir (System.FilePath.mk dir)
    pure true
  match ← (try read catch _ => pure false) with
  | b => pure b

/-- The Italian disc's `DMA3D.MMX` textures, decoded.

This is the check the carve and the palette rule exist for. The shipped
container was truncated to a third of its size and read at the header's stale
count, and the palette rule committed to the smallest valid split before asking
whether the area left over was a power of two. Together they failed 704 of the
705 resources. Both are fixed, and what that buys is measured here rather than
claimed: `docs/DMA3D.md` §11.

What the check holds each half to:

* the container is the size its own offset table says (4,019,622 bytes), and the
  count that table holds is 781, against the header's declared 705;
* the payload region is where the first offset says it is;
* the textures that decode are the ones whose length admits a split, and the
  ones that do not are reported rather than forced;
* every texture that decodes records whether the split was pinned, because most
  are not.
-/
def checkTextures3D (assetDir : String) : IO Bool := do
  let mut ok := true
  match ← Container.read (System.FilePath.mk (assetDir ++ "/DMA3D.MMX")) with
  | .error e => IO.eprintln s!"DMA3D.MMX: {e}"; return false
  | .ok container =>
    let n := container.resourceCount.toNat
    let declared := container.declaredCount.toNat
    let last := (container.offsets.getD n 0).toNat
    IO.println s!"DMA3D.MMX: {n} resources (the header declares {declared}), {container.data.size} bytes"
    if n != 705 then
      IO.eprintln s!"DMA3D.MMX: walked {n} resources, expected 705"
      ok := false
    if declared != 705 then
      IO.eprintln s!"DMA3D.MMX: declared {declared}, expected 705"
      ok := false
    if last != container.data.size then
      IO.eprintln s!"DMA3D.MMX: the last offset {last} is not the container's length {container.data.size}"
      ok := false
    -- The payload region begins where the first offset says, which is the one
    -- part of the layout that does not depend on the count dispute.
    let payloadStart := (container.offsets.getD 0 0).toNat
    if payloadStart != 29654 then
      IO.eprintln s!"DMA3D.MMX: the first payload starts at {payloadStart}, expected 29654"
      ok := false

    let mut decoded := 0
    let mut ambiguous := 0
    let mut outside := 0
    let mut noSplit := 0
    for i in [0:n] do
      match Container.payload container i with
      | .error _ => outside := outside + 1
      | .ok payload =>
        match parse payload with
        | .ok texture =>
          decoded := decoded + 1
          if texture.ambiguous then
            ambiguous := ambiguous + 1
        | .error _ => noSplit := noSplit + 1
    IO.println s!"DMA3D.MMX: {decoded} textures decoded ({ambiguous} of them with an unpinned palette split), {outside} offsets entries outside the container, {noSplit} with no split"
    if decoded != 705 then
      IO.eprintln s!"DMA3D.MMX: {decoded} textures decoded, expected 705"
      ok := false
    if ambiguous != 696 then
      -- Every one of them is a default: the constraint does not pin a unique
      -- split, so the smallest admissible palette was read and the ambiguity
      -- is on the record.
      IO.eprintln s!"DMA3D.MMX: {ambiguous} ambiguous, expected 696"
      ok := false
    if outside != 0 then
      IO.eprintln s!"DMA3D.MMX: {outside} offsets entries outside, expected 0"
      ok := false
    if noSplit != 0 then
      IO.eprintln s!"DMA3D.MMX: {noSplit} with no split, expected 0"
      ok := false
  return ok

/-- The 3D game's `--original` extraction, verified against the container.

The containers are carved out of the disc image by `test/carve3d.py`, so the
check takes both paths: the folder holding them, and the folder the extractor
wrote its output into. It is a *skip* rather than a failure when the output
folder is not there, because `run3d.sh` runs this check both before and after
the extraction it is checking.
-/
def checkOriginal3D (assetDir outDir : String) : IO Bool := do
  let mut ok := true
  -- `run3d.sh` runs this check before and after the `--original` extraction;
  -- before it there is no output to compare against, which is not a failure.
  if !(← dirExists outDir) then
    IO.eprintln s!"skipping: {outDir} does not exist; extract with --original first"
    return ok
  let entries ← System.FilePath.readDir (System.FilePath.mk assetDir)
  for entry in entries do
    let name := entry.fileName
    let stem :=
      match (name.splitOn ".").reverse with
      | _ :: rest => String.intercalate "." (rest.reverse)
      | [] => name
    let opened ← Container.read (System.FilePath.mk (Path.str entry.path))
    match opened with
    | .error e => IO.eprintln s!"{name}: {e}"; ok := false
    | .ok container =>
      if container.kind != ContainerKind.models && container.kind != ContainerKind.textures then
        continue
      let dir := System.FilePath.mk (outDir ++ "/" ++ (match container.kind with
        | ContainerKind.models => "Models"
        | _ => "Textures") ++ "/" ++ stem)
      let result ← Dma3d.Verify.checkOriginal container dir
      IO.println s!"{name}: {result.checked} files checked, {result.exact} byte-exact"
      if !result.mismatched.isEmpty then
        IO.eprintln s!"{name}: {result.mismatched.size} files differ from the container"
        ok := false
      -- Files that were not written: only the ones whose payload could not be
      -- read, because the extractor reported those instead of writing bytes
      -- the container does not hold.
      let unwritten := result.missing
      IO.println s!"{name}: {unwritten.size} resources have no file (their payloads are outside the container)"
  return ok

/-- The Rust converter's glTF, verified against the mesh contract in Lean.

The synthetic container under `synth/` is built by hand from the documented
chain (`u32 C`, `u16[C]`, `u32 V`, `vertex[V]`, `u32 V`, `normal[V]`), so the
conversion it drives has known geometry: 6 indices naming 4 vertices.
-/
def checkModern3D : IO Bool := do
  let read := fun (path : System.FilePath) => do
    try
      let data ← IO.FS.readBinFile path
      pure (some data)
    catch _ =>
      pure none
  match (← read (System.FilePath.mk "/tmp/dma3d-modern/Models/DMA3D/00000_syntheticTest.mod.gltf")) with
  | none =>
    -- Not present is not a failure: `run3d.sh` runs this check before and after
    -- the conversion, and before it there is nothing to verify. Reporting it
    -- separately is what lets the whole harness be run at either point.
    IO.eprintln "skipping: no modern output; extract on the synthetic 3D container first"
    return true
  | some bytes =>
    let text := MmfExtract.Bytes.decodeLossy bytes
    match (Dma3d.Verify.checkGltf text : Except String Nat) with
    | .error e => IO.eprintln s!"the glTF was rejected: {e}"; return false
    | .ok indexCount =>
      IO.println s!"the Rust converter's glTF verified in Lean: {indexCount} indices, all naming a vertex"
      -- A tampered index must be rejected, or the check proves nothing: the
      -- second `u16` of the index buffer becomes a vertex the count lacks.
      let bad := text.replace "AAABAAIAAw" "AAABAAkAAw"
      match (Dma3d.Verify.checkGltf bad : Except String Nat) with
      | .error _ => IO.println "the same check rejects a tampered index"
      | .ok _ => IO.eprintln "the check accepted a tampered index"; return false
      -- So must a byte length that does not match the decoded buffer.
      let badLen := text.replace "108" "96"
      match (Dma3d.Verify.checkGltf badLen : Except String Nat) with
      | .error _ => IO.println "the same check rejects a wrong byte length"
      | .ok _ => IO.eprintln "the check accepted a wrong byte length"; return false
      return true


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
  if !(← checkOriginal3D "/workspace/.tmp/dma3d/assets" "/tmp/dma3d-original") then ok := false
  if !(← checkTextures3D "/workspace/.tmp/dma3d/assets") then ok := false
  if !(← checkModern3D) then ok := false
  if !(← checkContainers "/RawAssets") then ok := false
  if ok then IO.println "all checks passed"; return 0 else return 1
