#!/usr/bin/env python3
"""Carve the 3D game's two containers out of its disc image.

The 3D game ships as a disc image, so its `DMA3D.MMM` and `DMA3D.MMX` are not
files in a folder: they are runs of bytes inside
`Disney Creazioni in 3D (Italy).bin`, and this finds them by their `MMFW `
signature and carves each by its own offsets table.

The carve is into `/workspace/.tmp/dma3d/`, which is gitignored: these are the
game's bytes, and the repository holds no game data.

## How the size is decided, and why it is not read from the header

A container's length is the last entry of its **offset table**, and the table is
`4*(N+1)` entries long. So the count has to be known before the size is, and the
`N` the header declares at `0x22` is not it.

This is measured on the shipped Italian disc, and it is the reason the extraction
used to fail: carving to `offsets[705]` cut `DMA3D.MMX` to 3,230,042 bytes, and
704 of its 705 textures then failed to decode. The true length is 4,019,622 bytes.

The count is therefore **walked**: from the header's `N`, extend it while each
next table entry is a plausible payload offset, i.e. the increment is between 4
bytes and 2 MiB. The walk stops where the table stops describing payloads, and
that entry is the container's length:

* `DMA3D.MMM`: the header says 460, the walk stops immediately (the next entry is
  25,458, which is not an increment), and the length is `offsets[460]` =
  4,139,055 — so the header's count was right.
* `DMA3D.MMX`: the header says 705, the walk runs on to **781**, and the length is
  `offsets[781]` = 4,019,622 — followed by a run of `0x03` padding, which
  corroborates that the container ends there.

The two readings of `DMA3D.MMX` are both self-consistent with the bytes and
disagree by 304 bytes (`4 * 76`), which is what the header's own count and the
table's real extent differ by. `docs/DMA3D.md` §11 records what that costs: the
names table's position and the metadata region's length cannot both be right, and
the payload base (`offsets[0]` = 29654) is the part that does not depend on the
choice — which is why the texture decode is unaffected and 698 of the 781
payloads fit the layout once the file is whole.
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

#: The most a single resource payload can be, in bytes. Beyond this a table entry
#: is not describing a payload and the table has ended.
MAX_PAYLOAD = 2 * 1024 * 1024


def u16(data, at):
    return struct.unpack(">H", data[at:at + 2])[0]


def u32(data, at):
    return struct.unpack(">I", data[at:at + 4])[0]


def count_and_length(data, at):
    """Walk the offset table to (count, length), starting from the header's N.

    Returns `(declared_count, walked_count, length)`.
    """
    base = at + 0x24
    declared = u16(data, at + 0x22)
    n = declared
    length = u32(data, base + 4 * n)
    while True:
        nxt = u32(data, base + 4 * (n + 1))
        delta = nxt - length
        if 4 <= delta < MAX_PAYLOAD:
            n += 1
            length = nxt
            continue
        break
    return declared, n, length


def carve(data, at, name):
    """Carve the container whose header starts at `at`, checking its sizes."""
    declared, n, length = count_and_length(data, at)
    base = at + 0x24

    # Every table entry must be a payload offset: the first one is where the
    # payload region starts, and the walked count's entry is where it ends.
    first = u32(data, base)
    if first <= 0x24 + 4 * (n + 1):
        raise SystemExit(f"{name}: the first payload starts before the offset table ends")
    if length <= first:
        raise SystemExit(f"{name}: offsets table does not increase ({first} -> {length})")

    # The region the names and the metadata occupy, measured rather than
    # assumed. The documented `6*N + 2` holds for the header's count on the
    # English build; on the Italian `Textures` container the walked count and
    # the header's differ by 304 bytes, and which of the two the names follow is
    # not something the shipped bytes decide (`docs/DMA3D.md` §11). What is not
    # in doubt is the payload region, which is what the decode reads.
    names_end_documented = base + 4 * (declared + 1) + 2 + 32 * declared
    blob = data[at:at + length]
    assert len(blob) == length
    return blob, n, declared, length, first, names_end_documented


def main():
    os.makedirs(OUT, exist_ok=True)
    with zipfile.ZipFile(DISC) as z:
        z.extractall(OUT)
    data = open(os.path.join(OUT, IMAGE), "rb").read()
    print(f"disc image: {len(data)} bytes")

    for tag, name in [
        (MODELS_TAG, "DMA3D.MMM"),
        (TEXTURES_TAG, "DMA3D.MMX"),
    ]:
        hits = [m.start() for m in re.finditer(re.escape(tag), data)]
        picked = None
        for at in hits:
            try:
                picked = (at, *carve(data, at, name))
            except (AssertionError, SystemExit, struct.error):
                continue
            break
        if picked is None:
            raise SystemExit(f"{name}: no copy of the container could be carved")
        at, blob, n, declared, length, _first, _ne = picked
        dest = os.path.join(OUT, "assets", name)
        os.makedirs(os.path.dirname(dest), exist_ok=True)
        open(dest, "wb").write(blob)
        print(
            f"carved {name} at {at:#x}: {len(blob)} bytes, "
            f"{n} resources (the header declares {declared}), "
            f"first payload at {u32(data, at + 0x24)}"
        )


if __name__ == "__main__":
    sys.exit(main())
