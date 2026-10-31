//! MMFW asset extractor for *Disney's Magic Artist Deluxe*.
//!
//! Extracts the proprietary resources of the game's `MMFW` containers into
//! modern, lossless formats. The engine is in-house (`DMA Art Ware`/`MME`),
//! not Clickteam Fusion.
//!
//! ## Module layout
//!
//! The second and third games use container versions 2 and 3, which parse
//! identically, so they share this top level: [`container`], [`picture`],
//! [`lz77`], [`sound`], [`film`], [`script`].
//!
//! The first game (1997) uses version 1, whose container, metadata, palette
//! placement and payload compression all differ. It lives in [`v1`] instead,
//! so the modules above stay free of version-1 variants and an edit to one
//! game's path cannot change the other's. [`container`] remains the single
//! shared parser: it reads the common header and dispatches to
//! [`v1::metadata`] for a version-1 region, which is the pattern every
//! container version follows here.
//!
//! The 3D game (Disney Creazioni in 3D) shares the version-3 skeleton but
//! adds two container *kinds* -- `Models` and `Textures` -- whose metadata
//! regions and payloads are its own, so it lives in [`dma3d`].

pub mod cli;
pub mod container;
pub mod dma3d;
pub mod film;
pub mod lz77;
pub mod picture;
pub mod script;
pub mod sound;
pub mod v1;

pub use container::{Container, ContainerKind, Dimension};
pub use lz77::decompress;
pub use picture::{ExtractReport, PixelFormat};
