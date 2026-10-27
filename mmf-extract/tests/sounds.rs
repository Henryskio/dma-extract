//! Sound-container tests.
//!
//! These exercise the real game files when they are present and are skipped
//! otherwise. They cover the metadata layout, both sample encodings, the
//! verbatim `--original` output, and that the modern WAV files decode back to
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
fn every_sound_decodes_and_wav_round_trips() {
    let Some(dir) = raw_assets_dir() else {
        eprintln!("skipping: game assets not present");
        return;
    };
    let tmp = tempdir("sounds-wav");

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

            let mut reader = hound::WavReader::open(&entry.path).expect("open wav");
            let spec = reader.spec();
            assert_eq!(spec.sample_rate, decoded.format.sample_rate);
            assert_eq!(spec.channels, decoded.format.channels);
            assert_eq!(spec.bits_per_sample, decoded.format.bits_per_sample);

            if decoded.encoding == sound::Encoding::Pcm8 {
                let samples: Vec<i8> = reader.samples::<i8>().map(Result::unwrap).collect();
                assert_eq!(samples.len(), decoded.pcm.len());
                for (&sample, &stored) in samples.iter().zip(&decoded.pcm) {
                    assert_eq!(
                        (i16::from(sample) + 128) as u8,
                        stored,
                        "{name} resource {}: WAV sample changed",
                        entry.index
                    );
                }
            } else {
                let samples: Vec<i16> = reader.samples::<i16>().map(Result::unwrap).collect();
                assert_eq!(samples.len() * 2, decoded.pcm.len());
                for (&sample, bytes) in samples.iter().zip(decoded.pcm.chunks_exact(2)) {
                    assert_eq!(
                        sample,
                        i16::from_le_bytes([bytes[0], bytes[1]]),
                        "{name} resource {}: WAV sample changed",
                        entry.index
                    );
                }
            }
        }
    }
}

#[test]
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
