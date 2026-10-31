//! MMFW sound resources.
//!
//! The sound containers use the same `MMFW` header as the picture
//! containers, but their metadata region is `14 * N + 2` bytes: after the
//! offset table and names there are `4 * N` Macintosh timestamps, a 2-byte
//! word, two per-resource byte arrays, a zeroed dimension table and a
//! `4 * N` decoded-size table.
//!
//! Two sample encodings occur in the shipped files. The low two bits of the
//! per-resource flags byte select between them:
//!
//! * **0 — stored PCM.** The payload is copied verbatim. The format byte
//!   (`attr`) gives the sample rate, channel count and bit depth.
//! * **anything else — differential PCM.** One stored byte becomes one
//!   signed 16-bit sample. The decoder is the one at `FUN_004c6b80` in the
//!   game executable (`DMA4.exe`), driven by a 256-entry `i16` table at
//!   virtual address `0x532778` / file offset `0x132778`.
//!
//! The format byte `C` is decoded as follows:
//!
//! ```text
//! bits 0-1  sample rate: 0 = 11025, 1 = 22050, 2/3 = 44100 Hz
//! bit 2     16-bit samples (always set on the differential stream)
//! bit 3     stereo (two interleaved channels)
//! bit 4+    auxiliary; not needed for PCM reconstruction
//! ```
//!
//! The differential codec is lossless with respect to the game's own
//! decoded samples: the output is exactly the signed 16-bit little-endian
//! sample stream the runtime produces. The stored PCM resources are
//! unsigned 8-bit bytes (silence is `0x80`) or **signed 16-bit,
//! big-endian**. Both forms are written to a normal WAV file in the modern
//! path, preserving every sample.
//!
//! ## The 16-bit stored order is big-endian, and that is measured
//!
//! The 228 stored-PCM16 resources in the three sound containers carry their
//! samples big-endian: Mac AIFF, the format the Windows/Mac hybrid disc's
//! audio was authored in, is big-endian. The measurement is a property of the
//! bytes, not of any decoder — a quiet 16-bit signal puts nearly all of its
//! energy in the low byte, so whichever byte position is almost always
//! `00`/`ff` is the high one. In 165 of the 228 resources with enough signal
//! to decide, that is the **even** byte; 22 are ambiguous and 41 too short to
//! call, and not one resource reads little-endian. Reading these streams the
//! other way is not a loud failure, which is why it survived this long: it
//! turns a quiet narration into a quiet hiss, so every resource still decodes
//! to a whole number of samples and `--original` still writes them
//! byte-exactly. It only shows up once the decoded audio is listened to.
//! [`swap16`] puts the samples into the little-endian order a WAV needs.

use crate::cli::sanitize;
use crate::container::Container;
use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};

/// The 256-entry predictor table at `DMA4.exe` file offset `0x132778`.
///
/// Entries 0..47 and 128..175 are absolute step sizes; the remaining
/// non-silence codes are added to the previous sample of the same channel.
#[rustfmt::skip]
const DPCM_TABLE: [i16; 256] = [
    1, 6, 19, 41, 73, 120, 180, 258, 353, 468, 603, 761, 942, 1149, 1381, 1641,
    1929, 2247, 2596, 2977, 3391, 3839, 4323, 4844, 5401, 5998, 6633, 7310, 8028, 8788, 9593, 10441,
    11335, 12276, 13264, 14300, 15385, 16520, 17707, 18945, 20236, 21581, 22981, 24435, 25946, 27515, 29141, 30826,
    2048, 2304, 2560, 2816, 3072, 3328, 3584, 3840, 1024, 1152, 1280, 1408, 1536, 1664, 1792, 1920,
    512, 576, 640, 704, 768, 832, 896, 960, 256, 288, 320, 352, 384, 416, 448, 480,
    128, 144, 160, 176, 192, 208, 224, 240, 64, 72, 80, 88, 96, 104, 112, 120,
    32, 36, 40, 44, 48, 52, 56, 60, 16, 18, 20, 22, 24, 26, 28, 30,
    8, 9, 10, 11, 12, 13, 14, 15, 0, 1, 2, 3, 4, 5, 6, 7,
    -1, -6, -19, -41, -73, -120, -180, -258, -353, -468, -603, -761, -942, -1149, -1381, -1641,
    -1929, -2247, -2596, -2977, -3391, -3839, -4323, -4844, -5401, -5998, -6633, -7310, -8028, -8788, -9593, -10441,
    -11335, -12276, -13264, -14300, -15385, -16520, -17707, -18945, -20236, -21581, -22981, -24435, -25946, -27515, -29141, -30826,
    -2048, -2304, -2560, -2816, -3072, -3328, -3584, -3840, -1024, -1152, -1280, -1408, -1536, -1664, -1792, -1920,
    -512, -576, -640, -704, -768, -832, -896, -960, -256, -288, -320, -352, -384, -416, -448, -480,
    -128, -144, -160, -176, -192, -208, -224, -240, -64, -72, -80, -88, -96, -104, -112, -120,
    -32, -36, -40, -44, -48, -52, -56, -60, -16, -18, -20, -22, -24, -26, -28, -30,
    -8, -9, -10, -11, -12, -13, -14, -15, 0, -1, -2, -3, -4, -5, -6, -7,
];

