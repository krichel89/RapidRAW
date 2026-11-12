//! MediaWiki metadata (Wikimedia Commons) for the fork's export targets.
//!
//! Kept in its own module so upstream merges touch as few existing files as
//! possible: the sidecar field, one line in `is_default_sidecar`, and the
//! module and command registration in `lib.rs`.

pub mod commands;
pub mod model;
