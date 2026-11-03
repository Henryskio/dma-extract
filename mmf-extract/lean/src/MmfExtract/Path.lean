import MmfExtract.Types

/-!
Path and filename helpers.

The Rust extractor asks `std::path` for the parts of a container's file name
(`file_stem`, `file_name`) and sanitises every resource name before it goes
into a file name; both are here, so no decoder has to know how paths are
spelled.
-/

namespace MmfExtract.Path

open System

/-- The string a `FilePath` carries. -/
def str (p : FilePath) : String := p.toString

/-- `dir / name`, spelled the way `std::path::Path::join` spells it. -/
def join (dir : FilePath) (name : String) : FilePath :=
  let base := dir.toString
  if base.isEmpty then FilePath.mk name
  else if base.endsWith "/" then FilePath.mk (base ++ name)
  else FilePath.mk (base ++ "/" ++ name)

/-- The container's file name without its extension, or `"resource"`. -/
def stem (p : FilePath) : String :=
  match FilePath.fileStem p with
  | some s => s
  | none => "resource"

/-- The container's file name including its extension, or `""`. -/
def fileName (p : FilePath) : String :=
  match FilePath.fileName p with
  | some s => s
  | none => ""

/-- Whether a resource name leaves `c` untouched. -/
def safe (c : Char) : Bool :=
  c.isAlphanum || c == '-' || c == '_' || c == '.'

/-- Drop leading and trailing `.` from a character list. -/
def trimDots (cs : List Char) : List Char :=
  (cs.dropWhile (· == '.')).reverse.dropWhile (· == '.') |>.reverse

/-- `index` zero-padded to five digits. -/
def padIndex (index : Nat) : String :=
  let digits := toString index
  String.ofList (List.replicate (5 - min 5 digits.length) '0') ++ digits

/-- Turn a resource name into something safe to put in a filename.

Non-alphanumerics other than `-`, `_` and `.` become `_`, leading and trailing
dots are dropped, and an empty result falls back to `resNNNNN`. This is the rule
`src/cli.rs::sanitize` applies, kept identical so both tools name the same
resource the same way.
-/
def sanitize (name : String) (index : Nat) : String :=
  let cleaned := String.ofList (trimDots (name.toList.map (fun c => if safe c then c else '_')))
  if cleaned.isEmpty then "res" ++ padIndex index else cleaned

end MmfExtract.Path
