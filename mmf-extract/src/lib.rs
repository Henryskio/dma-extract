//! MMFW asset extractor for *Disney's Magic Artist Deluxe*.
//!
//! Extracts the proprietary resources of the game's `MMFW` containers into
//! modern, lossless formats. The engine is in-house (`DMA Art Ware`/`MME`),
//! not Clickteam Fusion.

pub mod cli;
pub mod container;
pub mod film;
pub mod lz77;
pub mod picture;
pub mod script;
pub mod sound;

pub use container::{Container, ContainerKind, Dimension};
pub use lz77::decompress;
pub use picture::{ExtractReport, PixelFormat};
