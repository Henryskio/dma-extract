//! The first game's payload path, tested without the shipped files.
//!
//! `tests/game1.rs` pins the *measurements* taken from the game's CD. This
//! file pins the *code* that consumes those measurements, so it runs anywhere:
//! it builds a synthetic version-1 container in memory, stores one payload
//! plain and one as the first game's LZSS stream, and checks that the
//! `--original` path writes back exactly the bytes the runtime would see.
//!
//! Two things have to be true for that to be a real check rather than a
//! tautology:
//!
//! 1. The LZSS **compressor** below has to be an independent implementation
//!    of the format, not the decompressor run backwards. It is written from
//!    the token layout alone — a control bit per token, one byte for a
//!    literal, a big-endian u16 `((length-3) << 2) | distance` for a match —
//!    and the decompressor has to accept what it produces.
//! 2. The container builder has to lay out a version-1 picture region by the
//!    documented offsets, so the metadata parser really is exercised.

use mmf_extract::cli;
use mmf_extract::container::{Container, ContainerKind};
use mmf_extract::picture;
use mmf_extract::v1;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// The first game's LZSS encoder, written from the token layout.
// ---------------------------------------------------------------------------

/// One LZSS token, before the control bytes are added.
#[derive(Debug, Clone, Copy)]
enum Token {
    /// A literal byte, encoded as itself.
    Literal(u8),
    /// `distance` bytes back, `length` long.
    Match { distance: usize, length: usize },
}

/// Greedily tokenise `src` into LZSS tokens.
///
/// A match of `distance` bytes back and `length` long reproduces the `length`
/// bytes that start at `src[p - distance]`, which is what makes overlapping
/// runs legal: the decoder copies from a window it is still writing into.
/// Reducing a match to "`length` consecutive bytes equal to the `length` bytes
/// at `p - distance`" is therefore exact, not an approximation.
///
/// The two caps below are the format's, read off the u16 that encodes a match
/// rather than chosen for convenience: 10 low bits hold the distance, and the
/// remaining 6 hold `length - 3`. A length of 68 or a distance of 1024 would
/// overflow into the other field, which is why the caps are 66 and 1023.
fn tokenize(src: &[u8]) -> Vec<Token> {
    const MIN_LENGTH: usize = 3;
    const MAX_LENGTH: usize = MIN_LENGTH + (1 << 6) - 1; // 66
    const MAX_DISTANCE: usize = (1 << 10) - 1; // 1023

    let mut tokens = Vec::new();
    let mut pos = 0usize;
    while pos < src.len() {
        let max_length = (src.len() - pos).min(MAX_LENGTH);
        let mut best: Option<(usize, usize)> = None; // (distance, length)
        if max_length >= MIN_LENGTH {
            let max_distance = pos.min(MAX_DISTANCE);
            for distance in 1..=max_distance {
                let mut length = 0usize;
                while length < max_length && src[pos - distance + length] == src[pos + length] {
                    length += 1;
                }
                if length >= MIN_LENGTH && best.is_none_or(|(_, best_len)| length > best_len) {
                    best = Some((distance, length));
                }
            }
        }
        match best {
            Some((distance, length)) => {
                tokens.push(Token::Match { distance, length });
                pos += length;
            }
            None => {
                tokens.push(Token::Literal(src[pos]));
                pos += 1;
            }
        }
    }
    tokens
}

