#!/bin/sh
# Build the synthetic 3D container `docs/DMA3D.md` §9's verification runs on.
#
# The payload is the documented mesh chain (`docs/DMA3D.md` §3): `u32 C` index
# count, `u16[C]` zero-based indices naming a vertex below V, `u32 V` vertex
# count, `vertex[V]` of four floats with the last always 1.0, `u32 V` normal
# count, `normal[V]` of four floats with the last always 0.0. Around it, a
# container whose skeleton is the shared one: signature, `MM`, version 3, a
# resource count of 1, a 2-entry offsets table, a 32-byte name, a `4*N` u32
# array and the 100-byte `Models` trailer §2.1 measures.
#
# The container is written by hand rather than taken from a game file, so the
# geometry the converter has to meet is known: 6 indices naming 4 vertices,
# with vertex `i` at `(i, i/2, 1)` and normal `(0, 0, 1)`.
#
# Usage: sh lean/synth/mkmodel.sh <output-dir>

set -eu

out=${1:-/workspace/mmf-extract/lean/synth/out}
mkdir -p "$out"

python3 - "$out" <<'PY'
import struct, sys, os

out = sys.argv[1]

# ---- the payload: `mod II` + the mesh chain, all little-endian ----
C, V = 6, 4
p = bytearray(b'mod II')
p += struct.pack('<I', 0x10)          # the header's u32 16
p += b'\x00' * 32                     # a plausible header: 6000 bytes of scan room
p += struct.pack('<I', C)
for i in range(C):
    p += struct.pack('<H', i % V)     # zero-based, every one below V
p += struct.pack('<I', V)
for i in range(V):
    p += struct.pack('<4f', float(i), float(i) * 0.5, 1.0, 1.0)
p += struct.pack('<I', V)
for i in range(V):
    p += struct.pack('<4f', 0.0, 0.0, 1.0, 0.0)
# the `[0, 1, V]` record header and its float pairs: 0..1, so the writer reads
# them as texture coordinates rather than guessing.
p += struct.pack('<I', 0x0000_0001)
for i in range(V):
    p += struct.pack('<4f', 0.0, 0.0, 1.0, 0.0)

# ---- the container ----
n = 1
tag = b'MMFW Models\x00'                      # padded to the 16-byte signature
hdr = bytearray()
hdr += tag.ljust(16, b'\x00')                # 0x00..0x10 signature
hdr += b'MM'                                  # 0x10..0x12 endian marker
hdr += struct.pack('>H', 3)                   # 0x12..0x14 version 3
hdr += b'\x00' * 6                            # 0x14..0x1A file id
hdr += struct.pack('>d', 0.0)                 # 0x1A..0x22 timestamp
hdr += struct.pack('>H', n)                   # 0x22..0x24 resource count
assert len(hdr) == 0x24

off_base = 0x24
names_base = off_base + 4 * (n + 1) + 2
payload_start = names_base + 32 * n + 4 * n + 100
hdr += struct.pack('>I', payload_start)
hdr += struct.pack('>I', payload_start + len(p))
hdr += b'\x00\x00'                            # the 2 bytes after the table
hdr += b'syntheticTest.mod'.ljust(32, b'\x00')
hdr += struct.pack('>I', 0)                   # 4*N u32 array
hdr += b'\x00' * 100                          # the 100-byte trailer
assert len(hdr) == payload_start, (len(hdr), payload_start)

open(os.path.join(out, 'DMA3D.MMM'), 'wb').write(bytes(hdr + p))
print(f"synthetic container: {len(hdr) + len(p)} bytes, payload {len(p)} bytes, chain at 42")
PY
