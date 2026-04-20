//! Core library behind the `kdlfmt` CLI.
//!
//! This crate exposes the formatter, config types, and parser helpers that
//! used to live inside `kdlfmt/cli`. It is designed to be consumed by the
//! `kdlfmt` binary, `kdli`, and any other tool that wants the same formatting
//! behavior without the CLI surface.

mod config;
mod error;
mod high_level;
mod kdl;
mod version;

/// Semver version string of this crate. Matches Cargo.toml.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub use config::KdlFmtConfig;
pub use error::FmtError;
pub use high_level::{format_check, format_document, FormatCheck};
pub use kdl::{format_kdl, parse_kdl};
pub use version::KdlVersion;
