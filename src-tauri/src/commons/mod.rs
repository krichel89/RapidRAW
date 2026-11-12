//! Export to Wikimedia Commons: sign in with OAuth 2.0, upload the exported
//! file with its page text, then write its structured data.
//!
//! Kept apart from the rest of RapidRAW (own settings file, own credential
//! entry, one hook in the export) so that upstream merges stay small. The
//! metadata itself comes from [`crate::mediawiki`].
//!
//! * `oauth`, `wikitext`, `sdc`, `settings`: pure, unit-tested
//! * `client`: the conversation with the Commons API, tested against a local
//!   stand-in server
//! * `pipeline`: one upload from metadata to finished request
//! * `commands`, `export`, `session`: the Tauri side

mod client;
pub mod commands;
mod export;
mod keyring;
mod login_store;
mod oauth;
mod pipeline;
mod sdc;
mod session;
mod settings;
mod wikitext;

pub use export::on_exported;
