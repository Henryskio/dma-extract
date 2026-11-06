import MmfExtract.Bytes
import MmfExtract.Container
import MmfExtract.Dma3d.Texfile
import MmfExtract.Dma3d.Modfile
import MmfExtract.Path
import MmfExtract.Picture
import MmfExtract.Png

/-!
The 3D game's `Models` and `Textures` containers.

A port of `src/dma3d.rs` and `src/dma3d/texfile.rs`. Two output modes:

* `--original` writes every payload byte for byte — lossless, and byte-exact
  against the container.
* the modern texture path decodes each payload as the measured indexed image
  (`docs/DMA3D.md` §2.1) and writes it as a PNG; a texture that does not fit the
  layout is reported as a failure rather than written as something it is not.
* the modern model path writes a glTF per mesh-carrying payload in the Rust
  extractor. That writer is **not ported yet**, so the Lean core reports each
  model as a failure instead of silently writing a different format: writing the
  payload through under a `.gltf` name, or dropping the animation records, is
  exactly the guess this project forbids.
-/

namespace MmfExtract.Dma3d

open MmfExtract.Container (Container)
open MmfExtract.Dma3d.Modfile (Profile)

/-- One file the 3D output produced, as the index page lists it. -/
structure Extracted3D where
  index : Nat
  name : String
  path : System.FilePath
  bytes : Nat
  deriving Inhabited

/-- What happened while extracting one 3D container. -/
structure Extract3DReport where
  written : Nat := 0
  failed : Array (Nat × String × String) := #[]
  resources : Array Extracted3D := #[]
  /-- Textures written whose palette/index split the payload does not pin down,
  so the palette that was read is the smallest admissible one rather than a
  measurement. They are counted rather than failed: the pixels are right and only
  the count of unused palette entries is undetermined (`docs/DMA3D.md` §11). -/
  ambiguous : Nat := 0
  /-- Payloads written as a glTF line loop, i.e. drawing profiles rather than
  meshes (`docs/DMA3D.md` §11.25, §11.26). -/
  profiles : Nat := 0
  /-- Payloads written as JSON+text, i.e. `mod II` record streams whose record
  schema the bytes do not determine (`docs/DMA3D.md` §11.38). -/
  records : Nat := 0
  deriving Inhabited

/-- The output file name's stem: the resource name, sanitised.

The name already ends in the game's own extension (`createAmbientLight.mod`,
`dialogBtOk.TGA`), so `--original` writes the file under it verbatim — spelling
the format's extension as well produced `00033_od.mod.mod`, and stripping the
resource's produced `00033_od`. The modern paths append the extension of the
format they write, which is why they are the only ones that choose one.
-/
def outputStem (container : Container) (index : Nat) : String :=
  Path.sanitize (container.name index) index

/-- The `--original` path: every payload written exactly as stored.

The file keeps the name the container gives it, extension included, so nothing
is chosen and nothing is doubled.
-/
def extract (container : Container) (outDir : System.FilePath) : IO Extract3DReport := do
  IO.FS.createDirAll outDir
  let mut report : Extract3DReport := {}
  for index in [0:container.resourceCount.toNat] do
    let name := container.name index
    match Container.payload container index with
    | .error e =>
      report := { report with failed := report.failed.push (index, name, e) }
    | .ok payload =>
      -- The payload keeps the name the container gives it, spelled once.
      let path := MmfExtract.Path.join outDir
        (Path.padIndex index ++ "_" ++ outputStem container index)
      IO.FS.writeBinFile path payload
      report := { report with
        written := report.written + 1,
        resources := report.resources.push { index, name, path, bytes := payload.size } }
  return report

/-! ## The profile's glTF line loop -/

/-- The little-endian `f32` bytes of one float. -/
private def leF32 (f : Float) : ByteArray := Id.run do
  let u := f.toBits.toUInt32
  let mut out := ByteArray.emptyWithCapacity 4
  out := out.push (UInt8.ofNat (u &&& 0xff).toNat)
  out := out.push (UInt8.ofNat ((u >>> 8) &&& 0xff).toNat)
  out := out.push (UInt8.ofNat ((u >>> 16) &&& 0xff).toNat)
  out := out.push (UInt8.ofNat ((u >>> 24) &&& 0xff).toNat)
  return out

/-- The binary buffer of a profile's glTF: `0 … n-1, 0` as `u32`, padded, then the
positions as `f32`.

