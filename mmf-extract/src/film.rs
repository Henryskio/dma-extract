//! MMF2 film resources.
//!
//! A film is a sequence of frames, each of which is a picture-like resource
//! plus timing information. The frame encoding is still being
//! reverse-engineered; the extractor writes each frame's payload out
//! verbatim, which is lossless.

use anyhow::Result;
use crate::container::Container;
use crate::lz77;
use std::path::Path;

/// Extract the frames of a film container, returning how many were written.
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
