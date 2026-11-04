#!/usr/bin/env python3
"""Carve the 3D game's two containers out of its disc image.

The 3D game ships as a disc image, so its `DMA3D.MMM` and `DMA3D.MMX` are not
files in a folder: they are runs of bytes inside
`Disney Creazioni in 3D (Italy).bin`, and this finds them by their `MMFW `
signature and carves each by its own offsets table.

The carve is into `/workspace/.tmp/dma3d/`, which is gitignored: these are the
game's bytes, and the repository holds no game data.

The result is checked rather than assumed: the last entry of a container's
offsets table is its own length, so the carve's size is the container's, and
the region between the names table and the first payload must come to `4*N + 100`
(`Models`) and `6*N + 2` (`Textures`) -- the two lengths `docs/DMA3D.md` §2.1
measures.
"""

import os
import re
import struct
import sys
import zipfile

DISC = "/RawAssets/Disney Creazioni in 3D (Italy).zip"
IMAGE = "Disney Creazioni in 3D (Italy).bin"
OUT = "/workspace/.tmp/dma3d"

MODELS_TAG = b"MMFW Models"
TEXTURES_TAG = b"MMFW Textures"


def carve(data, at, name, expect_region):
    """Carve the container whose header starts at `at`, checking its sizes.

    Every offset the table holds is relative to the container's own start, so
    `at` is added to the file-relative positions and subtracted from the
    file-relative one the region is measured with.
    """
    n = struct.unpack(">H", data[at + 0x22:at + 0x24])[0]
    base = at + 0x24
    offs = [struct.unpack(">I", data[base + 4 * i:base + 4 * i + 4])[0] for i in range(n + 1)]
    if offs[-1] <= offs[0]:
        raise SystemExit(f"{name}: offsets table does not increase")
    names_end = 0x24 + 4 * (n + 1) + 2 + 32 * n
    region = offs[0] - names_end
    assert region == expect_region(n), (name, region, expect_region(n))
    blob = data[at:at + offs[n]]
    assert len(blob) == offs[n]
    return blob, n


def main():
    os.makedirs(OUT, exist_ok=True)
    with zipfile.ZipFile(DISC) as z:
        z.extractall(OUT)
    data = open(os.path.join(OUT, IMAGE), "rb").read()
    print(f"disc image: {len(data)} bytes")

    # The same container appears more than once on the disc; the copy that
    # measures `4*N + 100` / `6*N + 2` between its names table and its first
    # payload is the one whose offsets table this carve uses.
    for tag, name, expect in [
        (MODELS_TAG, "DMA3D.MMM", lambda n: 4 * n + 100),
        (TEXTURES_TAG, "DMA3D.MMX", lambda n: 6 * n + 2),
    ]:
        hits = [m.start() for m in re.finditer(re.escape(tag), data)]
        picked = None
        for at in hits:
            try:
                blob, n = carve(data, at, name, expect)
            except (AssertionError, SystemExit):
                continue
            picked = (at, blob, n)
            break
        if picked is None:
            raise SystemExit(f"{name}: no copy measures the region {expect(0)}")
        at, blob, n = picked
        dest = os.path.join(OUT, "assets", name)
        os.makedirs(os.path.dirname(dest), exist_ok=True)
        open(dest, "wb").write(blob)
        print(f"carved {name} at {at:#x}: {len(blob)} bytes, N={n}")


if __name__ == "__main__":
    sys.exit(main())