The loop closes, because a profile is a closed outline the game tints and extrudes,
so the last edge back to the first point is part of the shape.
-/
def profileBuffer (profile : Profile) : ByteArray := Id.run do
  let n := profile.points.size
  let mut out := ByteArray.emptyWithCapacity (4 * (n + 1) + 12 * n)
  for i in [0:n] do
    let u : UInt32 := i.toUInt32
    out := out.append (leU32 u)
  out := out.append (leU32 0)
  while out.size % 4 != 0 do
    out := out.push 0
  for (x, y, z) in profile.points do
    out := out.append (leF32 x)
    out := out.append (leF32 y)
    out := out.append (leF32 z)
  while out.size % 4 != 0 do
    out := out.push 0
  return out
where
  /-- A `u32` as four little-endian bytes. -/
  leU32 (u : UInt32) : ByteArray := Id.run do
    let mut out := ByteArray.emptyWithCapacity 4
    out := out.push (UInt8.ofNat (u &&& 0xff).toNat)
    out := out.push (UInt8.ofNat ((u >>> 8) &&& 0xff).toNat)
    out := out.push (UInt8.ofNat ((u >>> 16) &&& 0xff).toNat)
    out := out.push (UInt8.ofNat ((u >>> 24) &&& 0xff).toNat)
    return out

/-- The glTF 2.0 document of a profile, as `src/dma3d/gltf.rs::write_profile_gltf`
spells it: one primitive of mode `2` (`LINES`) whose indices close the loop.

The engine's profile-to-surface tessellation is not in the payload, so the document
is a line loop and never a surface — emitting one would be invented geometry
(`docs/DMA3D.md` §11.26).
-/
def profileGltf (name bufferName : String) (profile : Profile) : String := Id.run do
  let n := profile.points.size
  let binName := bufferName
  let mut min : Array Float := #[Float.inf]
  let mut max : Array Float := #[-Float.inf]
  for (x, y, z) in profile.points do
    for v in [x, y, z] do
      if v < min.back! then min := min.set! (min.size - 1) v
      if v > max.back! then max := max.set! (max.size - 1) v
  let document :=
    "{\"asset\":{\"version\":\"2.0\",\"generator\":\"mmf-extract (DMA3D §11.25)\"},\n" ++
    "\"scene\": 0,\n" ++
    "\"scenes\": [{\"nodes\": [0]}],\n" ++
    "\"nodes\": [{\"name\": \"" ++ escapeJson name ++ "\", \"mesh\": 0}],\n" ++
    "\"meshes\": [{\"name\": \"" ++ escapeJson name ++ "\", \"primitives\": [" ++
      "{\"attributes\": {\"POSITION\": 1}, \"indices\": 0, \"mode\": 2}]}],\n" ++
    "\"accessors\": [" ++
      "{\"bufferView\": 0, \"componentType\": 5125, \"count\": " ++ toString (n + 1) ++
        ", \"type\": \"SCALAR\"}," ++
      "{\"bufferView\": 1, \"componentType\": 5126, \"count\": " ++ toString n ++
        ", \"type\": \"VEC3\", \"min\": [" ++ joinFloats min ++
        "], \"max\": [" ++ joinFloats max ++ "]}],\n" ++
    "\"bufferViews\": [" ++
      "{\"buffer\": 0, \"byteOffset\": 0, \"byteLength\": " ++ toString (4 * (n + 1)) ++
        ", \"target\": 34963}," ++
      "{\"buffer\": 0, \"byteOffset\": " ++ toString (4 * (n + 1)) ++
        ", \"byteLength\": " ++ toString (12 * n) ++ ", \"target\": 34962}],\n" ++
    "\"buffers\": [{\"byteLength\": " ++ toString (4 * (n + 1) + 12 * n) ++
      ", \"uri\": \"" ++ escapeJson binName ++ "\"}]}\n"
  return document
where
  joinFloats (xs : Array Float) : String :=
    String.intercalate ", " (xs.toList.map toString)
  escapeJson (s : String) : String := Id.run do
    let mut out := ""
    for c in s.toList do
      if c == '"' || c == '\\' then
        out := out.push '\\'
      out := out.push c
    return out

