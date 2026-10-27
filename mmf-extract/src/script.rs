//! MMF2 script resources.
//!
//! A script container holds compiled bytecode plus the event/action
//! structure that references it. The bytecode is still being
//! reverse-engineered; the extractor writes each resource's payload out
//! verbatim, which is lossless.

use anyhow::Result;
use crate::container::Container;
use crate::lz77;
use std::path::Path;

/// Extract the scripts of a container, returning how many were written.
pub fn extract_container(container: &Container, out_dir: &Path, _original: bool) -> Result<usize> {
    std::fs::create_dir_all(out_dir)?;
    let mut written = 0usize;

    for index in 0..container.resource_count as usize {
        let name = crate::cli::sanitize(container.name(index), index);
        let file = out_dir.join(format!("{index:05}_{name}.raw"));

        let data = lz77::decompress(container.payload(index))
            .unwrap_or_else(|_| container.payload(index).to_vec());
        std::fs::write(&file, &data)?;
        written += 1;
    }

    Ok(written)
}
