#!/usr/bin/env python3
"""Extract selected files from a local ISO9660 image (Joliet if present).

Usage:
    extract_iso.py list <image.iso>
    extract_iso.py copy <image.iso> <dest-dir> <regex> [regex ...]
"""

import os
import re
import struct
import sys

SECTOR = 2048


def read_volume_descriptors(fh):
    fh.seek(16 * SECTOR)
    raw = fh.read(8 * SECTOR)
    root_le = None
    root_joliet = None
    for i in range(len(raw) // SECTOR):
        sec = raw[i * SECTOR : (i + 1) * SECTOR]
        if len(sec) < 2048 or sec[1:6] != b"CD001":
            continue
        vtype = sec[0]
        root = sec[156 : 156 + 34]
        if vtype == 1:
            root_le = root
        elif vtype == 2:
            root_joliet = root
    if root_joliet is not None:
        return root_joliet, True
    if root_le is not None:
        return root_le, False
    raise SystemExit("no volume descriptor found")


def walk(fh, lba, size, joliet, path="", out=None):
    if out is None:
        out = []
    fh.seek(lba * SECTOR)
    data = fh.read(size)
    pos = 0
    while pos < len(data):
        if data[pos] == 0:
            pos = ((pos + SECTOR - 1) // SECTOR) * SECTOR
            continue
        rec = data[pos : pos + 255]
        rec_len = rec[0]
        extent = struct.unpack_from("<I", rec, 2)[0]
        data_len = struct.unpack_from("<I", rec, 10)[0]
        flags = rec[25]
        name_len = rec[32]
        name_bytes = rec[33 : 33 + name_len]
        # Only the bare 1-byte records 0x00/0x01 are "." and "..".
        # Joliet names start with 0x00 for ASCII text, so they must not be
        # skipped on that byte alone.
        if not (name_len == 1 and name_bytes[0] in (0x00, 0x01)):
            if joliet:
                name = name_bytes.decode("utf-16-be", "replace")
            else:
                name = name_bytes.decode("ascii", "replace").split(";")[0]
            full = f"{path}/{name}".lstrip("/")
            if flags & 0x02:
                if name not in (".", ".."):
                    walk(fh, extent, data_len, joliet, full, out)
            else:
                out.append((full, extent, data_len))
        pos += rec_len
    return out


def main():
    mode = sys.argv[1]
    image = sys.argv[2]
    with open(image, "rb") as fh:
        root, joliet = read_volume_descriptors(fh)
        files = walk(fh, struct.unpack_from("<I", root, 2)[0],
                     struct.unpack_from("<I", root, 10)[0], joliet)
    if mode == "list":
        for name, lba, size in sorted(files):
            print(f"{size:>12} {lba:>10} {name}")
        print(f"# {len(files)} files, joliet={joliet}")
        return

    dest = sys.argv[3]
    patterns = [re.compile(p, re.I) for p in sys.argv[4:]]
    os.makedirs(dest, exist_ok=True)
    with open(image, "rb") as fh:
        for name, lba, size in sorted(files):
            if not any(pat.search(name) for pat in patterns):
                continue
            fh.seek(lba * SECTOR)
            data = fh.read(size)
            out_path = os.path.join(dest, name.replace("/", os.sep))
            os.makedirs(os.path.dirname(out_path), exist_ok=True)
            with open(out_path, "wb") as out:
                out.write(data)
            print(f"{len(data):>12} {name}")


if __name__ == "__main__":
    main()