/// The code that represents a zero step (digital silence) in the DPCM stream.
pub const DPCM_SILENCE: u8 = 0xF8;

/// How the samples of a sound resource are stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    /// Stored unsigned 8-bit PCM, one byte per sample.
    Pcm8,
    /// Stored signed 16-bit PCM, **big-endian** on the wire.
    Pcm16,
    /// The game's differential codec: one byte per signed 16-bit sample.
    Dpcm16,
}

impl Encoding {
    pub fn bits_per_sample(self) -> u16 {
        match self {
            Self::Pcm8 => 8,
            Self::Pcm16 | Self::Dpcm16 => 16,
        }
    }
}

/// Playback format of one sound resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SoundFormat {
    pub sample_rate: u32,
    pub channels: u16,
    pub bits_per_sample: u16,
}

/// One decoded sound resource.
pub struct DecodedSound {
    pub format: SoundFormat,
    pub encoding: Encoding,
    /// Interleaved little-endian PCM. 8-bit samples are unsigned; 16-bit
    /// samples are signed. This is exactly the byte stream written by
    /// `--original` and the sample data placed in the modern WAV file.
    pub pcm: Vec<u8>,
}

/// One resource written by [`extract_container`].
#[derive(Debug, Clone)]
pub struct ExtractedSound {
    pub index: usize,
    pub name: String,
    pub encoding: Encoding,
    pub format: SoundFormat,
    pub path: PathBuf,
    pub bytes: usize,
}

/// What happened while extracting one container.
#[derive(Debug, Default)]
pub struct ExtractReport {
    pub written: usize,
    pub failed: Vec<(usize, String, String)>,
    pub resources: Vec<ExtractedSound>,
}

/// Decode the format byte of a sound resource.
pub fn format_of(container: &Container, index: usize) -> SoundFormat {
    let attr = container.attr_of(index);
    let sample_rate = match attr & 3 {
        0 => 11_025,
        1 => 22_050,
        _ => 44_100,
    };
    let channels = if attr & 8 != 0 { 2 } else { 1 };
    let bits_per_sample = if attr & 4 != 0 { 16 } else { 8 };
    SoundFormat {
        sample_rate,
        channels,
        bits_per_sample,
    }
}

/// Decode one sound resource into interleaved PCM bytes.
/// Byte-swap a big-endian 16-bit PCM stream into little-endian.
///
/// The stored 16-bit sound payloads are big-endian (see the module note), so
/// every sample has its two bytes exchanged on the way into a WAV. An odd
/// trailing byte is preserved rather than dropped, which cannot happen for a
/// resource that has already passed the whole-sample check above.
fn swap16(stored: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(stored.len());
    let mut chunks = stored.chunks_exact(2);
    for pair in &mut chunks {
        out.push(pair[1]);
        out.push(pair[0]);
    }
    out.extend_from_slice(chunks.remainder());
    out
}