/// Encode `src` as a first-game LZSS stream.
///
/// Eight tokens per control byte, least significant bit first, bit set means
/// the token is a literal. A match is two big-endian bytes holding
/// `((length - 3) << 10) | slot`, where `slot` is the *window index* to read
/// from rather than the number of bytes to reach back.
///
/// That distinction is the whole reason this function tracks its own write
/// cursor: the decoder's window is a 1024-byte ring whose cursor starts at 958
/// and wraps, so `n` bytes back is not `n` in the encoded word. Emitting the
/// reach-back directly is the mistake that makes a match read the zeroed
/// prefix of the window instead of the actual history.
fn lzss_compress(src: &[u8]) -> Vec<u8> {
    /// Where the decoder's write cursor starts: it zeroes the first 958 slots
    /// and begins writing at 958.
    const WINDOW_START: usize = 958;
    const WINDOW: usize = 1024;

    let tokens = tokenize(src);
    let mut out = Vec::with_capacity(src.len() / 2 + tokens.len() / 8 + 1);
    let mut wp: usize = WINDOW_START;
    for group in tokens.chunks(8) {
        let mut control = 0u8;
        let control_slot = out.len();
        out.push(0);
        for (slot, token) in group.iter().enumerate() {
            match *token {
                Token::Literal(byte) => {
                    control |= 1 << slot;
                    out.push(byte);
                    wp = (wp + 1) % WINDOW;
                }
                Token::Match { distance, length } => {
                    // The decoder reads `distance = word & 0x3FF` and
                    // `length = (word >> 10) + 3`, so the length field is the
                    // word's *upper* six bits. Shifting it left by two instead
                    // overflows the distance and silently invents matches.
                    let word = ((length - 3) << 10) | (WINDOW + wp - distance) % WINDOW;
                    assert!(word >> 16 == 0, "match word {word} overflows 16 bits");
                    out.push((word >> 8) as u8);
                    out.push(word as u8);
                    wp = (wp + length) % WINDOW;
                }
            }
        }
        out[control_slot] = control;
    }
    out
}

/// The encoder's match word must stay inside the 16 bits the format gives it
/// for every legal (length, distance) pair, and the caps in [`tokenize`] must
/// be exactly the ones the decoder can decode.
#[test]
fn match_words_stay_inside_the_encoding() {
    for length in 3usize..=66 {
        for distance in [1usize, 2, 512, 1023] {
            let word = ((length - 3) << 10) | distance;
            assert!(word >> 16 == 0, "length {length} distance {distance}");
            let b0 = (word >> 8) as u8;
            let b1 = word as u8;
            assert_eq!(u16::from_be_bytes([b0, b1]) & 0x3FF, distance as u16);
            assert_eq!((b0 >> 2) as usize + 3, length);
        }
    }
}

/// A version-1 LZSS stream must never be *longer* than a naive
/// all-literals encoding, and must be shorter on data that repeats.
#[test]
fn compressor_never_expands_repetitive_data() {
    let mut data = Vec::new();
    for round in 0..40u32 {
        for b in 0..64u32 {
            data.push((b * 7 + round % 3) as u8);
        }
    }
    let compressed = lzss_compress(&data);
    let all_literals = data.len() + data.len().div_ceil(8);
    assert!(
        compressed.len() <= all_literals,
        "compressed {} bytes from {} (all-literals would be {all_literals})",
        compressed.len(),
        data.len()
    );
    assert!(
        compressed.len() < data.len(),
        "2560 bytes of repeating data should compress below their own size"
    );
}

