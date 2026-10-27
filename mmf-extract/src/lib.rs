//! MMFW (Clickteam Multimedia Fusion 2) asset extractor.
//!
//! Extracts the proprietary resources of a Multimedia Fusion 2 application
//! into modern, lossless formats.

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
