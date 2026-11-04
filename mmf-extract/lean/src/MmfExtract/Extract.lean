import MmfExtract.Container
import MmfExtract.Path
import MmfExtract.Script
import MmfExtract.Sound
import MmfExtract.Picture
import MmfExtract.V1
import MmfExtract.Film
import MmfExtract.Dma3d
import MmfExtract.V1.Picture
import MmfExtract.Types

/-!
The extraction driver.

This is the Lean half of `src/cli.rs::run`: it walks the asset folder, decides
which container is which, and writes the modern (`--original` absent) or
original output. The Rust CLI stays the driver of both halves -- it parses the
command line, asks this driver for a run and prints the report -- so the
argument handling, the summary text and the exit behaviour live in one place.

Failures are per-resource, never fatal: each one is reported and the rest of the
run continues, which is the rule the Rust driver follows too.
-/

namespace MmfExtract.Extract

open MmfExtract.Container ContainerKind

/-- Which asset types to extract.

`"all"` selects everything; unknown names are an error, so a typo does not
silently extract nothing.
-/
structure Selection where
  pictures : Bool
  sounds : Bool
  films : Bool
  scripts : Bool
  /-- `Blobs` containers -- the first game's animation records. -/
  blobs : Bool
  /-- The 3D game's `Models` and `Textures` containers. -/
  models : Bool
  deriving Inhabited

/-- Parse a comma-separated list of type names. -/
def Selection.parse (names : List String) : Except String Selection := do
  -- `true` in the order the structure declares: pictures, sounds, films,
  -- scripts, blobs, models. Written through `Selection.mk` so the field order
  -- cannot drift from the declaration when this is read.
  let all := Selection.mk true true true true true true
  let initial : Selection := Selection.mk false false false false false false
  let mut sel := initial
  let mut seen := false
  for raw in names do
    for part in raw.splitOn "," do
      let name := (toString (part.trimAscii)).toLower
      if name.isEmpty then
        continue
      match name with
      | "all" => sel := all; seen := true
      | "pictures" | "picture" | "images" | "image" => sel := { sel with pictures := true }; seen := true
      | "sounds" | "sound" | "audio" => sel := { sel with sounds := true }; seen := true
      | "films" | "film" | "videos" | "video" => sel := { sel with films := true }; seen := true
      | "scripts" | "script" => sel := { sel with scripts := true }; seen := true
      | "blobs" | "blob" => sel := { sel with blobs := true }; seen := true
      | "models" | "model" | "textures" | "texture" | "3d" => sel := { sel with models := true }; seen := true
      | other => throw s!"unknown asset type '{other}' (expected all, pictures, sounds, films, scripts, blobs or models)"
  if !seen then
    throw "no asset types selected"
  return sel

/-- Whether a container of this kind should be extracted. -/
def Selection.wants (sel : Selection) (kind : ContainerKind) : Bool :=
  match kind with
  | .pictures => sel.pictures
  | .sounds => sel.sounds
  | .films => sel.films
  | .scripts => sel.scripts
  | .blobs => sel.blobs
  | .models => sel.models
  | .textures => sel.models
  | .unknown => false

/-- What one run asked for. -/
structure Options where
  assetDir : System.FilePath
  output : System.FilePath
  selection : Selection
  original : Bool
  raw : Bool
  deriving Inhabited

/-- What was written, per asset type. -/
structure Report where
  pictures : Nat := 0
  sounds : Nat := 0
  films : Nat := 0
  scripts : Nat := 0
  blobs : Nat := 0
  models : Nat := 0
  failed : Array String := #[]
  deriving Inhabited

/-- The report's total. -/
def Report.total (r : Report) : Nat :=
  r.pictures + r.sounds + r.films + r.scripts + r.blobs + r.models

/-- Zero-padded five-digit index, as the Rust extractor's `{index:05}` writes. -/
def index5 (index : Nat) : String := Path.padIndex index

/-- `<stem>_<index>_<safe>`: the file name both tools give one resource. -/
def resourceStem (stem : String) (index : Nat) (name : String) : String :=
  stem ++ "_" ++ index5 index ++ "_" ++ Path.sanitize name index

/-- List the asset folder's files, sorted the way the Rust driver sorts them.