/// The compressor and the shipped decompressor are exact inverses.
///
/// This is the 1:1 round-trip the project requires of a decoder: anything the
/// compressor emits must decode back to the input, byte for byte, and the
/// decompressed length must be known up front (the container declares it).
#[test]
fn lzss_round_trips() {
    let mut cases: Vec<Vec<u8>> = vec![
        vec![],
        vec![0],
        vec![0xAB],
        vec![0xAB; 1],
        vec![0xAB; 66],
        vec![0xAB; 67],
        vec![0xAB; 1024],
        vec![0xAB; 1025],
        vec![0xAB; 3000],
        (0..=255u8).collect(),
        (0..1000u32).map(|i| (i % 7) as u8).collect(),
        (0..5000u32).map(|i| (i % 251) as u8).collect(),
    ];
    // Payloads that look like a picture: long transparent runs, a few colours.
    for (width, height, palette) in [(4usize, 3usize, 0x11u8), (7, 5, 0x22), (33, 9, 0x33)] {
        let mut payload = Vec::new();
        for y in 0..height {
            for x in 0..width {
                payload.push(if (x + y) % 3 == 0 { palette } else { 0 });
            }
        }
        cases.push(payload);
    }
    // A deterministic pseudo-random case: matches are rare, so this is the
    // case that punishes a token bound that is off by one.
    let mut state = 0x1234_5678u32;
    let mut noise = Vec::with_capacity(4096);
    for _ in 0..4096 {
        state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        noise.push((state >> 24) as u8);
    }
    cases.push(noise);

    for case in cases {
        let compressed = lzss_compress(&case);
        let expanded = v1::lzss::decompress(&compressed, case.len()).expect("decompress");
        assert_eq!(
            expanded, case,
            "round trip changed {} bytes",
            case.len()
        );
    }
}

/// Every match length the format can express, and every window distance at
/// the 1024-byte boundary, must survive.
#[test]
fn lzss_boundary_lengths_and_distances() {
    // Lengths 3..=66 are the whole (length - 3) range; 67 is not expressible.
    for length in [3usize, 4, 5, 32, 63, 64, 65, 66] {
        let mut case = vec![0u8; length];
        case.extend_from_slice(&vec![0xA5u8; length]);
        case.extend_from_slice(&vec![0u8; length]);
        let compressed = lzss_compress(&case);
        assert_eq!(
            v1::lzss::decompress(&compressed, case.len()).expect("decompress"),
            case,
            "length {length} did not round-trip"
        );
    }
    // A match that reaches exactly the 1024-byte window limit, and one past it.
    for offset in [1usize, 100, 1023, 1024, 1025, 2048] {
        let mut case = vec![0x5Au8; offset];
        case.extend_from_slice(&vec![0x5Au8; 600]);
        let compressed = lzss_compress(&case);
        assert_eq!(
            v1::lzss::decompress(&compressed, case.len()).expect("decompress"),
            case,
            "offset {offset} did not round-trip"
        );
    }
}

/// A stream that runs out before its declared length is an error, not a
/// truncated result.
#[test]
fn lzss_rejects_a_short_stream() {
    assert!(v1::lzss::decompress(&[0x01, 0x41], 3).is_err());
    // A control byte promising eight literals, with no bytes behind it.
    assert!(v1::lzss::decompress(&[0xFF], 1).is_err());
}

/// A declared length of zero means "stored as-is", so the decompressor must
/// not be reached at all.
#[test]
fn payload_of_a_plain_resource_is_the_stored_slice() {
    let container = synthetic_v1_container(&[Resource::plain("flat", 3, 4)]);
    let stored = container.payload(0);
    assert_eq!(
        v1::picture::payload(&container, 0).expect("payload"),
        stored,
        "an uncompressed payload must be copied through untouched"
    );
}

// ---------------------------------------------------------------------------
// A synthetic version-1 picture container.
// ---------------------------------------------------------------------------

/// How a resource's payload sits on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Storage {
    /// Stored as-is: the metadata declares an uncompressed size of **zero**,
    /// which is the first game's way of saying "there is nothing to expand".
    Plain,
    /// Stored as the first game's LZSS stream, with the expanded size declared
    /// so the decoder knows how much to produce.
    Compressed,
}

/// One resource of the synthetic container: its name, the dimensions the
/// metadata region declares, and the payload **after** any expansion.
struct Resource {
    name: &'static str,
    width: u16,
    height: u16,
    kind: u8,
    flags: u8,
    storage: Storage,
    /// The payload the game's runtime would hand to its blitter.
    expanded: Vec<u8>,
}

impl Resource {
    fn plain(name: &'static str, width: u16, height: u16) -> Self {
        Self {
            name,
            width,
            height,
            kind: 0,
            flags: 0,
            storage: Storage::Plain,
            expanded: flat_payload(width, height),
        }
    }