/-- The modern texture path: decode each payload and write a PNG. -/
def extractPngs (container : Container) (outDir : System.FilePath) : IO Extract3DReport := do
  IO.FS.createDirAll outDir
  let mut report : Extract3DReport := {}
  for index in [0:container.resourceCount.toNat] do
    let name := container.name index
    match Container.payload container index with
    | .error e =>
      report := { report with failed := report.failed.push (index, name, e) }
    | .ok payload =>
      let path := MmfExtract.Path.join outDir
        (Path.padIndex index ++ "_" ++ outputStem container index ++ ".png")
      match Texfile.parse payload with
      | .error e =>
        report := { report with failed := report.failed.push (index, name, s!"{e}") }
      | .ok texture =>
        let rgba := texture.toRgba
        IO.FS.writeBinFile path
          (Png.ofRgba texture.width texture.height (rgba.extract 0 (texture.width * texture.height * 4)))
        report := { report with
          written := report.written + 1,
          ambiguous := report.ambiguous + (if texture.ambiguous then 1 else 0),
          resources := report.resources.push
            { index, name, path, bytes := texture.width * texture.height * 4 } }
  return report

/-- Write a `mod II` record stream as a JSON+text document.

`docs/DMA3D.md` §11.38: 48 of the shipped payloads carry the magic and the `u32`
after it and are read by neither the mesh reader nor the profile reader. Their
record schema is **not derivable from the bytes** — §11.32 measured that no position
in the stream holds a constant across payloads — so this records what is measured
and states what is not, rather than asserting a meaning it does not have.

JSON+text is the same modern format the scripts decoder emits, because these are
data records rather than geometry: writing them as a glTF would invent a shape the
payload does not carry, which `AGENTS.md` forbids.
-/
def writeRecordJson (path : System.FilePath) (name : String) (r : Modfile.MagicRecord) : IO Unit := do
  let pairs := String.intercalate ", " (r.pairs.toList.map toString)
  let floats := String.intercalate ", " (r.floats.toList.map toString)
  let document :=
    "{\n" ++
    s!"  \"name\": {jsonStr name},\n" ++
    "  \"what this is\": \"a mod II record stream the mesh reader and the drawing-profile reader both reject\",\n" ++
    "  \"meaning\": \"not determined: no position in the record stream holds a constant across payloads \
     (docs/DMA3D.md 11.32), so the record schema is not read from the bytes\",\n" ++
    "  \"measured\": {\n" ++
    s!"    \"magic offset\": {r.magicOffset},\n" ++
    s!"    \"record type\": {r.recordType},\n" ++
    s!"    \"stream offset\": {r.magicOffset + 10},\n" ++
    s!"    \"pairs\": [{pairs}],\n" ++
    s!"    \"floats that read as numbers\": [{floats}]\n" ++
    "  }\n" ++
    "}\n"
  IO.FS.writeBinFile path document.toUTF8
where
  /-- A JSON string literal: the quotes, and a backslash or quote escaped. -/
  jsonStr (s : String) : String := Id.run do
    let mut out := "\""
    for c in s.toList do
      if c == '"' || c == '\\' then
        out := out.push '\\'
      out := out.push c
    return out ++ "\""

/-- The modern model path.

The **Rust converter** is the writer of modern models in this pipeline
(`src/cli.rs::convert_models`), so this does not write: it reads each payload with
`Modfile.parse` and reports the ones that fit neither the mesh chain nor the
drawing profile, which is the same set the converter reports
(`docs/DMA3D.md` §11.25, §11.26).

The reading is ported so the two halves cannot drift: the Lean core's count of
payloads carrying a `f32` point stream is checked against the Rust reader's in
`test/Test.lean`, and a payload both accept is the one the converter writes. A
payload that fits neither is reported with the reader's own reason, so the run
says what is missing rather than silently skipping it.
-/
def extractGltf (container : Container) (outDir : System.FilePath) :
    IO Extract3DReport := do
  IO.FS.createDirAll outDir
  let mut report : Extract3DReport := {}
  let n := container.resourceCount.toNat
  for index in [0:n] do
    let name := container.name index
    match Container.payload container index with
    | Except.error e =>
      report := { report with failed := report.failed.push (index, name, e) }
    | Except.ok payload =>
      match Modfile.parse payload with
      | Except.ok _ =>
        -- A drawing profile, which the Rust converter writes as a glTF line
        -- loop. Counted here so the Lean core's own accounting agrees with what
        -- the converter writes, and not written, because writing it here as well
        -- would produce the same file twice under the same name.
        -- Not counted in `written`: this path writes no file, and the Rust
        -- converter's count is the one `src/cli.rs` adds. Counting it here as
        -- well would report the same file twice.
        report := { report with profiles := report.profiles + 1 }
      | Except.error e =>
        -- Neither a mesh chain nor a point stream. It may still carry a `mod II`
        -- record stream, which is written as JSON+text: the shape is measured
        -- (§11.38) and the record schema is not, so the document says so rather
        -- than asserting a meaning the bytes do not carry.
        match Modfile.parseMagicRecord payload with
        | Except.error recordErr =>
          report := { report with
            failed := report.failed.push (index, name,
              s!"{e}; the record reading also fails ({recordErr})") }
        | Except.ok _record =>
          -- Counted in `records` but **not** written, and not in `written`
          -- either: the Rust converter is the writer of modern models in this
          -- pipeline (`src/cli.rs::convert_models`), and a file written by both
          -- halves under the same name would be counted twice. That is the same
          -- rule the profile reading already follows.
          report := { report with records := report.records + 1 }
  return report

