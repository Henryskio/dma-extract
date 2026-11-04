#!/bin/sh
# The 3D path end to end, from the shipped containers to a verified glTF.
#
# 1. carve the 3D game's two containers out of its disc image (read-only source)
# 2. `--original`: the Lean core writes every payload byte for byte
# 3. the Lean check compares each file against the container's own slice
# 4. `--modern`: the Rust converter turns a mesh-carrying payload into a glTF
# 5. the Lean check reads that glTF back and holds it to the mesh contract
#
# Run from `mmf-extract/lean`:
#
#   sh test/run3d.sh

set -eu

here=$(cd "$(dirname "$0")" && pwd)
proj="$here/.."
# (the disc is carved by carve3d.py into /workspace/.tmp/dma3d)

dma=/workspace/.tmp/dma3d/assets
bin="$proj/../target/debug/mmf-extract"

# ---- 1. carve the containers, once ----
if [ ! -f "$dma/DMA3D.MMM" ]; then
  python3 "$here/carve3d.py"
fi

# ---- 2. `--original`, in Lean ----
outdir="/tmp/dma3d-original"
rm -rf "$outdir"
"$bin" "$dma" --output "$outdir" --types models,textures --original

# ---- 3. the check, in Lean ----
LEAN_PATH="$proj/.lake/build/lib/lean" lean --run "$here/Test.lean"

# ---- 4. `--modern`, in Rust, on the synthetic container ----
synthdir="$proj/synth/out"
[ -f "$synthdir/DMA3D.MMM" ] || sh "$proj/synth/mkmodel.sh" "$synthdir"
modern="/tmp/dma3d-modern"
rm -rf "$modern"
"$bin" "$synthdir" --output "$modern" --types models

# ---- 5. the verification, in Lean ----
LEAN_PATH="$proj/.lake/build/lib/lean" lean --run "$here/Test.lean"
