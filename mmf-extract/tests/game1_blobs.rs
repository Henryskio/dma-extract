//! The first game's `Blobs` payloads — the `goofy 2.6.3` animation records.
//!
//! These tests pin the format the game's own reader consumes, and the one
//! property that makes a modern output trustworthy: **decoding every payload
//! and re-encoding it reproduces the container's bytes exactly**. That is the
//! `AGENTS.md` 1:1 rule applied to this container kind, and it is what lets a
//! viewer rely on the JSON dump rather than on `--original`.
//!
//! The reader is `FUN_004b5f34` in `DISNEYMA.EXE` (see `docs/GAME1.md` §1 for
//! the carve table), reached through the header parser `FUN_004b5c6b`, the
//! object iterator `FUN_004b5ec0` and the object switch `FUN_004a7ee0`. What
//! those four functions establish is written out in `docs/GAME1.md` §6.

use mmf_extract::container::Container;
use mmf_extract::v1::blobs::{Blob, OBJECT_BYTES, OBJECT_KIND_MAX, OBJECT_KIND_MIN, STAGE_OFFSET};
use std::path::{Path, PathBuf};

/// The `u16` length prefix and the 12-byte version stamp that opens every one
/// of the 305 `pack.BLB` payloads: `0x000c` then `"goofy 2.6.3\0"`.
const STAMP: &[u8] = b"\x00\x0cgoofy 2.6.3\x00";

/// The `d` value every record of the three smallest payloads carries, and
/// which the frame loop dispatches on.
const OP_MOVE: u32 = 19;

/// Payloads whose whole length is accountably header + records + terminator.
///
/// The header is 38 bytes: the 14-byte stamp, then a fixed 20-byte prologue,
/// then `u32 count` and `u32 count * 8`. Those last two are the proof — they
/// make the body size an exact multiple of the record width, which is the one
/// thing the reader asserts about the stream.
const DECODED: &[(&str, u32)] = &[("session 1", 49), ("session 2", 103), ("session 3", 78)];

/// How many payloads `pack.BLB` ships.
const PAYLOADS: usize = 305;

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

/// Open `pack.BLB`, or skip the test when the first game's assets are absent.
fn open_blobs() -> Option<(Container, PathBuf)> {
    let dir = game1_dir()?;
    let path = dir.join("pack.BLB");
    if !path.is_file() {
        eprintln!("skipping: pack.BLB not present");
        return None;
    }
    let container = Container::open(&path).expect("open pack.BLB");
    Some((container, path))
}

/// Every payload opens with the same 14-byte stamp, and the container's own
/// bytes are what `--original` writes.
#[test]
fn every_blob_payload_starts_with_the_goofy_stamp() {
    let Some((container, _)) = open_blobs() else {
        eprintln!("skipping: first game assets not present");
        return;
    };
    assert_eq!(container.version, 1);
    assert_eq!(container.kind, mmf_extract::container::ContainerKind::Blobs);
    let n = container.resource_count as usize;
    assert_eq!(n, PAYLOADS, "pack.BLB holds 305 resources");

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
        assert_eq!(payload, &container.data[start..start + payload.len()]);
    }
}

/// The stamp is a writer's watermark that the loader **does** check.
///
/// `FUN_004b5c6b` reads the stamp and compares it against the string at
/// `0x512aac` before it will set the stream's version field to `0x107`, so the
/// bytes are validated on load. What they are *not* is a magic that makes the
/// payload self-describing: the parser reads past them by length, not by
/// matching a fixed offset table.
#[test]
fn the_stamp_is_a_length_prefixed_string_the_loader_checks() {
    let Some((container, _)) = open_blobs() else {
        return;
    };
    for i in 0..container.resource_count as usize {
        let payload = container.payload(i);
        let len = u16::from_be_bytes([payload[0], payload[1]]);
        assert_eq!(len, 0x0c, "payload {i}: the stamp's length prefix");
        assert_eq!(
            &payload[2..2 + len as usize],
            b"goofy 2.6.3\0",
            "payload {i}: the stamp's string"
        );
    }
}

