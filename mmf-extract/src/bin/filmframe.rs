//! Dump the 3D game's film key frames as raw 8bpp index buffers.
//!
//! Scratch bin. `DMA3D.MMF`'s three films fail in `src/film.rs` with
//! `bad bounds`, so the per-film record layout of this build is not the third
//! game's. The chunk stream itself parses fine, and each key-frame chunk
//! declares the byte count it decodes to, so the *buffer size* is already
//! known: 480000, 252192 and 307200 for the three films.
//!
//! What is not known is the width and the height inside that buffer, and the
//! wrong width concatenates true rows into one another (`docs/DMA3D.md` §4.2
//! measured exactly this for the textures). So this writes each decoded frame
//! to a file, unmodified, for a renderer to look at.
//!
//! ```text
//! filmframe <DMA3D.MMF> [NAME] [--out DIR]
//! ```

use clap::Parser;
use mmf_extract::container::Container;
use mmf_extract::lz77;

#[derive(Parser, Debug)]
#[command(
    name = "filmframe",
    about = "Decode each 3D film's key frames to raw 8bpp index buffers"
)]
struct Args {
    /// The Films container to read.
    #[arg(value_name = "CONTAINER")]
    path: std::path::PathBuf,

    /// Only payloads whose resource name contains this text.
    #[arg(value_name = "NAME", default_value = "")]
    name: String,

    /// Where to write the raw buffers.
    #[arg(long, value_name = "DIR", default_value = "/tmp/filmframes")]
    out: std::path::PathBuf,
}

/// Key-frame chunk type. Version 3 writes `0x0A`; the third game's shipped
/// films are the only other measurement, and they use the same value.
const TYPE_FULL_FRAME: u16 = 0x0A;

fn main() {
    let args = Args::parse();
    let data = std::fs::read(&args.path).expect("read container");
    let container = Container::parse(&args.path, data).expect("open Films");
    std::fs::create_dir_all(&args.out).expect("create the output directory");

    let n = container.resource_count as usize;
    for index in 0..n {
        let name = container.name(index);
        if !args.name.is_empty() && !name.contains(&args.name) {
            continue;
        }
        let payload = container.payload(index);
        let mut offset = 0usize;
        let mut frames = 0usize;
        while offset + 8 <= payload.len() {
            let kind = u16::from_be_bytes([payload[offset + 2], payload[offset + 3]]);
            let len = u32::from_be_bytes([
                payload[offset + 4],
                payload[offset + 5],
                payload[offset + 6],
                payload[offset + 7],
            ]) as usize;
            let start = offset + 8;
            let end = start + len;
            if end > payload.len() {
                eprintln!("{name}: chunk at {offset:#x} runs past the payload");
                break;
            }
            if kind == TYPE_FULL_FRAME && len >= 8 {
                let lz77_len = u32::from_be_bytes([
                    payload[start],
                    payload[start + 1],
                    payload[start + 2],
                    payload[start + 3],
                ]) as usize;
                let decoded_len = u32::from_be_bytes([
                    payload[start + 4],
                    payload[start + 5],
                    payload[start + 6],
                    payload[start + 7],
                ]) as usize;
                match lz77::decompress(&payload[start + 8..start + 8 + lz77_len]) {
                    Ok(decoded) => {
                        let stem = name.trim_end_matches(".flm");
                        let path = args
                            .out
                            .join(format!("{index:05}_{stem}_{frames:03}.raw"));
                        std::fs::write(&path, &decoded).expect("write the frame");
                        println!(
                            "{name}: frame {frames} at {offset:#x} -> {} bytes (header says \
                             {decoded_len}) written to {}",
                            decoded.len(),
                            path.display()
                        );
                        frames += 1;
                    }
                    Err(e) => {
                        println!(
                            "{name}: frame {frames} at {offset:#x}: LZ77 failed: {e}"
                        );
                        break;
                    }
                }
            }
            offset = end;
        }
        if frames == 0 {
            println!("{name}: no key frame decoded");
        }
    }
}
