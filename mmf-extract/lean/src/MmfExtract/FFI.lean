import MmfExtract.Extract

/-!
The one entry point the Rust CLI calls.

`mmf-extract` keeps its command line in Rust (`src/main.rs`, `src/cli.rs`), so
the Lean side of the split is a single C-ABI function: a request string in, a
report string out. The request is line-oriented, `key` TAB `value`:

```text
asset_dir	/RawAssets
output	/workspace/extracted
types	pictures,sounds
original	0
raw	0
```

and the report mirrors the Rust `Summary`:

```text
pictures	30404
sounds	683
failed	2
<message for each failure>
```

The function is `unsafe` because it runs an `IO` action immediately, which is
what a C caller wants; `unsafeIO` is the same escape hatch Lean's own `main`
uses.
-/

namespace MmfExtract.FFI

/-- Render one report line. -/
private def line (key : String) (value : String) : String := key ++ "\t" ++ value ++ "\n"

/-- The report text the Rust CLI parses and prints. -/
def render (report : MmfExtract.Extract.Report) : String :=
  line "pictures" (toString report.pictures)
  ++ line "sounds" (toString report.sounds)
  ++ line "films" (toString report.films)
  ++ line "scripts" (toString report.scripts)
  ++ line "blobs" (toString report.blobs)
  ++ line "models" (toString report.models)
  ++ line "total" (toString report.total)
  ++ line "failed" (toString report.failed.size)
  ++ String.intercalate "" (report.failed.toList.map (fun f => "fail\t" ++ f ++ "\n"))

/-- Parse the request string into [`MmfExtract.Extract.Options`]. -/
def parseRequest (request : String) : Except String MmfExtract.Extract.Options := do
  let mut assetDir : Option String := none
  let mut output : Option String := none
  let mut types : Option (List String) := none
  let mut original := false
  let mut raw := false
  for row in request.splitOn "\n" do
    if row.isEmpty then
      continue
    let parts := row.splitOn "\t"
    let key := parts.getD 0 ""
    let value := String.intercalate "\t" (parts.drop 1)
    match key with
    | "asset_dir" => assetDir := some value
    | "output" => output := some value
    | "types" => types := some (value.splitOn ",")
    | "original" => original := value == "1"
    | "raw" => raw := value == "1"
    | _ => throw s!"unknown request key '{key}'"
  let some dir := assetDir | throw "request has no asset_dir"
  let out := output.getD "."
  let selection ← MmfExtract.Extract.Selection.parse (types.getD ["all"])
  return {
    assetDir := System.FilePath.mk dir, output := System.FilePath.mk out,
    selection, original, raw
  }

/-- The request-and-report boundary.

Returns the rendered report. A request that cannot be parsed is reported as one
failed run rather than as a crash, so the Rust CLI can print the reason and exit
the way it does for its own errors.
-/
@[export mmf_extract]
unsafe def extract (request : String) : String :=
  match parseRequest request with
  | .error e => "error\t" ++ e ++ "\n"
  | .ok options =>
    match unsafeIO (do
      let report ← MmfExtract.Extract.run options
      pure (render report)) with
    | .error e => "error\t" ++ toString e ++ "\n"
    | .ok text => text

end MmfExtract.FFI