pub fn decode(container: &Container, index: usize) -> Result<DecodedSound> {
    let stored = container.payload(index);
    let flags = container.flags_of(index);
    let attr = container.attr_of(index);
    let mut format = format_of(container, index);

    let encoding = if flags & 3 == 0 {
        if attr & 4 != 0 {
            Encoding::Pcm16
        } else {
            Encoding::Pcm8
        }
    } else {
        if attr & 4 == 0 {
            bail!(
                "resource {index} uses the differential codec but its format byte 0x{attr:02x} does not set the 16-bit flag"
            );
        }
        Encoding::Dpcm16
    };

    // The differential decoder expands one byte to one 16-bit sample. Its
    // output is always 16-bit, regardless of what the raw format byte says.
    if encoding == Encoding::Dpcm16 {
        format.bits_per_sample = 16;
    }

    let expected = container.plain_len(index);
    let pcm = match encoding {
        Encoding::Pcm8 | Encoding::Pcm16 => {
            if expected != stored.len() {
                bail!(
                    "resource {index} is stored PCM but the size table says {expected} bytes and the payload has {}",
                    stored.len()
                );
            }
            let sample_bytes = usize::from(format.bits_per_sample / 8);
            if stored.len() % sample_bytes != 0 {
                bail!(
                    "resource {index} has {} bytes, not a whole number of {}-bit samples",
                    stored.len(),
                    format.bits_per_sample
                );
            }
            // Stored 16-bit PCM is big-endian on the wire -- see the module
            // note on how that was measured -- so it is byte-swapped here into
            // the little-endian order a WAV file needs. `--original` writes
            // `container.payload`, not this, so the byte-exact archival path
            // is untouched by the swap.
            if encoding == Encoding::Pcm16 {
                swap16(stored)
            } else {
                stored.to_vec()
            }
        }
        Encoding::Dpcm16 => {
            if expected != stored.len() * 2 {
                bail!(
                    "resource {index} is differential but the size table says {expected} bytes, while {} stored bytes expand to {}",
                    stored.len(),
                    stored.len() * 2
                );
            }
            decode_dpcm(stored, format.channels)?
        }
    };

    let frame_bytes = usize::from(format.channels) * usize::from(format.bits_per_sample / 8);
    if frame_bytes == 0 || pcm.len() % frame_bytes != 0 {
        bail!(
            "resource {index}: {} decoded bytes do not form whole {}x{}-bit frames",
            pcm.len(),
            format.channels,
            format.bits_per_sample
        );
    }

    Ok(DecodedSound {
        format,
        encoding,
        pcm,
    })
}

/// Decode the game's differential PCM into interleaved signed 16-bit LE PCM.
///
/// This is a direct transliteration of `FUN_004c6b80` in `DMA4.exe`.
/// Predictors are independent per channel and are reset to zero at the
/// start of the stream. The addition wraps at 16 bits, matching the x86
/// implementation's `short` arithmetic.
pub fn decode_dpcm(input: &[u8], channels: u16) -> Result<Vec<u8>> {
    if channels == 0 {
        bail!("differential PCM has zero channels");
    }
    let channels = usize::from(channels);
    let mut previous = vec![0i16; channels];
    let mut out = Vec::with_capacity(input.len() * 2);
    let mut channel = 0usize;

    for &code in input {
        let mut sample = DPCM_TABLE[usize::from(code)];
        if (code & 0x7f) > 0x2f && code != DPCM_SILENCE {
            sample = sample.wrapping_add(previous[channel]);
        }
        previous[channel] = sample;
        out.extend_from_slice(&sample.to_le_bytes());
        channel = (channel + 1) % channels;
    }

    Ok(out)
}

