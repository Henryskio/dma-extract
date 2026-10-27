#!/usr/bin/env python3
"""List the contents of a remote ISO9660 image using HTTP range requests.

Only the volume descriptors and directory records are fetched (a few hundred
kilobytes), not the whole image, so a large CD can be inspected without
downloading or deleting anything.

Usage:
    iso_range.py list   <url>            print the ISO's file table
    iso_range.py save   <url> <out.bin>  extract one image file to disk
                                         (records listed by `iso_range list`)
"""

import struct
import sys
import urllib.request

SECTOR = 2048
USER_AGENT = "curl/8.5.0"


def fetch(url: str, start: int, length: int) -> bytes:
    req = urllib.request.Request(
        url,
        headers={
            "Range": f"bytes={start}-{start + length - 1}",
            "User-Agent": USER_AGENT,
        },
    )
    with urllib.request.urlopen(req, timeout=180) as resp:
        return resp.read()


def read_volume_descriptors(url: str):
    """Return (root_record, joliet) where root_record is the root directory
    record bytes and joliet selects UCS-2 names."""
    blob = fetch(url, 16 * SECTOR, 8 * SECTOR)
    root_be = None
    root_le = None
    for i in range(len(blob) // SECTOR):
        sec = blob[i * SECTOR : (i + 1) * SECTOR]
        if len(sec) < 2048 or sec[1:6] != b"CD001":
            continue
        vtype = sec[0]
        escape = sec[88:120]
        root = sec[156 : 156 + 34]
        if vtype == 1:
            root_le = root
        elif vtype == 2:
            root_be = (root, escape)
    if root_be:
        return root_be[0], True
    if root_le:
        return root_le, False
    raise SystemExit("no volume descriptor found")


def walk(url: str, lba: int, size: int, joliet: bool, path: str = "", out=None):
    """Depth-first walk of one directory extent, recording files."""
    if out is None:
        out = []
    data = fetch(url, lba * SECTOR, size)
    pos = 0
    while pos < len(data):
        rec = data[pos : pos + 255]
        if len(rec) < 34:
            break
        rec_len = rec[0]
        if rec_len == 0:
            # Records do not cross sector boundaries; skip to the next one.
            pos = ((pos + SECTOR - 1) // SECTOR) * SECTOR
            continue
        extent = struct.unpack_from("<I", rec, 2)[0]
        flags = rec[25]
        name_len = rec[32]
        name_bytes = rec[33 : 33 + name_len]
        if joliet:
            name = name_bytes.decode("utf-16-be", "replace")
        else:
            name = name_bytes.decode("ascii", "replace").split(";")[0]
        if name not in ("\x00",) and name_bytes[0] not in (0x00, 0x01):
            if flags & 0x02:
                if name not in (".", ".."):
                    walk(url, extent, struct.unpack_from("<I", rec, 10)[0],
                         joliet, f"{path}/{name}".lstrip("/"), out)
            else:
                out.append((f"{path}/{name}".lstrip("/"), extent,
                            struct.unpack_from("<I", rec, 10)[0]))
        pos += rec_len
    return out


def main():
    mode = sys.argv[1]
    url = sys.argv[2]
    if mode == "list":
        root, joliet = read_volume_descriptors(url)
        files = walk(url, struct.unpack_from("<I", root, 2)[0],
                     struct.unpack_from("<I", root, 10)[0], joliet)
        total = 0
        for name, lba, size in files:
            print(f"{size:>12} {lba:>9} {name}")
            total += size
        print(f"# {len(files)} files, {total} bytes total")
    elif mode == "save":
        name, lba, size = sys.argv[3], int(sys.argv[4]), int(sys.argv[5])
        data = fetch(url, lba * SECTOR, size)
        with open(name, "wb") as fh:
            fh.write(data)
        print(f"wrote {name}: {len(data)} bytes")
    else:
        raise SystemExit(__doc__)


if __name__ == "__main__":
    main()