    fn compressed(name: &'static str, width: u16, height: u16) -> Self {
        Self {
            name,
            width,
            height,
            kind: 0,
            flags: 0,
            storage: Storage::Compressed,
            expanded: flat_payload(width, height),
        }
    }

    /// The bytes this resource occupies in the container, and the uncompressed
    /// size its metadata must declare for them.
    fn on_disk(&self) -> (Vec<u8>, u32) {
        match self.storage {
            Storage::Plain => (self.expanded.clone(), 0),
            Storage::Compressed => (lzss_compress(&self.expanded), self.expanded.len() as u32),
        }
    }
}

/// A flat 32bpp picture in the container's byte order: transparency, red,
/// green, blue, so the payload is exactly `4 * w * h` bytes.
fn flat_payload(width: u16, height: u16) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 * width as usize * height as usize);
    for y in 0..height {
        for x in 0..width {
            out.push(if (x + y) % 5 == 0 { 0xFF } else { 0x00 });
            out.push((x * 8) as u8);
            out.push((y * 8) as u8);
            out.push(0x40);
        }
    }
    out
}

/// What [`flat_payload`] must decode to: straight RGBA, so the transparency
/// byte moves to the end and is inverted.
fn flat_rgba(width: u16, height: u16) -> Vec<u8> {
    let payload = flat_payload(width, height);
    let mut out = Vec::with_capacity(payload.len());
    for pixel in payload.chunks_exact(4) {
        // (transparency, red, green, blue) -> (red, green, blue, 255 - transparency)
        out.push(pixel[1]);
        out.push(pixel[2]);
        out.push(pixel[3]);
        out.push(255 - pixel[0]);
    }
    out
}

/// Build a version-1 `MMFW Pictures` container holding `resources`.
///
/// The layout follows `docs/GAME1.md` §2 and §4.3: the count at `0x1E`, the
/// `N + 1` offsets from `0x20`, 32-byte names, then the metadata region
/// (handle array, `0x0007` marker, flags, kinds, dimensions, uncompressed
/// sizes, the unknown N-byte array, a palette count of zero and no bank),
/// then the payloads.
fn synthetic_v1_container(resources: &[Resource]) -> Container {
    let n = resources.len();
    let mut out: Vec<u8> = Vec::new();

    // Header.
    out.extend_from_slice(b"MMFW Pictures");
    out.resize(0x10, 0);
    out.extend_from_slice(b"MM");
    out.extend_from_slice(&1u16.to_be_bytes());
    out.extend_from_slice(&[0u8; 6]); // file id
    out.extend_from_slice(&[0u8; 4]); // timestamp, high half
    out.extend_from_slice(&(n as u16).to_be_bytes()); // 0x1E: the count

    // The offsets table starts at 0x20; its last entry is the file size, so it
    // is computed once the payloads are laid out.
    let payloads: Vec<Vec<u8>> = resources.iter().map(|r| r.on_disk().0).collect();
    let metadata_start = out.len()
        + 4 * (n + 1)
        + 2
        + 32 * n
        + 4 * n
        + 2
        + n
        + n
        + 4 * n
        + 4 * n
        + n
        + 2;
    let mut cursor = metadata_start;
    let mut table = Vec::with_capacity(n + 1);
    for payload in &payloads {
        table.push(cursor as u32);
        cursor += payload.len();
    }
    table.push(cursor as u32);
    for entry in table {
        out.extend_from_slice(&entry.to_be_bytes());
    }

    out.extend_from_slice(&[0u8; 2]); // padding after the offsets table

    // 32-byte names, NUL padded.
    for resource in resources {
        let mut field = [0u8; 32];
        field[..resource.name.len()].copy_from_slice(resource.name.as_bytes());
        out.extend_from_slice(&field);
    }

    // The version-1 metadata region.
    for _ in resources {
        out.extend_from_slice(&0u32.to_be_bytes()); // handle array
    }
    out.extend_from_slice(&7u16.to_be_bytes()); // picture metadata marker
    for resource in resources {
        out.push(resource.flags);
    }
    for resource in resources {
        out.push(resource.kind);
    }
    for resource in resources {
        out.extend_from_slice(&resource.height.to_be_bytes());
        out.extend_from_slice(&resource.width.to_be_bytes());
    }
    for resource in resources {
        let (_, uncompressed) = resource.on_disk();
        out.extend_from_slice(&uncompressed.to_be_bytes());
    }
    for _ in resources {
        out.push(0); // the unknown N-byte array
    }
    out.extend_from_slice(&0u16.to_be_bytes()); // palette count: no bank

    // The payloads, concatenated.
    for payload in &payloads {
        out.extend_from_slice(payload);
    }

    let tmp = tempdir("v1-synthetic");
    let path = tmp.join("synthetic.PIC");
    std::fs::write(&path, &out).expect("write synthetic container");
    Container::open(&path).expect("parse synthetic container")
}