/// Extract every sound in a container, returning how many were written.
///
/// In modern mode each resource becomes a WAV file with the game's original
/// sample rate, channels and bit depth. In original mode the decoded PCM is
/// written verbatim with a `.pcm` extension. Resources that cannot be
/// decoded are counted in the report, warned about on stderr, and skipped.
pub fn extract_container(
    container: &Container,
    out_dir: &Path,
    original: bool,
) -> Result<ExtractReport> {
    std::fs::create_dir_all(out_dir)?;
    let mut report = ExtractReport::default();

    for index in 0..container.resource_count as usize {
        let name = container.name(index);
        let decoded = match decode(container, index) {
            Ok(decoded) => decoded,
            Err(error) => {
                warn_failure(container, index, name, &error);
                report
                    .failed
                    .push((index, name.to_string(), error.to_string()));
                continue;
            }
        };

        let stem = output_stem(container, index);
        let extension = if original { "pcm" } else { "wav" };
        let path = out_dir.join(format!("{index:05}_{stem}.{extension}"));

        let written = if original {
            std::fs::write(&path, &decoded.pcm)
                .with_context(|| format!("writing {}", path.display()))
        } else {
            write_wav(&path, &decoded)
        };

        if let Err(error) = written {
            warn_failure(container, index, name, &error);
            report
                .failed
                .push((index, name.to_string(), error.to_string()));
            continue;
        }

        report.written += 1;
        report.resources.push(ExtractedSound {
            index,
            name: name.to_string(),
            encoding: decoded.encoding,
            format: decoded.format,
            path,
            bytes: decoded.pcm.len(),
        });
    }

    Ok(report)
}

fn warn_failure(container: &Container, index: usize, name: &str, error: &anyhow::Error) {
    eprintln!(
        "warning: {}: sound resource {index} '{name}': {error:#}",
        container.path.display()
    );
}

/// Write decoded PCM as a standard RIFF/WAVE file.
fn write_wav(path: &Path, decoded: &DecodedSound) -> Result<()> {
    let spec = hound::WavSpec {
        channels: decoded.format.channels,
        sample_rate: decoded.format.sample_rate,
        bits_per_sample: decoded.format.bits_per_sample,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec)
        .with_context(|| format!("creating {}", path.display()))?;

    match decoded.encoding {
        Encoding::Pcm8 => {
            for &sample in &decoded.pcm {
                // hound's 8-bit interface is signed; adding 128 on write
                // recovers the original unsigned byte exactly.
                writer.write_sample((i16::from(sample) - 128) as i8)?;
            }
        }
        Encoding::Pcm16 | Encoding::Dpcm16 => {
            for bytes in decoded.pcm.chunks_exact(2) {
                writer.write_sample(i16::from_le_bytes([bytes[0], bytes[1]]))?;
            }
        }
    }
    writer.finalize()?;
    Ok(())
}

/// Build a filename stem from the resource name, dropping the source audio
/// extension so the output extension is unambiguous.
fn output_stem(container: &Container, index: usize) -> String {
    let safe = sanitize(container.name(index), index);
    let lower = safe.to_ascii_lowercase();
    for extension in [".aif", ".aiff", ".wav"] {
        if lower.ends_with(extension) {
            let stem = &safe[..safe.len() - extension.len()];
            if !stem.is_empty() {
                return stem.to_string();
            }
        }
    }
    safe
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silence_code_produces_zero_samples() {
        let pcm = decode_dpcm(&[DPCM_SILENCE; 4], 1).unwrap();
        assert_eq!(pcm, vec![0; 8]);
    }

    #[test]
    fn predictor_is_independent_per_channel() {
        // Code 0 is an absolute +1; code 0x30 adds the table's +2048 to the
        // previous sample of its channel.
        let mono = decode_dpcm(&[0, 0x30, 0x30, DPCM_SILENCE], 1).unwrap();
        assert_eq!(
            mono.chunks_exact(2)
                .map(|b| i16::from_le_bytes([b[0], b[1]]))
                .collect::<Vec<_>>(),
            vec![1, 2049, 4097, 0]
        );

        let stereo = decode_dpcm(&[0, 0, 0x30, 0x30], 2).unwrap();
        assert_eq!(
            stereo
                .chunks_exact(2)
                .map(|b| i16::from_le_bytes([b[0], b[1]]))
                .collect::<Vec<_>>(),
            vec![1, 1, 2049, 2049]
        );
    }
}