/-- Extract a `Models` container: the glTF path when modern, the payload when
`--original`. -/
def extractModels (container : Container) (outDir : System.FilePath) (original : Bool) :
    IO Extract3DReport := do
  if original then
    return ← extract container outDir
  else
    return ← extractGltf container outDir

/-- Extract a `Textures` container: the PNG path when modern, the payload when
`--original`. -/
def extractTextures (container : Container) (outDir : System.FilePath) (original : Bool) :
    IO Extract3DReport := do
  if original then
    return ← extract container outDir
  else
    return ← extractPngs container outDir

-------------------------------------------------------------------------------
-- The index page
-------------------------------------------------------------------------------

/-- The page's head: no script, no stylesheet, no network. -/
def indexHead : String :=
  "<!DOCTYPE html>\n" ++
  "<html lang=\"en\">\n" ++
  "<meta charset=\"utf-8\">\n" ++
  "<title>3D assets</title>\n" ++
  "<style>\n" ++
  " body { font: 15px/1.5 system-ui, sans-serif; margin: 0 auto; max-width: 76rem; padding: 2rem 1rem; background: #16181d; color: #e6e6e6; }\n" ++
  " h1 { font-size: 1.6rem; margin: 0 0 .25rem; }\n" ++
  " h2 { font-size: 1.2rem; margin: 2.5rem 0 .25rem; border-top: 1px solid #2c3038; padding-top: 1.5rem; }\n" ++
  " .count { color: #8b93a3; font-weight: normal; font-size: .9rem; }\n" ++
  " .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(96px, 1fr)); gap: .75rem; margin: 1rem 0; }\n" ++
  " figure { margin: 0; }\n" ++
  " img { width: 100%; height: auto; background: #0b0d10; border: 1px solid #2c3038; border-radius: 4px; display: block; }\n" ++
  " figcaption { font-size: .7rem; color: #8b93a3; text-align: center; margin-top: .2rem; }\n" ++
  " #how { background: #1b1f26; border: 1px solid #2c3038; border-radius: 6px; padding: 1rem 1.25rem; margin: 2rem 0 0; }\n" ++
  " #how h2 { border: 0; margin: 0 0 .5rem; padding: 0; font-size: 1rem; }\n" ++
  " #how code { background: #0b0d10; padding: .1rem .3rem; border-radius: 3px; }\n" ++
  "</style>\n" ++
  "<h1>3D assets</h1>\n" ++
  "<p>Pictures of everything this game shipped, recovered from its own containers.</p>\n"

/-- The page's tail: what a reader does with a glTF they have just been shown. -/
def indexTail : String :=
  "\n<div id=\"how\">\n" ++
  "<h2>How to open a <code>.gltf</code></h2>\n" ++
  "<ul>\n" ++
  " <li><b>In a browser, no install:</b> drag the file onto\n" ++
  "     <code>https://gltf-viewer.donmccurdy.com</code>. Nothing is uploaded.</li>\n" ++
  " <li><b>In Blender:</b> <code>File &rarr; Import &rarr; glTF 2.0</code>.</li>\n" ++
  " <li><b>On Windows 10/11:</b> open it in the <i>3D Viewer</i> app.</li>\n" ++
  " <li>Each <code>.gltf</code> has a <code>.tail</code> file beside it holding the\n" ++
  "     bytes that are still undecoded (the animation and the texture\n" ++
  "     coordinates), so nothing is thrown away.</li>\n" ++
  "</ul>\n" ++
  "</div>\n"

