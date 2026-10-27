//! Dump every resource's decompressed payload to a directory.
//!
//! `dump <container> <outdir>` writes one `<index>_<name>.bin` per resource
//! holding the payload after LZ77 decompression, plus a `names.tsv` index.

use anyhow::{Context, Result};
use mmf_extract::container::Container;
use mmf_extract::lz77;
use std::path::Path;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: mmf-extract dump <container> <outdir>");
        return Ok(());
    }
    let path = &args[1];
    let outdir = Path::new(&args[2]);
    std::fs::create_dir_all(outdir)?;

    let c = Container::open(path)?;
    let mut index = String::new();
    let mut failures = 0usize;

    for i in 0..c.resource_count as usize {
        let name = c.name(i);
        let safe: String = name
            .chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
                    ch
                } else {
                    '_'
                }
            })
            .collect();
        let file = outdir.join(format!("{i:05}_{safe}.bin"));
        match lz77::decompress(c.payload(i)) {
            Ok(data) => {
                std::fs::write(&file, &data)
                    .with_context(|| format!("writing {}", file.display()))?;
                index.push_str(&format!("{i}\t{}\t{}\t{}\n", data.len(), name, file.display()));
            }
            Err(e) => {
                failures += 1;
                index.push_str(&format!("{i}\tERR\t{}\t{}\n", e, name));
            }
        }
    }

    std::fs::write(outdir.join("names.tsv"), index)?;
    eprintln!("{}: wrote {} payloads ({failures} failures)", path, c.resource_count);
    Ok(())
}
