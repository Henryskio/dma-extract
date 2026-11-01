//! Dump what the 3D game's Films container holds, chunk by chunk.
//!
//! Scratch bin. `src/film.rs` reports `bad bounds top=258 left=772 bottom=0
//! right=0` on film 0 of `DMA3D.MMF`, so the shared metadata layout does not
//! fit this build. This prints the container's own tables and the first bytes
//! of the payload so the layout can be measured rather than assumed.

use clap::Parser;
use mmf_extract::container::{Container, ContainerKind};

#[derive(Parser, Debug)]
#[command(name = "filmprobe3d", about = "Dump the 3D game's Films container")]
struct Args {
    /// The Films container to read.
    #[arg(value_name = "CONTAINER")]
    path: std::path::PathBuf,
}

fn main() {
    let args = Args::parse();
    let data = std::fs::read(&args.path).expect("read container");
    let container = Container::parse(&args.path, data).expect("open Films");
    let n = container.resource_count as usize;
    println!("kind={:?} version={} N={n}", container.kind, container.version);

    // The metadata region the shared parser walks: names_end + 4*N is where it
    // reads the marker, and it solves record_len/palette_count from there to
    // the first payload.
    let region_start = container.names_end + 4 * n;
    let payload_start = container.offsets[0] as usize;
    println!(
        "\nnames_end={:#x}  block(4N later)={:#x}  payload_start={:#x}  region={} bytes",
        container.names_end,
        region_start,
        payload_start,
        payload_start - region_start
    );
    let region = &container.data[region_start..payload_start];
    println!("  the region, {} bytes:", region.len());
    for k in (0..region.len()).step_by(16) {
        let row = &region[k..(k + 16).min(region.len())];
        println!(
            "    {:#06x}  {}  {}",
            region_start + k,
            row.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" "),
            row.iter().map(|b| if b.is_ascii_graphic() { *b as char } else { '.' }).collect::<String>()
        );
    }

    for index in 0..n {
        let payload = container.payload(index);
        println!("\n=== #{index} {} len={} ===", container.name(index), payload.len());
        println!("  kind={:?}", container.kind);
        // The container's per-resource tables, which is where the shared
        // decoder looks for the film's dimensions.
        let _ = ContainerKind::Films;
        println!("  first 64 bytes of the metadata region view:");
        println!("    {}", payload.iter().take(64).map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" "));
        println!("  as u16: {}", payload.chunks_exact(2).take(24).map(|c| u16::from_le_bytes([c[0], c[1]])).map(|v| v.to_string()).collect::<Vec<_>>().join(" "));
        println!("  as u32: {}", payload.chunks_exact(4).take(16).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).map(|v| v.to_string()).collect::<Vec<_>>().join(" "));
        println!("  as u32be: {}", payload.chunks_exact(4).take(16).map(|c| u32::from_be_bytes([c[0], c[1], c[2], c[3]])).map(|v| v.to_string()).collect::<Vec<_>>().join(" "));
    }
}