/-- List the files of `dir` whose names end with `extension`, returning the
index and the full file name.

The output names are `<index:05>_<rest>.<ext>`, so the leading digits are the
container's resource index and the sort order is the game's.

The extension is the filter because the page's figures are `<img>` tags: only a
file a browser can draw belongs on it. Listing every numbered file instead put
the `--original` payloads — a `.mod` is 460 bytes of the game's own binary, not
an image — into that markup, so the page showed a grid of broken pictures.
-/
def listDir (dir : System.FilePath) (extension : String) : IO (Array (Nat × String)) := do
  let mut out : Array (Nat × String) := #[]
  let entries ← System.FilePath.readDir dir
  for entry in entries do
    let name := entry.fileName
    if !name.endsWith extension then
      continue
    let digits := String.ofList (name.toList.takeWhile (·.isDigit))
    let index ← match digits.toNat? with
      | some value => pure value
      | none => pure 18446744073709551615
    out := out.push (index, name)
  return out.qsort (·.1 ≤ ·.1)

/-- One `<figure>` of the index page. -/
private def figureLink (dirPrefix : String) (linkName imageName : String) (index : Nat) : String :=
  ("<figure><a href=\"" ++ dirPrefix ++ "/" ++ linkName
    ++ "\" title=\"Open " ++ Path.padIndex index
    ++ "\" ><img src=\"" ++ dirPrefix ++ "/" ++ imageName
    ++ "\" alt=\"" ++ Path.padIndex index
    ++ "\" loading=\"lazy\"></a><figcaption>" ++ Path.padIndex index
    ++ "</figcaption></figure>\n")

/-- One figure whose link and image are the same path. -/
private def figure (dirPrefix : String) (name : String) (index : Nat) : String :=
  figureLink dirPrefix name name index

/-- The index page, built from what was written rather than from what the
container claims it holds. It goes at `base`, beside both directories, and
refers to them by a relative path.

It is a single dependency-free file — no script, no stylesheet, no network — so
it works by double-clicking it from a local folder.
-/
def writeIndex (base : System.FilePath) (textureDir modelDir : System.FilePath) :
    IO (Except String Nat) := do
  try
    let textures ← listDir textureDir "png"
    let models ← listDir modelDir "gltf"
    let previews ← listDir modelDir "preview.png"

    let texturePrefix := relativeTo base textureDir
    let modelPrefix := relativeTo base modelDir

    let mut page := indexHead
    page := page ++ s!"<h2>Objects <span class=\"count\">{models.size}</span></h2>\n"
    page := page ++
      "<p>Every object below is a rendered picture of the model itself, made from the model's own vertices. The file next to it is a <code>.gltf</code>, which is the model. <a href=\"#how\">How to open one</a>.</p>\n"
    page := page ++ "<div class=\"grid\">\n"
    -- The preview beside each glTF, so the figure shows the object and links
    -- it. A `.gltf` is not an image, and the page used the same path for both
    -- the link and the `<img>`, so every object rendered as a broken picture
    -- and the previews the converter writes were never shown.
    for (index, name) in models.toList do
      let image :=
        match previews.find? (fun p => p.1 == index) with
        | some (_, preview) => preview
        | none => name
      page := page ++ figureLink modelPrefix name image index
    page := page ++ "</div>\n"

    page := page ++ s!"<h2>Textures <span class=\"count\">{textures.size}</span></h2>\n"
    page := page ++
      "<p>The game stores each texture as a palette and a grid of one-byte indices. They are expanded to ordinary PNGs here.</p>\n"
    page := page ++ "<div class=\"grid\">\n"
    for (index, name) in textures.toList do
      page := page ++ figure texturePrefix name index
    page := page ++ "</div>\n"
    page := page ++ indexTail

    let path := MmfExtract.Path.join base "index.html"
    IO.FS.writeBinFile path (page.toUTF8)
    return .ok (models.size + textures.size)
  catch e =>
    return .error s!"writing the 3D index page: {e}"
where
  /-- `dir` as a path relative to `base`, spelled with forward slashes so the
  page works wherever it is opened from. -/
  relativeTo (base dir : System.FilePath) : String :=
    let baseText := MmfExtract.Path.str base
    let dirText := MmfExtract.Path.str dir
    if dirText.startsWith (baseText ++ "/") then
      (dirText.drop (baseText.length + 1)).toString
    else
      dirText

end MmfExtract.Dma3d
