//! Error types surfaced by `kdlfmt-core`.

use miette::Diagnostic;
use thiserror::Error;

/// Errors produced by the core library.
#[derive(Debug, Error, Diagnostic)]
pub enum FmtError {
    /// The input could not be parsed as KDL.
    #[error(transparent)]
    #[diagnostic(transparent)]
    Parse(#[from] kdl::KdlError),

    /// I/O failure while reading formatter inputs.
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),

    /// A config file was present but malformed.
    #[error("invalid config: {0}")]
    InvalidConfig(String),

    /// A non-standard expression string was encountered while disallowed.
    #[error(
        "Expression strings are disabled at line {line}, column {column} in node `{node_name}` {entry_label}. Re-run without `--no-expression-strings` to allow backtick-delimited values."
    )]
    #[diagnostic(
        code(kdlfmt::expression_string_rejected),
        help("enable expression strings by omitting `--no-expression-strings`")
    )]
    ExpressionStringRejected {
        #[label("expression string here")]
        span: miette::SourceSpan,
        line: usize,
        column: usize,
        node_name: String,
        entry_label: String,
    },
}
