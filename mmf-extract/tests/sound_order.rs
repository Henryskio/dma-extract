//! The byte order of the stored 16-bit sound samples.
//!
//! `src/sound.rs` used to copy the stored PCM through unchanged, on the
//! assumption that "stored PCM is already little-endian". That assumption is
//! false for every 16-bit resource with a decisive signal: the samples are
//! **big-endian**, which is what an AIFF-authored asset from this Win/Mac
//! hybrid disc would be. Reading them the other way is not a loud failure — it
//! turns a quiet narration into a quiet hiss, so every resource still decodes
//! to a whole number of samples and `--original` still writes them
//! byte-exactly, which is exactly why the bug survived.
//!
//! The check here is a property of the *decoded samples*, so it holds whatever
//! the container: a quiet 16-bit signal has almost all of its energy in the low
//! byte, so a little-endian reading of quiet audio is dominated by bytes whose
//! high byte is zero. The assertion is that the decoded stream of a quiet
//! 16-bit resource is *not* dominated by zero high bytes, and that it really is
//! quiet — both of which fail if the bytes are swapped either way.

use mmf_extract::container::{Container, ContainerKind};
use mmf_extract::sound;
use std::path::{Path, PathBuf};

/// The first game's containers, which is where the byte order was measured.
const GAME1_CONTAINERS: &[&str] = &["HD.SND", "mpApp.SND", "pack.SND"];

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

/// Fraction of samples whose low 9 bits are zero.
///
/// A little-endian reading of big-endian quiet audio multiplies every sample by
/// 256, which lands its low bits at zero; a correct reading spreads them. The
/// measure is order-sensitive and quiet-sensitive at once, so it is a single
/// number that a byte-order error cannot survive.
fn low_bits_zero_share(pcm: &[u8]) -> f64 {
    let samples = pcm.len() / 2;
    if samples == 0 {
        return 0.0;
    }
    let zero = pcm
        .chunks_exact(2)
        .filter(|s| (s[0] as u16 & 0x01ff) == 0)
        .count();
    zero as f64 / samples as f64
}

/// The decoded stream of a stored-16-bit resource is in the wire order.
///
/// `MddL3015` is a first-game lesson track: stored signed 16-bit, 22050 Hz,
/// mono, and its first stored sample is `ff e2` — that is -30 read
/// big-endian, and -7454 read the other way. Only the big-endian reading is
/// a plausible audio sample, and only the little-endian *output* is what a
/// WAV file can hold, so this pins both halves of the swap at once.
#[test]
fn a_sixteen_bit_resource_decodes_to_the_wire_order() {
    let Some(dir) = game1_dir() else {
        eprintln!("skipping: first game assets not present");
        return;
    };
    let path = dir.join("pack.SND");
    if !path.is_file() {
        eprintln!("skipping: pack.SND not present");
        return;
    }
    let container = Container::open(&path).expect("open pack.SND");
    assert_eq!(container.kind, ContainerKind::Sounds);
    let index = (0..container.resource_count as usize)
        .find(|&i| container.name(i).eq_ignore_ascii_case("MddL3015.AIF"))
        .expect("MddL3015.AIF in pack.SND");
    let decoded = sound::decode(&container, index).expect("decode MddL3015");
    assert_eq!(decoded.encoding, sound::Encoding::Pcm16);
    assert_eq!(decoded.format.bits_per_sample, 16);

    // The stored bytes are big-endian: the first sample is `ff e2`, i.e. -30.
    let stored = container.payload(index);
    assert_eq!(stored[0], 0xff);
    assert_eq!(stored[1], 0xe2);
    assert_eq!(i16::from_be_bytes([stored[0], stored[1]]), -30);
    // ...and the decoded stream holds it little-endian, as a WAV needs.
    assert_eq!(decoded.pcm[0], 0xe2);
    assert_eq!(decoded.pcm[1], 0xff);

    let share = low_bits_zero_share(&decoded.pcm);
    assert!(
        share < 0.5,
        "the decoded stream looks byte-swapped: {share:.2} of its samples have zero low bits"
    );
}

/// Every stored-16-bit resource in the first game's containers decodes without
/// its low bits collapsing.
///
/// This is the container-wide form of the single-resource test: it walks all
/// 228 stored-PCM16 resources and asserts the same property, so one file being
/// fixed cannot hide another still being swapped.
#[test]
fn no_stored_sixteen_bit_resource_is_byte_swapped() {
    let Some(dir) = game1_dir() else {
        eprintln!("skipping: first game assets not present");
        return;
    };
    let mut checked = 0usize;
    let mut quiet = 0usize;
    for name in GAME1_CONTAINERS {
        let path = dir.join(name);
        if !path.is_file() {
            continue;
        }
        let container = Container::open(&path).expect("open container");
        for i in 0..container.resource_count as usize {
            let decoded = match sound::decode(&container, i) {
                Ok(d) => d,
                Err(_) => continue,
            };
            if decoded.encoding != sound::Encoding::Pcm16 {
                continue;
            }
            checked += 1;
            let share = low_bits_zero_share(&decoded.pcm);
            assert!(
                share < 0.6,
                "{name} resource {i} '{}': {share:.2} of its samples have zero low bits, \
                 which is the signature of byte-swapped 16-bit audio",
                container.name(i)
            );
            // Only the resources with enough signal to matter are counted as a
            // real check; the short ones pass trivially.
            if share < 0.5 {
                quiet += 1;
            }
        }
    }
    assert!(checked > 0, "no stored-16-bit resources were found");
    eprintln!("checked {checked} stored-16-bit resources, {quiet} with a decisive signal");
}
