import MmfExtract.Bytes
import MmfExtract.Container
import MmfExtract.Dma3d.Texfile
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
  deriving Inhabited

/-- Build a filename stem from the resource name, dropping the source extension
so the output extension is unambiguous. -/
def outputStem (container : Container) (index : Nat) : String :=
  Path.sanitize (container.name index) index

/-- The `--original` path: every payload written exactly as stored. -/
def extract (container : Container) (outDir : System.FilePath) (extension : String) :
    IO Extract3DReport := do
  IO.FS.createDirAll outDir
  let mut report : Extract3DReport := {}
  for index in [0:container.resourceCount.toNat] do
    let name := container.name index
    match Container.payload container index with
    | .error e =>
      report := { report with failed := report.failed.push (index, name, e) }
    | .ok payload =>
      let path := MmfExtract.Path.join outDir
        (Path.padIndex index ++ "_" ++ outputStem container index ++ "." ++ extension)
      IO.FS.writeBinFile path payload
      report := { report with
        written := report.written + 1,
        resources := report.resources.push { index, name, path, bytes := payload.size } }
  return report

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
          resources := report.resources.push
            { index, name, path, bytes := texture.width * texture.height * 4 } }
  return report

/-- The modern model path.

The Rust extractor writes a glTF per payload that carries the measured mesh
chain and the payload verbatim for the rest, with a `.tail` file and an
`.anim.txt` beside each glTF. That writer is not ported yet, so every model is
reported as a failure — the run continues, and the reason says precisely what is
missing, rather than a silent format change.
-/
def extractGltf (container : Container) (_outDir : System.FilePath) :
    IO Extract3DReport := do
  let mut report : Extract3DReport := {}
  let n := container.resourceCount.toNat
  for index in [0:n] do
    let name := container.name index
    report := { report with
      failed := report.failed.push (index, name,
        "the 3D model decode (glTF, mesh, animation records, texture coordinates) is not ported to Lean yet; use --original for the verbatim payload") }
  return report

/-- Extract a `Models` container: the glTF path when modern, the payload when
`--original`. -/
def extractModels (container : Container) (outDir : System.FilePath) (original : Bool) :
    IO Extract3DReport := do
  if original then
    return ← extract container outDir "mod"
  else
    return ← extractGltf container outDir

/-- Extract a `Textures` container: the PNG path when modern, the payload when
`--original`. -/
def extractTextures (container : Container) (outDir : System.FilePath) (original : Bool) :
    IO Extract3DReport := do
  if original then
    return ← extract container outDir "tex"
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
private def figure (dirPrefix : String) (name : String) (index : Nat) : String :=
  ("<figure><a href=\"" ++ dirPrefix ++ "/" ++ name
    ++ "\" title=\"Open " ++ Path.padIndex index
    ++ "\" ><img src=\"" ++ dirPrefix ++ "/" ++ name
    ++ "\" alt=\"" ++ Path.padIndex index
    ++ "\" loading=\"lazy\"></a><figcaption>" ++ Path.padIndex index
    ++ "</figcaption></figure>\n")

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
    -- The previews are listed for the same reason the Rust extractor lists
    -- them: so the page is built from what is on disk. A model without one
    -- still gets a figure, its image being absent rather than the model.
    let _previews ← listDir modelDir "preview.png"

    let texturePrefix := relativeTo base textureDir
    let modelPrefix := relativeTo base modelDir

    let mut page := indexHead
    page := page ++ s!"<h2>Objects <span class=\"count\">{models.size}</span></h2>\n"
    page := page ++
      "<p>Every object below is a rendered picture of the model itself, made from the model's own vertices. The file next to it is a <code>.gltf</code>, which is the model. <a href=\"#how\">How to open one</a>.</p>\n"
    page := page ++ "<div class=\"grid\">\n"
    for (index, name) in models.toList do
      -- A model without a preview still gets a figure; its image is simply
      -- missing rather than the model being left out.
      page := page ++ figure modelPrefix name index
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
