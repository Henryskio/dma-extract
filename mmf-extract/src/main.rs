//! Extract assets from a Clickteam Multimedia Fusion 2 application.
//!
//! `mmf-extract <asset-dir> [--output DIR] [--types all,pictures,...]
//!              [--original]`
//!
//! By default every asset is converted to a modern, lossless format. With
//! `--original` the assets are instead written in the game's own on-disk
//! format, but still extracted resource by resource.

use anyhow::{Context, Result};
use clap::Parser;
use mmf_extract::cli;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "mmf-extract",
    about = "Extract the assets of a Multimedia Fusion 2 application",
    long_about = "Extracts the assets of a Clickteam Multimedia Fusion 2 application \
into modern, lossless formats (PNG for pictures, FLAC/WAV for sounds). \
With --original the resources are written in the game's own format instead."
)]
struct Args {
    /// Folder holding the unextracted assets (the .MMP/.MMA/.MMF/.MMS files).
    #[arg(value_name = "ASSET_DIR")]
    asset_dir: PathBuf,

    /// Where to write the extracted assets. Defaults to the current directory.
    #[arg(short, long, value_name = "DIR")]
    output: Option<PathBuf>,

    /// Comma-separated list of asset types to extract. "all" means everything.
    #[arg(short, long, value_name = "TYPES", default_value = "all", value_delimiter = ',')]
    types: Vec<String>,

    /// Extract into the game's original formats instead of modern ones.
    #[arg(long)]
    original: bool,

    /// Also write the untouched, still-compressed payload of every resource.
    #[arg(long)]
    raw: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let output = args.output.clone().unwrap_or_else(|| PathBuf::from("."));
    let selection = cli::AssetSelection::parse(&args.types)
        .with_context(|| format!("parsing --types {:?}", args.types))?;

    if !args.asset_dir.is_dir() {
        anyhow::bail!(
            "{}: not a directory (pass the folder holding the unextracted assets)",
            args.asset_dir.display()
        );
    }

    let summary = cli::run(&args.asset_dir, &output, &selection, args.original, args.raw)?;
    cli::print_summary(&summary);
    Ok(())
}
