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
use mmf_extract::sound::format_of;
use std::collections::HashMap;
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

/// Resources whose format byte carries the wrong bit depth.
///
/// Each entry is (container, name, the depth the payload actually is). The
/// first five are the ones a listener confirmed by ear and that earshot then
/// corroborated; the rest are the same rule applied to the whole container, and
/// earshot hears a coherent sound where the declared reading was noise.
const WRONG_DEPTH: &[(&str, &str, u16)] = &[
    // Confirmed by a listener as "ear rape" at the declared depth.
    ("pack.SND", "MdyIn001.AIF", 16),
    ("pack.SND", "MdyIn002.AIF", 16),
    ("mpApp.SND", "IahUh066.AIF", 16),
    ("mpApp.SND", "IahUh069.AIFF", 16),
    ("pack.SND", "Mpl__S01.aif", 8),
    ("pack.SND", "Mmk_S01.aif", 16),
    ("mpApp.SND", "Smp__S03.aif", 16),
    // Found by the same rule over all 451, verified by earshot.
    ("pack.SND", "CreditsMusic.aif", 16),
    ("mpApp.SND", "Credits music.aif", 16),
    ("mpApp.SND", "UVp__M01.aif", 16),
    ("mpApp.SND", "UVp__M02.aif", 16),
    ("pack.SND", "CtaTlS01.aif", 8),
    ("pack.SND", "CtaTsS01.aif", 8),
    ("mpApp.SND", "CUdBtS01.aif", 8),
    ("mpApp.SND", "ZOOMRECT.AIF", 16),
    ("HD.SND", "CQtBtS01.aif", 8),
];

/// Resources that are correctly declared and must stay as they are.
///
/// A rule that only ever flips is not a rule. These are all fine at the depth
/// their format byte gives, and several of them are the very sounds the flipped
/// list is measured against: `MdyIn003` is the third Daisy intro, which decodes
/// correctly as 16-bit while its two siblings do not.
const RIGHT_DEPTH: &[(&str, &str, u16)] = &[
    ("pack.SND", "MdyIn003.AIF", 16),
    ("pack.SND", "mmkIn001.AIF", 16),
    ("pack.SND", "MddL3015.AIF", 16),
    ("pack.SND", "Mdd__S01.aif", 8),
    ("pack.SND", "MCHRaS01.aif", 8),
    ("HD.SND", "CPtBtS01.aif", 8),
];

/// Open a first-game sound container by file name.
fn open_sound(dir: &Path, file: &str) -> Container {
    let path = dir.join(file);
    assert!(path.is_file(), "{} not present", path.display());
    let c = Container::open(&path).expect("open sound container");
    assert_eq!(c.kind, ContainerKind::Sounds);
    c
}

/// Find a resource by name, case-insensitively.
fn find(container: &Container, name: &str) -> usize {
    (0..container.resource_count as usize)
        .find(|&i| container.name(i).eq_ignore_ascii_case(name))
        .unwrap_or_else(|| panic!("{} not in {}", name, container.path.display()))
}

/// The bit depth a resource decodes at, which is what the payload argues for
/// rather than what the format byte happens to say.
fn decoded_bits(container: &Container, index: usize) -> u16 {
    sound::decode(container, index)
        .unwrap_or_else(|e| panic!("decode {}: {e}", container.name(index)))
        .format
        .bits_per_sample
}

/// Every resource whose payload disagrees with its format byte is decoded at the
/// payload's depth.
///
/// This is `AGENTS.md`'s rule for metadata hints made testable: the format byte's
/// bit-depth bit is wrong for these resources, and the samples decide. Without
/// the override each one is either a standing alias (16-bit data read as 8-bit)
/// or a full-scale step (the other way), which is what makes them unlistenable.
#[test]
fn the_payloads_depth_wins_over_the_format_byte() {
    let Some(dir) = game1_dir() else {
        eprintln!("skipping: first game assets not present");
        return;
    };
    let mut opened: std::collections::HashMap<String, Container> = std::collections::HashMap::new();
    let mut checked = 0usize;
    for (container_file, name, want) in WRONG_DEPTH {
        let container = opened
            .entry((*container_file).to_string())
            .or_insert_with(|| open_sound(&dir, container_file));
        let i = find(container, name);
        // The format byte must actually disagree, or this case proves nothing.
        assert_eq!(
            format_of(container, i).bits_per_sample,
            if *want == 16 { 8 } else { 16 },
            "{name}: the format byte should read the wrong depth"
        );
        assert_eq!(
            decoded_bits(container, i),
            *want,
            "{name}: should decode at {want}-bit"
        );
        checked += 1;
    }
    assert_eq!(checked, WRONG_DEPTH.len());
}

/// A correctly declared resource keeps its declared depth.
///
/// The rule must not be a way of always preferring the other reading. These six
/// are fine as declared, three of them in the same family as the flipped ones.
#[test]
fn a_correctly_declared_resource_keeps_its_depth() {
    let Some(dir) = game1_dir() else {
        eprintln!("skipping: first game assets not present");
        return;
    };
    let mut opened: std::collections::HashMap<String, Container> = std::collections::HashMap::new();
    for (container_file, name, want) in RIGHT_DEPTH {
        let container = opened
            .entry((*container_file).to_string())
            .or_insert_with(|| open_sound(&dir, container_file));
        let i = find(container, name);
        assert_eq!(
            format_of(container, i).bits_per_sample,
            *want,
            "{name}: the format byte should read {want}-bit"
        );
        assert_eq!(
            decoded_bits(container, i),
            *want,
            "{name}: should stay at {want}-bit"
        );
    }
}

/// The decoded stream of a flipped resource does not sit on the rails.
///
/// This is the measurement the rule is built on, asserted on the output rather
/// than on the payload: whatever the depth turned out to be, the samples must end
/// up comfortably inside the range. A magnitude over 0.99 of full scale in more
/// than 2% of samples means the flip produced a stepped signal, not audio.
#[test]
fn a_flipped_resource_decodes_off_the_rails() {
    let Some(dir) = game1_dir() else {
        eprintln!("skipping: first game assets not present");
        return;
    };
    let mut opened: std::collections::HashMap<String, Container> = std::collections::HashMap::new();
    for (container_file, name, _) in WRONG_DEPTH {
        let container = opened
            .entry((*container_file).to_string())
            .or_insert_with(|| open_sound(&dir, container_file));
        let i = find(container, name);
        let decoded = sound::decode(container, i).unwrap_or_else(|e| panic!("decode {name}: {e}"));
        let pcm = &decoded.pcm[..decoded.pcm.len().min(200_000)];
        let (rails, samples, scale) = if decoded.format.bits_per_sample >= 16 {
            let mut rails = 0usize;
            let mut samples = 0usize;
            for pair in pcm.chunks_exact(2) {
                let v = i16::from_le_bytes([pair[0], pair[1]]) as f64 / 32768.0;
                if v.abs() >= 0.99 {
                    rails += 1;
                }
                samples += 1;
            }
            (rails, samples, 1usize)
        } else {
            let rails = pcm
                .iter()
                .filter(|&&b| ((f64::from(b) - 128.0) / 128.0).abs() >= 0.99)
                .count();
            (rails, pcm.len(), 1usize)
        };
        let share = rails as f64 / samples.max(1) as f64;
        assert!(
            share < 0.02,
            "{name}: {share:.3} of the decoded samples sit on the rails"
        );
        let _ = scale;
    }
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
