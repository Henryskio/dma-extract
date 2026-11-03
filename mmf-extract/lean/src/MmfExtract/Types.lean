/-!
The types every MMFW module shares: the container kind and a picture's pixel
dimensions.

They live in their own module because the first game's metadata parser and the
shared container parser both need them, and an import cycle between those two is
exactly the layering violation the repository forbids.
-/

namespace MmfExtract

/-- The kind of payload a container holds. -/
inductive ContainerKind where
  | pictures
  | sounds
  | films
  | blobs
  | scripts
  | models
  | textures
  | unknown
  deriving Repr, BEq, DecidableEq, Inhabited

/-- The pixel dimensions of one picture resource. -/
structure Dimension where
  width : UInt16
  height : UInt16
  deriving Repr, BEq, Inhabited

end MmfExtract
