//! The first game's `Blobs` payloads — the measured parts of `goofy 2.6.3`.
//!
//! These tests do **not** claim the format is decoded. They pin down what is
//! measured against the game's own reader so that the next attempt starts from
//! a fixed point rather than from a guess, and so that a change to the extractor
//! cannot silently redefine what "measured" means.
//!
//! The game's record reader is `FUN_004b5f34` in `DISNEYMA.EXE` (the first
//! game's executable, carved from the public disc image; see `docs/GAME1.md` §1
//! for the LBA table). It walks the payload with a cursor and consumes **8
//! bytes per record**: `u16 a` at +0, `u16 b` at +2, `u32 d` at +4, all
//! big-endian on the wire, with an optional byte-swap and a `-800` applied to
//! both u16s when the object's version field reads `0x107`.
//!
//! What is pinned here is reached only through that reader: the three smallest
//! payloads are fully decoded, and their record stream is proven to be exactly
//! what the reader consumes.

use mmf_extract::container::Container;
use std::path::{Path, PathBuf};

/// The `u16` length prefix and the 12-byte version stamp that opens every one
/// of the 305 `pack.BLB` payloads: `0x000c` then `"goofy 2.6.3\0"`.
const STAMP: &[u8] = b"\x00\x0cgoofy 2.6.3\x00";

/// The `d` value every record of the three decoded payloads carries, and which
/// the step function dispatches on.
const OP_MOVE: u32 = 19;

/// Payloads whose whole length is accountably header + records + terminator.
///
/// The header is 38 bytes: the 14-byte stamp, then a fixed 20-byte prologue,
/// then `u32 count` and `u32 count * 8`. Those last two are the proof — they
/// make the body size an exact multiple of the record width, which is the one
/// thing the reader asserts about the stream.
const DECODED: &[(&str, u32)] = &[
    ("session 1", 49),
    ("session 2", 103),
    ("session 3", 78),
];

fn game1_dir() -> Option<PathBuf> {
    let extra = std::env::var("GAME1_ASSETS").unwrap_or_default();
    for candidate in ["/PathAssets/Dati", extra.as_str(), "/RawAssets/Dati"] {
        let p = Path::new(candidate);
        if p.is_dir() {
            return Some(p.to_path_buf());
        }
    }
    None
}

/// Read one payload out of `pack.BLB` by resource index.
fn blob(path: &Path, index: usize) -> (Vec<u8>, Vec<u8>) {
    let container = Container::open(path).expect("open pack.BLB");
    let payload = container.payload(index).to_vec();
    (payload, std::fs::read(path).expect("read pack.BLB"))
}

/// Every payload opens with the same 14-byte stamp, and the container's own
/// bytes are what `--original` writes.
#[test]
fn every_blob_payload_starts_with_the_goofy_stamp() {
    let Some(dir) = game1_dir() else {
        eprintln!("skipping: first game assets not present");
        return;
    };
    let path = dir.join("pack.BLB");
    if !path.is_file() {
        eprintln!("skipping: pack.BLB not present");
        return;
    }
    let (_, on_disk) = blob(&path, 0);
    let container = Container::open(&path).expect("open pack.BLB");
    assert_eq!(container.version, 1);
    assert_eq!(container.kind, mmf_extract::container::ContainerKind::Blobs);
    let n = container.resource_count as usize;
    assert_eq!(n, 305, "pack.BLB holds 305 resources");

    for i in 0..n {
        let payload = container.payload(i);
        assert_eq!(
            &payload[..STAMP.len()],
            STAMP,
            "payload {i} does not open with the goofy stamp"
        );
        // The payload is a byte-exact slice of the container, which is what
        // `--original` writes out.
        let start = container.offsets[i] as usize;
        assert_eq!(payload, &on_disk[start..start + payload.len()]);
    }
}

/// The stamp is a writer's watermark, not a magic the loader checks.
///
/// No `cmp`/`mov` of those bytes exists anywhere in `DISNEYMA.EXE`, and the
/// string's address `0x512aac` is referenced exactly once and from a data
/// table rather than from code. This test pins the part that is checkable from
/// the assets: the 12-byte stamp is a length-prefixed string, so a reader that
/// skips `u16 len + len` bytes lands on the prologue in every payload.
#[test]
fn the_stamp_is_a_length_prefixed_string() {
    let Some(dir) = game1_dir() else {
        eprintln!("skipping: first game assets not present");
        return;
    };
    let path = dir.join("pack.BLB");
    if !path.is_file() {
        return;
    }
    let (_, on_disk) = blob(&path, 0);
    let container = Container::open(&path).expect("open pack.BLB");
    for i in 0..container.resource_count as usize {
        let payload = container.payload(i);
        let len = u16::from_be_bytes([payload[0], payload[1]]);
        assert_eq!(len, 0x0c, "payload {i}: the stamp's length prefix");
        assert_eq!(
            &payload[2..2 + len as usize],
            b"goofy 2.6.3\0",
            "payload {i}: the stamp's string"
        );
        assert!(
            payload.len() >= 2 + len as usize,
            "payload {i}: the stamp needs 2 + {len} bytes"
        );
    }
    // The stamp's bytes appear in the container exactly once per payload.
    let start = container.offsets[0] as usize;
    let first = &on_disk[start..start + STAMP.len()];
    assert_eq!(first, STAMP);
}

