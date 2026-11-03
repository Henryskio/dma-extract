//! MMFW asset extractor for *Disney's Magic Artist Deluxe*.
//!
//! Extracts the proprietary resources of the game's `MMFW` containers into
//! modern, lossless formats. The engine is in-house (`DMA Art Ware`/`MME`),
//! not Clickteam Fusion.
//!
//! ## Module layout
//!
//! The extraction itself is written in **Lean** and lives in `lean/`
//! (a Lake project). This crate keeps the command line and the summary; every
//! container parse, decode and output write is reached through [`lean`], which
//! calls the single `mmf_extract` entry point that Lean exports. The Rust
//! decoder modules below are the reference implementation the Lean port was
//! written against: they still exist because `src/bin/`'s analysis bins and the
//! regression tests read them, and they are the evidence a Lean decode is
//! compared with.
//!
//! The second and third games use container versions 2 and 3, which parse
//! identically, so they share this top level in Lean too: `MmfExtract.Container`,
//! `MmfExtract.Picture`, `MmfExtract.Lz77`, and so on.
//!
//! The first game (1997) uses version 1, whose container, metadata, palette
//! placement and payload compression all differ. It lives in Lean's
//! `MmfExtract.V1` instead, so the modules above stay free of version-1
//! variants and an edit to one game's path cannot change the other's.
//! `MmfExtract.Container` remains the single shared parser: it reads the common
//! header and dispatches to `MmfExtract.V1.Metadata` for a version-1 region.
//!
//! The 3D game (Disney Creazioni in 3D) shares the version-3 skeleton but
//! adds two container *kinds* -- `Models` and `Textures` -- whose metadata
//! regions and payloads are its own, so it lives in `MmfExtract.Dma3d`.

pub mod cli;
pub mod container;
pub mod dma3d;
pub mod film;
pub mod lean;
pub mod lz77;
pub mod picture;
pub mod script;
pub mod sound;
pub mod v1;

pub use container::{Container, ContainerKind, Dimension};
pub use lz77::decompress;
pub use picture::{ExtractReport, PixelFormat};