fn tempdir(tag: &str) -> PathBuf {
    let base = std::env::var("MMF_TEST_TMP")
        .unwrap_or_else(|_| std::env::temp_dir().join("mmf-extract-tests").display().to_string());
    let dir = Path::new(&base).join(format!("{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

/// The container must parse as version 1, with the metadata region ending
/// exactly on the first payload.
#[test]
fn synthetic_container_parses_as_version_1() {
    let container = synthetic_v1_container(&[
        Resource::plain("flat", 4, 3),
        Resource::compressed("squashed", 4, 3),
    ]);
    assert_eq!(container.version, 1);
    assert_eq!(container.kind, ContainerKind::Pictures);
    assert_eq!(container.resource_count, 2);
    assert_eq!(container.names, vec!["flat".to_string(), "squashed".to_string()]);
    // A zero means "stored plain"; the compressed entry declares its expanded
    // size, which is the whole discriminator.
    assert_eq!(v1::uncompressed_len(&container, 0), 0);
    assert_eq!(
        v1::uncompressed_len(&container, 1),
        4 * 4 * 3,
        "the compressed resource must declare its expanded size"
    );
    assert_eq!(container.palettes.len(), 0);
}

/// The payload path expands what the container declares.
#[test]
fn payload_expands_only_what_is_declared() {
    let container = synthetic_v1_container(&[
        Resource::plain("flat", 4, 3),
        Resource::compressed("squashed", 7, 5),
    ]);
    for index in 0..2 {
        let expanded = v1::picture::payload(&container, index).expect("payload");
        let on_disk = std::fs::read(&container.path).expect("read container");
        let start = container.offsets[index] as usize;
        let end = container.offsets[index + 1] as usize;
        if v1::uncompressed_len(&container, index) == 0 {
            assert_eq!(
                expanded,
                &on_disk[start..end],
                "resource {index}: a stored-plain payload must come back as the container slice"
            );
        } else {
            assert_ne!(
                expanded,
                &on_disk[start..end],
                "resource {index}: a compressed payload must not be the stored bytes"
            );
        }
        if v1::uncompressed_len(&container, index) != 0 {
            assert_eq!(
                expanded.len(),
                v1::uncompressed_len(&container, index),
                "resource {index}: the expansion must be the declared length"
            );
        } else {
            assert_eq!(
                expanded.len(),
                end - start,
                "resource {index}: a stored-plain payload must keep its own length"
            );
        }
    }
}

/// `--original` must write the payload **after** the container's decompression
/// step, byte for byte, and must be byte-exact against the container for a
/// resource stored plain.
#[test]
fn original_mode_writes_the_expanded_payload() {
    let container = synthetic_v1_container(&[
        Resource::plain("flat", 4, 3),
        Resource::compressed("squashed", 7, 5),
        Resource::plain("flat2", 2, 2),
        Resource::compressed("squashed2", 3, 3),
    ]);
    let out_dir = tempdir("v1-original").join("Pictures").join("synthetic.PIC");
    let report = cli::extract_pictures_original(&container, &out_dir).expect("extract");
    assert_eq!(report.resources.len(), 4, "every resource must be written");
    assert!(report.failed.is_empty(), "{:?}", report.failed);

    let on_disk = std::fs::read(&container.path).expect("read container");
    for entry in &report.resources {
        let written = std::fs::read(&entry.png).expect("read .dat");
        let index = entry.index;
        let stored =
            &on_disk[container.offsets[index] as usize..container.offsets[index + 1] as usize];

        // A resource stored plain must come out as the container's own bytes.
        if v1::uncompressed_len(&container, index) == 0 {
            assert_eq!(
                written, stored,
                "resource {index} ('{}'): .dat must be the container's bytes verbatim",
                entry.name
            );
            continue;
        }

        // A compressed one must come out expanded, and be nothing else.
        assert_ne!(
            written, stored,
            "resource {index} ('{}'): .dat must not be the stored stream",
            entry.name
        );
        assert_eq!(
            written,
            v1::lzss::decompress(stored, v1::uncompressed_len(&container, index))
                .expect("decompress"),
            "resource {index} ('{}'): .dat must be the stream's exact expansion",
            entry.name
        );
        assert_eq!(
            written.len(),
            v1::uncompressed_len(&container, index),
            "resource {index} ('{}'): .dat length must be the declared size",
            entry.name
        );
        // The stream really is shorter than what it expands to, so this check
        // cannot pass by writing the payload through unchanged.
        assert!(
            stored.len() < written.len(),
            "resource {index} ('{}'): the stream should be shorter than its expansion",
            entry.name
        );
    }
}

/// A compressed payload must decode to the same pixels as the plain one, so
/// the `--original` bytes are also what the modern path reads.
#[test]
fn both_storage_forms_decode_to_the_same_pixels() {
    let container = synthetic_v1_container(&[
        Resource::plain("flat", 6, 4),
        Resource::compressed("squashed", 6, 4),
    ]);
    let pixels = flat_rgba(6, 4);
    for index in 0..2 {
        let decoded = v1::picture::decode(&container, index).expect("decode");
        assert_eq!(decoded.width, 6, "resource {index}");
        assert_eq!(decoded.height, 4, "resource {index}");
        assert_eq!(decoded.rgba, pixels, "resource {index}: pixels differ");
    }
}

/// The version-1 decoder used by the CLI must be the one that expands.
#[test]
fn the_cli_decoder_dispatch_is_version_aware() {
    let container = synthetic_v1_container(&[
        Resource::plain("flat", 4, 3),
        Resource::compressed("squashed", 4, 3),
    ]);
    let decoded = cli::picture_decoder(&container)(&container, 1).expect("decode");
    assert_eq!(decoded.rgba.len(), 4 * 4 * 3);
    // The payload function the `--original` path uses is the same one.
    let payload = cli::picture_payload(&container);
    assert_eq!(payload(&container, 1).expect("payload").len(), 4 * 4 * 3);
    assert_eq!(payload(&container, 0).expect("payload"), container.payload(0));
}

/// The shared (version 2/3) path must not have learned anything about version
/// 1: it must still read a payload straight through when there is no size to
/// undeclare.
#[test]
fn the_shared_payload_path_is_unaffected() {
    let container = synthetic_v1_container(&[Resource::compressed("squashed", 4, 3)]);
    // `picture::payload` is the version-2/3 rule and has no LZSS step, so for a
    // version-1 container that declares one it hands back the stored stream.
    assert_eq!(
        picture::payload(&container, 0).expect("shared payload"),
        container.payload(0)
    );
}
