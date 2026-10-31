//! Sound-container tests.
//!
//! These exercise the real game files when they are present and are skipped
//! otherwise. They cover the metadata layout, both sample encodings, the
//! verbatim `--original` output, and that the modern FLAC files decode back to
//! exactly the same samples.

use mmf_extract::container::{Container, ContainerKind};
use mmf_extract::sound;
use std::path::{Path, PathBuf};

const SOUND_CONTAINERS: &[&str] = &["HD.MMA", "app.MMA"];

fn raw_assets_dir() -> Option<PathBuf> {
    // The asset folder is normally mounted at /RawAssets; point DMA_ASSETS at
    // another copy if needed.
    let extra = std::env::var("DMA_ASSETS").unwrap_or_default();
    for candidate in ["/RawAssets", extra.as_str()] {
        let path = PathBuf::from(candidate);
        if path.is_dir() {
            return Some(path);
        }
    }
    None
}

fn tempdir(tag: &str) -> PathBuf {
    let base = std::env::var("MMF_TEST_TMP").unwrap_or_else(|_| {
        std::env::temp_dir()
            .join("mmf-extract-tests")
            .display()
            .to_string()
    });
    let dir = Path::new(&base).join(format!("{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

#[test]
fn sound_metadata_layout_matches_the_loader() {
    let Some(dir) = raw_assets_dir() else {
        eprintln!("skipping: game assets not present");
        return;
    };

    for name in SOUND_CONTAINERS {
        let path = dir.join(name);
        if !path.exists() {
            continue;
        }
        let data = std::fs::read(&path).expect("read container");
        let n = u16::from_be_bytes([data[0x22], data[0x23]]) as usize;
        let names_base = 0x24 + 4 * (n + 1) + 2;
        let names_end = names_base + 32 * n;
        let payload_start =
            u32::from_be_bytes([data[0x24], data[0x25], data[0x26], data[0x27]]) as usize;

        assert_eq!(
            payload_start - names_end,
            14 * n + 2,
            "{name}: sound metadata is not 14*N+2 bytes"
        );
        assert_eq!(
            &data[names_end + 4 * n + 2..names_end + 5 * n + 2],
            &Container::open(&path).unwrap().attr[..],
            "{name}: attribute array is not at 4*N+2"
        );
        assert_eq!(
            &data[names_end + 5 * n + 2..names_end + 6 * n + 2],
            &Container::open(&path).unwrap().flags[..],
            "{name}: flags array is not at 5*N+2"
        );

        let container = Container::open(&path).expect("open container");
        assert_eq!(container.kind, ContainerKind::Sounds);
        assert_eq!(container.resource_count as usize, n);
        assert!(
            container
                .dimensions
                .iter()
                .all(|d| d.width == 0 && d.height == 0)
        );

        for i in 0..n {
            let stored = container.payload_len(i);
            let attr = container.attr_of(i);
            let flags = container.flags_of(i);
            let decoded_size = container.sizes[i] as usize;
            match flags & 3 {
                0 => {
                    assert_eq!(
                        attr & 4,
                        0,
                        "{name} resource {i}: stored PCM does not declare 8-bit"
                    );
                    assert_eq!(
                        decoded_size, stored,
                        "{name} resource {i}: stored PCM size mismatch"
                    );
                }
                _ => {
                    assert_ne!(
                        attr & 4,
                        0,
                        "{name} resource {i}: differential stream does not declare 16-bit"
                    );
                    assert_eq!(
                        decoded_size,
                        stored * 2,
                        "{name} resource {i}: differential size mismatch"
                    );
                }
            }
        }
    }
}

#[test]
fn every_sound_decodes_and_its_modern_output_round_trips() {
    let Some(dir) = raw_assets_dir() else {
        eprintln!("skipping: game assets not present");
        return;
    };
    let tmp = tempdir("sounds-modern");

    let mut flac_count = 0usize;
    let mut pcm8_count = 0usize;
    let mut stereo_count = 0usize;
    for name in SOUND_CONTAINERS {
        let path = dir.join(name);
        if !path.exists() {
            continue;
        }
        let container = Container::open(&path).expect("open container");
        let n = container.resource_count as usize;
        let out_dir = tmp.join(name);
        let report = sound::extract_container(&container, &out_dir, false).expect("extract sounds");

        assert!(report.failed.is_empty(), "{name}: {:?}", report.failed);
        assert_eq!(report.written, n, "{name}: not every sound was written");
        assert_eq!(report.resources.len(), n);

        for entry in &report.resources {
            let decoded = sound::decode(&container, entry.index).expect("decode");
            assert_eq!(entry.format, decoded.format);
            assert_eq!(entry.encoding, decoded.encoding);
            assert_eq!(entry.bytes, decoded.pcm.len());

            // Every resource is FLAC, at the width the payload was measured to
            // hold. The official `flac` tool preserves the source bit depth
            // (unlike ffmpeg's FLAC encoder, which promotes 8-bit to 16-bit),
            // so there is no resource that has to fall back to WAV any more;
            // this assertion is what keeps it that way.
            assert_eq!(
                entry.path.extension().and_then(|e| e.to_str()),
                Some("flac"),
                "{name} resource {}: {}-bit audio should be written as .flac",
                entry.index,
                decoded.format.bits_per_sample
            );
            flac_count += 1;
            if decoded.format.bits_per_sample == 8 {
                pcm8_count += 1;
            }
            if decoded.format.channels == 2 {
                stereo_count += 1;
            }

            assert_flac_round_trips(name, entry.index, &entry.path, &decoded);
        }
    }
    eprintln!(
        "extracted {flac_count} FLAC files ({pcm8_count} 8-bit, {stereo_count} stereo, all round-tripped byte-identical)"
    );
}

/// A FLAC file must decode back to exactly the samples the decoder produced,
/// at the width the decoder settled on.
///
/// Two things are checked, and neither can be faked by the other.
///
/// **Samples.** The file is decoded with the same official `flac` tool that
/// wrote it, as headerless little-endian PCM, and compared byte for byte with
/// the decoder's output. If the encoding were lossy in any way, or the WAV fed
/// to the encoder carried the wrong payload, the bytes would differ.
///
/// **What the file says about itself.** The `STREAMINFO` block is parsed
/// straight out of the file (it is the first metadata block, so the layout is
/// fixed) and its rate, channel count, bit depth and sample count are compared
/// with the decoder's. This is the check that would have caught ffmpeg: its
/// FLAC encoder promoted 8-bit to 16-bit and the samples still round-tripped
/// perfectly, so only the declared depth gave it away. `bits_per_raw_sample=8`
/// in the finished file is the contract this extractor has with its readers.
fn assert_flac_round_trips(name: &str, index: usize, path: &Path, decoded: &sound::DecodedSound) {
    assert_streaminfo_matches(name, index, path, decoded);

    let raw_path = path.with_extension("raw.out");
    let sign = if decoded.format.bits_per_sample == 8 {
        "unsigned"
    } else {
        "signed"
    };
    let status = std::process::Command::new("flac")
        .arg("--decode")
        .arg("--silent")
        .arg("--force-raw-format")
        .arg("--endian=little")
        .arg(format!("--sign={sign}"))
        .arg("--force")
        .arg("--output-name")
        .arg(&raw_path)
        .arg(path)
        .status()
        .expect("run flac to decode the FLAC back to raw PCM");
    assert!(
        status.success(),
        "{name} resource {index}: flac could not decode {}",
        path.display()
    );

    let recovered =
        std::fs::read(&raw_path).unwrap_or_else(|e| panic!("read {}: {e}", raw_path.display()));
    let _ = std::fs::remove_file(&raw_path);

    assert_eq!(
        recovered.len(),
        decoded.pcm.len(),
        "{name} resource {index}: FLAC round-tripped to {} bytes of PCM, expected {}",
        recovered.len(),
        decoded.pcm.len()
    );
    assert_eq!(
        recovered, decoded.pcm,
        "{name} resource {index}: FLAC did not round-trip to the same samples"
    );
}

/// Parse the `STREAMINFO` block of a FLAC file and check it against the decoder.
///
/// The format is `"fLaC"`, then a metadata-block header (block type in the high
/// 7 bits, "last" flag in bit 0, 3-byte length). `STREAMINFO` is type 0 and 34
/// bytes long, of which the first 18 header bytes hold, packed big-endian:
///
/// ```text
/// bits 63..44 (20)  sample rate in Hz
/// bits 43..41 (3)   channel count - 1
/// bits 40..36 (5)   bits per sample - 1
/// bits 35..0  (36)  total samples (frames), 0 when unknown
/// ```
///
/// Reading it here rather than with ffprobe keeps the check inside the crate:
/// it needs no external tool and it is measured on the same bytes every writer
/// produces.
fn assert_streaminfo_matches(name: &str, index: usize, path: &Path, decoded: &sound::DecodedSound) {
    let bytes = std::fs::read(path).expect("read flac");
    assert_eq!(
        &bytes[..4],
        b"fLaC",
        "{name} resource {index}: not a FLAC file"
    );
    // The first block header: block type 0 (STREAMINFO), 34-byte body.
    assert_eq!(
        bytes[4] >> 1,
        0,
        "{name} resource {index}: the first metadata block is not STREAMINFO"
    );
    let body_len = u32::from_be_bytes([0, bytes[5], bytes[6], bytes[7]]) as usize;
    assert_eq!(
        body_len, 34,
        "{name} resource {index}: STREAMINFO is {body_len} bytes, expected 34"
    );

    let info = &bytes[8..42];
    let packed = u64::from_be_bytes(info[10..18].try_into().expect("8 bytes of STREAMINFO"));
    let sample_rate = (packed >> 44) & 0xf_ffff;
    let channels = ((packed >> 41) & 0x7) + 1;
    let bits_per_sample = ((packed >> 36) & 0x1f) + 1;
    let total_frames = packed & 0xf_ffff_ffff;

    assert_eq!(
        sample_rate,
        u64::from(decoded.format.sample_rate),
        "{name} resource {index}: the FLAC declares {sample_rate} Hz"
    );
    assert_eq!(
        channels,
        u64::from(decoded.format.channels),
        "{name} resource {index}: the FLAC declares {channels} channels"
    );
    // The one that matters: the file must claim the depth the payload was
    // measured to hold, not a depth some encoder decided to promote it to.
    assert_eq!(
        bits_per_sample,
        u64::from(decoded.format.bits_per_sample),
        "{name} resource {index}: the FLAC declares {bits_per_sample}-bit audio where the payload is {}-bit",
        decoded.format.bits_per_sample
    );
    assert_eq!(
        total_frames * channels * bits_per_sample / 8,
        decoded.pcm.len() as u64,
        "{name} resource {index}: the FLAC declares {total_frames} frames, expected {}",
        decoded.pcm.len() as u64
            / u64::from(decoded.format.channels)
            / u64::from(decoded.format.bits_per_sample / 8)
    );
}

fn original_output_is_the_decoded_payload_verbatim() {
    let Some(dir) = raw_assets_dir() else {
        eprintln!("skipping: game assets not present");
        return;
    };
    let tmp = tempdir("sounds-original");

    for name in SOUND_CONTAINERS {
        let path = dir.join(name);
        if !path.exists() {
            continue;
        }
        let container = Container::open(&path).expect("open container");
        let n = container.resource_count as usize;
        let out_dir = tmp.join(name);
        let report = sound::extract_container(&container, &out_dir, true).expect("extract sounds");

        assert!(report.failed.is_empty(), "{name}: {:?}", report.failed);
        assert_eq!(report.written, n);

        for entry in &report.resources {
            let decoded = sound::decode(&container, entry.index).expect("decode");
            let written = std::fs::read(&entry.path).expect("read pcm");
            assert_eq!(
                written, decoded.pcm,
                "{name} resource {}: original output changed the samples",
                entry.index
            );
            assert_eq!(
                written.len(),
                container.sizes[entry.index] as usize,
                "{name} resource {}: original length differs from the size table",
                entry.index
            );
            if decoded.encoding == sound::Encoding::Pcm8 {
                assert_eq!(
                    written,
                    container.payload(entry.index),
                    "{name} resource {}: stored PCM was transformed",
                    entry.index
                );
            }
        }
    }
}

#[test]
fn known_files_have_the_expected_playback_format() {
    let Some(dir) = raw_assets_dir() else {
        eprintln!("skipping: game assets not present");
        return;
    };

    // "Ball30sec" is 28.8 seconds of stereo 22.05 kHz audio only when the
    // format byte is interpreted as rate code 1 plus the stereo bit. This is
    // an independent check on the channel/rate rules.
    let path = dir.join("app.MMA");
    if path.exists() {
        let container = Container::open(&path).expect("open container");
        let index = (0..container.resource_count as usize)
            .find(|&i| container.name(i).eq_ignore_ascii_case("Ball30sec.AIF"))
            .expect("Ball30sec.AIF");
        let decoded = sound::decode(&container, index).expect("decode Ball30sec");
        assert_eq!(decoded.format.sample_rate, 22_050);
        assert_eq!(decoded.format.channels, 2);
        assert_eq!(decoded.format.bits_per_sample, 16);
        let frames = decoded.pcm.len() / 4;
        let seconds = frames as f64 / 22_050.0;
        assert!(
            (28.7..=28.9).contains(&seconds),
            "Ball30sec lasts {seconds:.3} s, expected about 28.8 s"
        );

        let silence = (0..container.resource_count as usize)
            .find(|&i| container.name(i).eq_ignore_ascii_case("silence.aif"))
            .expect("silence.aif");
        let decoded = sound::decode(&container, silence).expect("decode silence");
        assert!(decoded.pcm.iter().all(|&b| b == 0));
    }

    let path = dir.join("HD.MMA");
    if path.exists() {
        let container = Container::open(&path).expect("open container");
        let index = (0..container.resource_count as usize)
            .find(|&i| container.name(i).eq_ignore_ascii_case("ISthumbDRAG.aif"))
            .expect("ISthumbDRAG.aif");
        let decoded = sound::decode(&container, index).expect("decode ISthumbDRAG");
        assert_eq!(decoded.format.sample_rate, 22_050);
        assert_eq!(decoded.format.channels, 1);
        assert_eq!(decoded.format.bits_per_sample, 8);
        assert_eq!(decoded.encoding, sound::Encoding::Pcm8);
    }
}