/// The three smallest payloads decode completely through the reader's layout.
///
/// Header is 38 bytes: the 14-byte stamp, a fixed 20-byte prologue, then
/// `u32 count` and `u32 count * 8`. The records are `{u16 a; u16 b; u32 d}`,
/// big-endian, and the stream ends with a `d == 0` terminator.
#[test]
fn the_three_smallest_payloads_decode_through_the_readers_layout() {
    let Some((container, _)) = open_blobs() else {
        return;
    };
    for (want_name, want_count) in DECODED {
        let Some(i) =
            (0..container.resource_count as usize).find(|&i| container.name(i) == *want_name)
        else {
            eprintln!("skipping {want_name}: not present");
            continue;
        };
        let p = container.payload(i);

        // 38-byte header: stamp, fixed prologue, count, count*8.
        assert_eq!(&p[..STAMP.len()], STAMP, "{want_name}: stamp");
        assert_eq!(
            &p[STAMP.len()..30],
            &[
                0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x0c, 0x00, 0x00, 0x00, 0x28, 0x17, 0x00,
                0x00, 0xff
            ],
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
    let Some((container, _)) = open_blobs() else {
        return;
    };
    for (want_name, _) in DECODED {
        let Some(i) =
            (0..container.resource_count as usize).find(|&i| container.name(i) == *want_name)
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

/// Every one of the 305 payloads closes its stream the same way.
///
/// This is a property of the container, not of the three payloads that are
/// fully decoded, so it is asserted over all of them: the last four bytes of
/// every payload are a zero, which is the terminator record's `d`.
#[test]
fn every_payload_ends_with_a_zero_terminator() {
    let Some((container, _)) = open_blobs() else {
        return;
    };
    let n = container.resource_count as usize;
    for i in 0..n {
        let p = container.payload(i);
        assert!(p.len() >= 8, "payload {i} is shorter than one record");
        assert_eq!(
            u32::from_be_bytes([
                p[p.len() - 4],
                p[p.len() - 3],
                p[p.len() - 2],
                p[p.len() - 1]
            ]),
            0,
            "payload {i} does not end with the d == 0 terminator"
        );
    }
}

/// Every payload's header carries a count and a byte size in a fixed ratio.
///
/// The `u32` just past the stamp and the `u32` after it satisfy
/// `second == first * 12` in all 305 payloads. That is a real, universal
/// property of the header, and it is what makes the section boundary
/// derivable — but the 12-byte section has **not** been decoded, so this test
/// pins only the arithmetic, not an interpretation of the section's contents.
#[test]
fn every_header_carries_a_count_and_its_byte_size() {
    let Some((container, _)) = open_blobs() else {
        return;
    };
    let n = container.resource_count as usize;
    for i in 0..n {
        let p = container.payload(i);
        assert!(p.len() >= 22, "payload {i} is too short for a header");
        let count = u32::from_be_bytes([p[14], p[15], p[16], p[17]]);
        let size = u32::from_be_bytes([p[18], p[19], p[20], p[21]]);
        assert!(count > 0, "payload {i}: the header's count is zero");
        assert_eq!(
            size,
            count * OBJECT_BYTES as u32,
            "payload {i}: the header's size field is not count * 12"
        );
    }
}

/// **The 1:1 test.** Decode every payload and re-encode it: the result must be
/// the container's bytes, exactly.
///
/// Everything the decoder reads is written back — the stamp, both header
/// counts, every object header, the record length tag and every record — so a
/// payload that survives this round trip has been accounted for byte for byte.
/// A payload the decoder cannot account for fails here rather than producing
/// output that silently disagrees with the game's own reader.
#[test]
fn every_payload_round_trips_through_decode_and_encode() {
    let Some((container, _)) = open_blobs() else {
        return;
    };
    let n = container.resource_count as usize;
    let mut objects = 0usize;
    let mut records = 0usize;
    for i in 0..n {
        let payload = container.payload(i);
        let blob = Blob::decode(payload)
            .unwrap_or_else(|e| panic!("payload {i} '{}': {e}", container.name(i)));
        assert_eq!(
            blob.encode(),
            payload,
            "payload {i} '{}': re-encoding does not reproduce the payload",
            container.name(i)
        );
        objects += blob.objects.len();
        records += blob.records().count();
    }
    // The container's totals, measured here rather than restated from prose.
    assert_eq!(objects, 209_751, "pack.BLB drives 209,751 objects");
    assert_eq!(records, 8_817_121, "pack.BLB holds 8,817,121 records");
}

/// The object section and the record stream account for the whole payload.
///
/// Each of these is a relation the game's own parser asserts, checked here on
/// every payload so a change to the decoder cannot quietly start accepting a
/// payload whose arithmetic does not close.
#[test]
fn the_payload_arithmetic_closes_on_every_payload() {
    let Some((container, _)) = open_blobs() else {
        return;
    };
    for i in 0..container.resource_count as usize {
        let payload = container.payload(i);
        let blob = Blob::decode(payload)
            .unwrap_or_else(|e| panic!("payload {i} '{}': {e}", container.name(i)));

        // object_bytes == count * 12, and the section ends where the records'
        // length tag begins.
        assert_eq!(
            blob.object_bytes,
            blob.object_count * OBJECT_BYTES as u32,
            "payload {i}: object_bytes is not count * 12"
        );
        let objects_end = 22 + blob.object_bytes as usize;
        assert!(
            objects_end + 4 <= payload.len(),
            "payload {i}: the object section overruns the payload"
        );

        // The length tag equals the bytes that actually remain after it.
        let tag = u32::from_be_bytes([
            payload[objects_end],
            payload[objects_end + 1],
            payload[objects_end + 2],
            payload[objects_end + 3],
        ]);
        assert_eq!(
            tag, blob.record_bytes,
            "payload {i}: the length tag and the decoded record_bytes disagree"
        );
        assert_eq!(
            blob.record_bytes as usize,
            payload.len() - objects_end - 4,
            "payload {i}: the length tag does not cover the remaining bytes"
        );
        assert_eq!(
            blob.record_bytes as usize % 8,
            0,
            "payload {i}: the record stream is not a whole number of records"
        );

        // Every object's records sum to the stream's records, and each object's
        // run is closed by an event-0 terminator.
        let sum: usize = blob.objects.iter().map(|o| o.record_count as usize).sum();
        assert_eq!(
            sum,
            blob.record_bytes as usize / 8,
            "payload {i}: the objects' record counts do not fill the stream"
        );
        for (oi, object) in blob.objects.iter().enumerate() {
            assert_eq!(
                object.records.len(),
                object.record_count as usize,
                "payload {i} object {oi}: the record count and the records disagree"
            );
            let last = object
                .records
                .last()
                .unwrap_or_else(|| panic!("payload {i} object {oi}: no records"));
            assert_eq!(
                last.event, 0,
                "payload {i} object {oi}: the run is not closed by event 0"
            );
        }
    }
}

/// Every object kind is one the game's switch constructs.
///
/// `FUN_004a7ee0` switches on the object's first `u32` with cases from
/// [`OBJECT_KIND_MIN`] to [`OBJECT_KIND_MAX`], so a kind outside that range
/// would be a payload the game itself could not load.
#[test]
fn every_object_kind_is_one_the_game_switches_on() {
    let Some((container, _)) = open_blobs() else {
        return;
    };
    for i in 0..container.resource_count as usize {
        let blob =
            Blob::decode(container.payload(i)).unwrap_or_else(|e| panic!("payload {i}: {e}"));
        for (oi, object) in blob.objects.iter().enumerate() {
            assert!(
                (OBJECT_KIND_MIN..=OBJECT_KIND_MAX).contains(&object.kind),
                "payload {i} object {oi}: kind {} is outside the switch",
                object.kind
            );
        }
    }
}

/// The reader's normalisation is the stage offset its own code applies.
///
/// `FUN_004b5f34` adds `0xfce0` to both coordinates when the stream's version
/// reads `0x107`, and the header parser sets exactly that once the stamp
/// matches. The test pins the arithmetic on a record whose stored values are
/// known, so a change to the transform is visible.
#[test]
fn the_reader_adds_the_stage_offset_to_both_coordinates() {
    let Some((container, _)) = open_blobs() else {
        return;
    };
    let Some(i) =
        (0..container.resource_count as usize).find(|&i| container.name(i) == "session 1")
    else {
        return;
    };
    let blob = Blob::decode(container.payload(i)).expect("decode session 1");
    let first = blob.objects[0].records[0];
    // session 1's first record is a=1680, b=720 on the wire.
    assert_eq!(first.a, 1680);
    assert_eq!(first.b, 720);

    // The reader's `add word [esi], 0xfce0` is u16 arithmetic applied to the
    // stored value, so it is the offset itself that produces the stage
    // coordinates. `0xfce0` overflows to a negative i16, which is why one
    // `wrapping_add` serves both the positive and the negative case.
    assert_eq!(first.a as u16 as i32 + STAGE_OFFSET, 880);
    assert_eq!(first.b as u16 as i32 + STAGE_OFFSET, -80);
    assert_eq!(first.stage_coords(), (880, -80));
    assert_eq!((first.a.wrapping_add(0xfce0)) as i16 as i32, 880);
    assert_eq!((first.b.wrapping_add(0xfce0)) as i16 as i32, -80);
}

/// A payload that does not fit is refused, not guessed at.
///
/// `AGENTS.md` makes an unmeasured guess a failure. Each mutation below breaks
/// one measured relation, and the decoder must reject it rather than emit
/// output that disagrees with the game's own reader.
#[test]
fn a_payload_that_breaks_a_measured_relation_is_refused() {
    // A truncated payload: the object section runs past the end.
    let mut short = vec![0u8; 22 + 12];
    short[..STAMP.len()].copy_from_slice(STAMP);
    short[14..18].copy_from_slice(&1u32.to_be_bytes());
    short[18..22].copy_from_slice(&12u32.to_be_bytes());
    let err = Blob::decode(&short).unwrap_err().to_string();
    assert!(
        err.contains("object section") || err.contains("payload is"),
        "a payload with no record tag should fail on its arithmetic: {err}"
    );

    // A payload whose object_bytes does not equal count * 12.
    let mut broken = vec![0u8; 22 + 12];
    broken[..STAMP.len()].copy_from_slice(STAMP);
    broken[14] = 0;
    broken[15] = 0;
    broken[16] = 0;
    broken[17] = 1; // count 1
    broken[18] = 0;
    broken[19] = 0;
    broken[20] = 0;
    broken[21] = 24; // bytes 24, not 12
    let err = Blob::decode(&broken).unwrap_err().to_string();
    assert!(
        err.contains("object_bytes"),
        "count * 12 must be enforced: {err}"
    );

    // A payload whose one object's run does not end in event 0.
    let mut full = vec![0u8; 22 + 12 + 4 + 8];
    full[..STAMP.len()].copy_from_slice(STAMP);
    full[14..18].copy_from_slice(&1u32.to_be_bytes());
    full[18..22].copy_from_slice(&12u32.to_be_bytes());
    // object header: kind 6, parameter 0, one record
    full[22..26].copy_from_slice(&6u32.to_be_bytes());
    full[26..30].copy_from_slice(&0u32.to_be_bytes());
    full[30..34].copy_from_slice(&1u32.to_be_bytes());
    // record_bytes 8
    full[34..38].copy_from_slice(&8u32.to_be_bytes());
    // the one record carries event 19, not the terminator
    full[42..46].copy_from_slice(&19u32.to_be_bytes());
    let err = Blob::decode(&full).unwrap_err().to_string();
    assert!(
        err.contains("event 0") || err.contains("terminator"),
        "a run not closed by event 0 must be refused: {err}"
    );

    // And the same payload with the terminator in place decodes.
    full[42..46].copy_from_slice(&0u32.to_be_bytes());
    let blob = Blob::decode(&full).expect("a closed run decodes");
    assert_eq!(blob.objects.len(), 1);
    assert_eq!(blob.objects[0].records.len(), 1);
    assert_eq!(blob.encode(), full, "the minimal payload round-trips");
}