A folder that cannot be read is an error the caller reports.
-/
def listAssets (dir : System.FilePath) : IO (Except String (Array System.FilePath)) := do
  try
    let entries ← System.FilePath.readDir dir
    let mut files : Array System.FilePath := #[]
    for entry in entries do
      let path : System.FilePath := System.FilePath.mk (Path.str entry.root ++ "/" ++ entry.fileName)
      let isDir ← System.FilePath.isDir path
      if !isDir then
        files := files.push path
    -- `Array.qsort` with a `≤` predicate is a stable-enough sort for file
    -- names: what matters is that both tools visit the same order.
    return .ok (files.qsort (fun a b => Path.str a ≤ Path.str b))
  catch e =>
    return .error s!"reading {Path.str dir}: {e}"

/-- Warn about a failed resource and keep going.

The message is flattened to one line first: the report protocol this function
fills is line-oriented, and a resource name read out of a container can hold a
newline (the 3D game's names are garbage in places), which would tear the
protocol and make the Rust side read a partial line as a whole one.
-/
def reportFailure (r : IO.Ref Report) (where_ : String) : IO Unit := do
  let flattened := String.ofList (where_.toList.map (fun c => if c == '\n' || c == '\r' then ' ' else c))
  r.modify fun rep => { rep with failed := rep.failed.push flattened }
  IO.eprintln s!"warning: {flattened}"

/-- Extract every picture resource of a container, the modern (`--original`
absent) or the original way.

`out_dir` receives one PNG per resource, `raw_dir` the untouched payload.
-/
def extractPictures (container : Container) (outDir rawDir : System.FilePath)
    (stem : String) (original : Bool) (r : IO.Ref Report) : IO Unit := do
  let n := container.resourceCount.toNat
  for index in [0:n] do
    let name := container.name index
    let safe := Path.sanitize name index
    if original then
      -- The payload byte for byte: exactly what the game's runtime would hand
      -- its blitter, before any byte-swap transform. Which decompression step
      -- that is depends only on the container's version, and the wiring layer
      -- picks it -- the same rule the Rust CLI's `picture_payload` applies.
      let file := MmfExtract.Path.join outDir (index5 index ++ "_" ++ safe ++ ".dat")
      let payload : Container → Nat → Except String ByteArray :=
        if container.version == 1 then MmfExtract.V1.Picture.payload else Picture.payload
      match payload container index with
      | .error e => reportFailure r s!"{Path.str container.path}: resource {index} '{name}': {e}"
      | .ok data =>
        IO.FS.writeBinFile file data
        r.modify fun rep => { rep with pictures := rep.pictures + 1 }
    else
      -- The untouched payload, byte for byte as it appears in the container.
      let rawFile := MmfExtract.Path.join rawDir (resourceStem stem index name ++ ".bin")
      match Container.payload container index with
      | .error e => reportFailure r s!"{Path.str container.path}: resource {index} '{name}': {e}"
      | .ok payload => IO.FS.writeBinFile rawFile payload
      match Picture.payload container index with
      | .error e =>
        reportFailure r s!"{Path.str container.path}: resource {index} '{name}': {e}"
      | .ok data =>
        match Picture.decodeBytes container index data none with
        | .error e =>
          reportFailure r s!"{Path.str container.path}: resource {index} '{name}': {e}"
        | .ok decoded =>
          let _dim := container.dimension index
          if decoded.width == 0 || decoded.height == 0 then
            continue  -- zero dimension: skipped, not a failure
          let pngFile := MmfExtract.Path.join outDir (resourceStem stem index name ++ ".png")
          try
            Picture.writePng pngFile decoded
            r.modify fun rep => { rep with pictures := rep.pictures + 1 }
          catch e =>
            reportFailure r s!"{Path.str container.path}: resource {index} '{name}': {e}"

/-- One extraction run, reporting what it wrote. -/
def run (options : Options) : IO Report := do
  let r : IO.Ref Report ← IO.mkRef {}
  let assetDir := options.assetDir
  let output := options.output
  let sel := options.selection
  let original := options.original

  let files ← do
    let listed ← listAssets assetDir
    match listed with
    | .ok files => pure files
    | .error e =>
      reportFailure r e
      return (← r.get)

  -- Textures are extracted on their own pass first, because the 3D game's
  -- models embed the textures they name and the container iteration order is
  -- alphabetical, which puts `Models` first.
  -- Textures are extracted first, on their own pass, because the 3D game's
  -- models embed the textures they name and the container iteration order is
  -- alphabetical, which puts `Models` first.
  let mut textureDir : Option System.FilePath := none
  for path in files do
    let opened ← Container.read path
    let container ←
      match opened with
      | .ok c => pure c
      | .error _ => continue
    if container.kind != .textures || !sel.wants container.kind then
      continue
    let stem := Path.stem path
    let outDir := MmfExtract.Path.join (MmfExtract.Path.join output "Textures") stem
    let report ← Dma3d.extractTextures container outDir original
    for (index, name, why) in report.failed do
      reportFailure r s!"{MmfExtract.Path.str path}: texture {index} '{name}': {why}"
    r.modify fun rep => { rep with models := rep.models + report.written }
    textureDir := some outDir

  for path in files do
    let opened ← Container.read path
    let container ←
      match opened with
      | .ok c => pure c
      | .error _ => continue  -- not one of ours
    if !sel.wants container.kind then
      continue
    if container.kind == .textures then
      continue  -- handled on the pass above
    let stem := Path.stem path
    match container.kind with
    | .pictures =>
      IO.FS.createDirAll (MmfExtract.Path.join (MmfExtract.Path.join output "Pictures") stem)
      IO.FS.createDirAll (MmfExtract.Path.join (MmfExtract.Path.join output "Raw") stem)
      extractPictures container
        (MmfExtract.Path.join (MmfExtract.Path.join output "Pictures") stem)
        (MmfExtract.Path.join (MmfExtract.Path.join output "Raw") stem)
        stem original r
    | .sounds =>
      let outDir := MmfExtract.Path.join (MmfExtract.Path.join output "Sounds") stem
      match ← Sound.extract container outDir original with
      | .error e =>
        -- A missing encoder is fatal for the whole container: it is reported as
        -- one failure, not one warning per resource.
        reportFailure r s!"{MmfExtract.Path.str path}: {e}"
      | .ok report =>
        for (index, name, why) in report.failed do
          reportFailure r s!"{MmfExtract.Path.str path}: sound resource {index} '{name}': {why}"
        r.modify fun rep => { rep with sounds := rep.sounds + report.written }
    | .films =>
      let outDir := MmfExtract.Path.join (MmfExtract.Path.join output "Films") stem
      match ← Film.extract container outDir original with
      | .error e =>
        -- A film that cannot be fully decoded is reported and skipped rather
        -- than aborting the whole run.
        reportFailure r s!"{MmfExtract.Path.str path}: {e}"
      | .ok n => r.modify fun rep => { rep with films := rep.films + n }
    | .scripts =>
      let outDir := MmfExtract.Path.join (MmfExtract.Path.join output "Scripts") stem
      match ← Script.extract container outDir original with
      | .error e => reportFailure r s!"{MmfExtract.Path.str path}: {e}"
      | .ok n => r.modify fun rep => { rep with scripts := rep.scripts + n }
    | .blobs => r.modify fun rep => { rep with blobs := rep.blobs + 0 }
    | .models =>
      let outDir := MmfExtract.Path.join (MmfExtract.Path.join output "Models") stem
      let report ← Dma3d.extractModels container outDir original
      for (index, name, why) in report.failed do
        reportFailure r s!"{MmfExtract.Path.str path}: 3D resource {index} '{name}': {why}"
      r.modify fun rep => { rep with models := rep.models + report.written }
    | .textures | .unknown => pure ()

  -- The 3D index page is a *modern-mode* artifact and is written by the Rust
  -- driver, through the `MmfExtract.FFI.index` entry point, after the converter
  -- has added the glTFs. It is not written here for two measured reasons: it
  -- is made of `<img>` tags, so only a file a browser can draw belongs on it,
  -- and in `--original` nothing a browser can draw has been written — a page
  -- produced from that mode listed 460 objects and 629 textures and drew not
  -- one of them, which is decoration that lies. One page, one implementation
  -- (here, in Lean), one writer (the half that produced the last file it
  -- lists).
  let _ := textureDir

  return (← r.get)

end MmfExtract.Extract
