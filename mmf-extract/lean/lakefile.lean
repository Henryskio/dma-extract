import Lake
open Lake DSL

package «mmf-extract-lean» where

-- The extraction core. `MmfExtract` is the library root: every module under it
-- is a separate file so the C compiler sees them as one translation unit chain
-- and the shared-library initializer wires their module initializers.
@[default_target]
lean_lib MmfExtract where
  roots := #[`MmfExtract]
  srcDir := "src"