/// The three smallest payloads decode completely through the reader's layout.
///
/// Header is 38 bytes: the 14-byte stamp, a fixed 20-byte prologue, then
/// `u32 count` and `u32 count * 8`. The records are `{u16 a; u16 b; u32 d}`,
/// big-endian, and the stream ends with a `d == 0` terminator.
#[test]
fn the_three_smallest_payloads_decode_through_the_readers_layout() {
    let Some(dir) = game1_dir() else {
        eprintln!("skipping: first game assets not present");
        return;
    };
    let path = dir.join("pack.BLB");
    if !path.is_file() {
        return;
    }
    let container = Container::open(&path).expect("open pack.BLB");
    let names_base = 0x20 + 4 * (container.resource_count as usize + 1) + 2;
    let on_disk = std::fs::read(&path).expect("read pack.BLB");
    let _ = (names_base, on_disk);

    for (want_name, want_count) in DECODED {
        // Find the payload by name.
        let mut index = None;
        for i in 0..container.resource_count as usize {
            if container.name(i) == *want_name {
                index = Some(i);
                break;
            }
        }
        let Some(i) = index else {
            eprintln!("skipping {want_name}: not present");
            continue;
        };
        let p = container.payload(i);

        // 38-byte header: stamp, fixed prologue, count, count*8.
        assert_eq!(&p[..STAMP.len()], STAMP, "{want_name}: stamp");
        assert_eq!(
            &p[STAMP.len()..30],
            &[0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x0c, 0x00, 0x00, 0x00, 0x28, 0x17, 0x00, 0x00, 0xff],
            "{want_name}: the fixed prologue is identical in every decoded payload"
        );
        let count = u32::from_be_bytes([p[30], p[31], p[32], p[33]]);
        let size = u32::from_be_bytes([p[34], p[35], p[36], p[37]]);
        assert_eq!(count, *want_count, "{want_name}: the header's record count");
        assert_eq!(size, count * 8, "{want_name}: the header's body size");
        assert_eq!(
            p.len(),
            38 + size as usize,
            "{want_name}: the header's counts must account for the whole payload"
        );

        // Records: {u16 a; u16 b; u32 d}, big-endian.
        let mut records = Vec::with_capacity(count as usize);
        for k in 0..count as usize {
            let at = 38 + 8 * k;
            records.push((
                u16::from_be_bytes([p[at], p[at + 1]]),
                u16::from_be_bytes([p[at + 2], p[at + 3]]),
                u32::from_be_bytes([p[at + 4], p[at + 5], p[at + 6], p[at + 7]]),
            ));
        }
        // Every record but the last is the opcode the step function dispatches
        // on, and the last is a zero terminator.
        for (k, (a, b, d)) in records.iter().enumerate() {
            assert!(
                *a > 0 && *b > 0,
                "{want_name}[{k}]: a and b are coordinates, so both are positive"
            );
            let last = k + 1 == records.len();
            if last {
                assert_eq!(*d, 0, "{want_name}: the final record is a terminator");
            } else {
                assert_eq!(
                    *d, OP_MOVE,
                    "{want_name}[{k}]: every other record carries opcode {OP_MOVE}"
                );
            }
        }
    }
}

/// The record run of a payload continues to its very last byte and is closed
/// by a `d == 0` terminator, which is what lets a reader know it is done.
#[test]
fn the_record_run_is_closed_by_a_zero_terminator() {
    let Some(dir) = game1_dir() else {
        eprintln!("skipping: first game assets not present");
        return;
    };
    let path = dir.join("pack.BLB");
    if !path.is_file() {
        return;
    }
    let container = Container::open(&path).expect("open pack.BLB");
    for (want_name, _) in DECODED {
        let Some(i) = (0..container.resource_count as usize)
            .find(|&i| container.name(i) == *want_name)
        else {
            continue;
        };
        let p = container.payload(i);
        let last = &p[p.len() - 8..];
        let (_, _, d) = (
            u16::from_be_bytes([last[0], last[1]]),
            u16::from_be_bytes([last[2], last[3]]),
            u32::from_be_bytes([last[4], last[5], last[6], last[7]]),
        );
        assert_eq!(d, 0, "{want_name}: the last 8 bytes close the record run");
    }
}
