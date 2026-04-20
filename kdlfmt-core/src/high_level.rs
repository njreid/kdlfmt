//! Convenience API combining parse + format in one call.

use crate::{FmtError, KdlFmtConfig, KdlVersion, format_kdl, parse_kdl};

/// Parse `source` as KDL and return the formatted output.
pub fn format_document(
    source: &str,
    version: KdlVersion,
    cfg: &KdlFmtConfig,
) -> Result<String, FmtError> {
    let (doc, version) = parse_kdl(source, Some(version), true)?;
    Ok(format_kdl(doc, cfg, version))
}

/// Result of a format-check pass.
pub struct FormatCheck {
    /// `true` if the input was already byte-identical to the formatter output.
    pub was_formatted: bool,
    /// What a format pass would produce.
    pub would_write: String,
}

/// Parse + format, and report whether the input was already normalized.
pub fn format_check(
    source: &str,
    version: KdlVersion,
    cfg: &KdlFmtConfig,
) -> Result<FormatCheck, FmtError> {
    let formatted = format_document(source, version, cfg)?;
    Ok(FormatCheck {
        was_formatted: formatted == source,
        would_write: formatted,
    })
}
