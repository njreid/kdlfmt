use crate::{cli::KdlVersion, config::KdlFmtConfig};

#[inline]
pub fn parse_kdl(
    input: &str,
    version: Option<KdlVersion>,
) -> miette::Result<(kdl::KdlDocument, KdlVersion)> {
    match version {
        Some(KdlVersion::V1) => Ok((kdl::KdlDocument::parse_v1(input)?, KdlVersion::V1)),
        Some(KdlVersion::V2) => Ok((kdl::KdlDocument::parse_v2(input)?, KdlVersion::V2)),
        None => {
            if let Ok(v2) = kdl::KdlDocument::parse_v2(input) {
                Ok((v2, KdlVersion::V2))
            } else {
                Ok((kdl::KdlDocument::parse_v1(input)?, KdlVersion::V1))
            }
        }
    }
}

#[inline]
pub fn format_kdl(
    mut input: kdl::KdlDocument,
    config: &KdlFmtConfig,
    version: KdlVersion,
) -> String {
    let format_config = config.get_formatter_config();
    input.autoformat_config(&format_config);

    lint_comments_in_doc(&mut input);

    if config.newlines_before_comments > 0 {
        apply_newlines_before_comments(&mut input, config.newlines_before_comments);
    }

    // ensure_v1/v2 must run before justify: autoformat() sets entry.format = None,
    // and ensure_v1/v2 re-initializes it (with leading: " "), making format_mut()
    // return Some so we can widen the leading gap.
    if KdlVersion::V1 == version {
        input.ensure_v1();
    } else {
        input.ensure_v2();
    }

    if config.justify_first_property {
        apply_justify_first_property(input.nodes_mut());
    }

    input.to_string()
}

fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => {
            let upper: String = c.to_uppercase().collect();
            upper + chars.as_str()
        }
    }
}

/// Process a single line: trim trailing whitespace if it contains a `//` comment,
/// normalize to exactly one space after `//`, and capitalize the first word.
fn lint_line_comment(line: &str) -> String {
    if let Some(idx) = line.find("//") {
        let trimmed = line.trim_end();
        let prefix = &trimmed[..idx];
        let rest = &trimmed[(idx + 2).min(trimmed.len())..];
        let rest_trimmed = rest.trim_start();
        if rest_trimmed.is_empty() {
            format!("{}//", prefix)
        } else {
            format!("{}// {}", prefix, capitalize_first(rest_trimmed))
        }
    } else {
        // No comment — return as-is to preserve indentation whitespace.
        line.to_string()
    }
}

/// Apply `lint_line_comment` to every line in a trivia string, preserving
/// the string's structure (including any trailing newline).
fn lint_trivia(s: &str) -> String {
    // split('\n') rather than lines() so a trailing '\n' round-trips as ["...", ""].
    s.split('\n')
        .map(lint_line_comment)
        .collect::<Vec<_>>()
        .join("\n")
}

fn lint_comments_in_doc(doc: &mut kdl::KdlDocument) {
    if let Some(fmt) = doc.format_mut() {
        fmt.leading = lint_trivia(&fmt.leading);
        fmt.trailing = lint_trivia(&fmt.trailing);
    }
    for node in doc.nodes_mut() {
        lint_comments_in_node(node);
    }
}

fn lint_comments_in_node(node: &mut kdl::KdlNode) {
    if let Some(fmt) = node.format_mut() {
        fmt.leading = lint_trivia(&fmt.leading);
        fmt.before_terminator = lint_trivia(&fmt.before_terminator);
        fmt.trailing = lint_trivia(&fmt.trailing);
    }
    for entry in node.entries_mut() {
        if let Some(fmt) = entry.format_mut() {
            fmt.trailing = lint_trivia(&fmt.trailing);
        }
    }
    if let Some(children) = node.children_mut() {
        lint_comments_in_doc(children);
    }
}

/// Insert `n` blank lines before any top-level node whose leading trivia
/// contains a comment. Skips the first node (no preceding node to separate from).
fn apply_newlines_before_comments(doc: &mut kdl::KdlDocument, n: u32) {
    let extra = "\n".repeat(n as usize);
    for (i, node) in doc.nodes_mut().iter_mut().enumerate() {
        if i == 0 {
            continue;
        }
        if let Some(fmt) = node.format_mut() {
            if fmt.leading.contains("//") || fmt.leading.contains("/*") {
                fmt.leading = format!("{}{}", extra, fmt.leading);
            }
        }
    }
}

/// Pad the space between each node name and its first entry so that first
/// entries of sibling nodes all start at the same column. Applied recursively.
fn apply_justify_first_property(nodes: &mut [kdl::KdlNode]) {
    let max_len = nodes
        .iter()
        .map(|n| n.name().value().len())
        .max()
        .unwrap_or(0);

    for node in nodes.iter_mut() {
        let padding = " ".repeat(max_len - node.name().value().len() + 1);
        if let Some(first_entry) = node.entries_mut().first_mut() {
            if let Some(fmt) = first_entry.format_mut() {
                fmt.leading = padding;
            }
        }
        if let Some(children) = node.children_mut() {
            apply_justify_first_property(children.nodes_mut());
        }
    }
}

#[cfg(test)]
mod test {
    use super::parse_kdl;
    use crate::{cli::KdlVersion, config::KdlFmtConfig, kdl::format_kdl};

    #[test]
    fn it_should_be_reversible() {
        let input = "world {\n    child \"1\"\n    child \"2\"\n}\n";
        let (doc, version) = parse_kdl(input, Some(KdlVersion::V1)).expect("it to parse valid kdl");
        let formatted = format_kdl(doc, &KdlFmtConfig::default(), version);
        assert_eq!(input, formatted);
    }

    #[test]
    fn it_should_lint_leading_comment_uppercase() {
        // Leading comment before a node: lowercase first word → capitalised.
        let input = "// this starts lowercase\nnode\n";
        let (doc, version) =
            parse_kdl(input, Some(KdlVersion::V1)).expect("it to parse valid kdl");
        let formatted = format_kdl(doc, &KdlFmtConfig::default(), version);
        assert!(
            formatted.contains("// This starts lowercase"),
            "expected capitalised leading comment, got: {formatted:?}"
        );
    }

    #[test]
    fn it_should_lint_leading_comment_trailing_whitespace() {
        // Trailing whitespace on a leading comment line must be stripped.
        let input = "// trailing spaces   \nnode\n";
        let (doc, version) =
            parse_kdl(input, Some(KdlVersion::V1)).expect("it to parse valid kdl");
        let formatted = format_kdl(doc, &KdlFmtConfig::default(), version);
        assert!(
            !formatted.contains("   "),
            "expected trailing whitespace stripped, got: {formatted:?}"
        );
    }

    #[test]
    fn it_should_lint_leading_comment_single_space_after_marker() {
        // `//no-space` and `//  double-space` both get exactly one space.
        let input = "//nospace\nnode\n";
        let (doc, version) =
            parse_kdl(input, Some(KdlVersion::V1)).expect("it to parse valid kdl");
        let formatted = format_kdl(doc, &KdlFmtConfig::default(), version);
        assert!(
            formatted.contains("// Nospace"),
            "expected '// Nospace', got: {formatted:?}"
        );
    }

    #[test]
    fn it_should_lint_inline_comment_v1() {
        // Inline comment on the same line as a node (V1, where trivia is reliably preserved).
        let input = "node // inline lowercase  \n";
        let (doc, version) =
            parse_kdl(input, Some(KdlVersion::V1)).expect("it to parse valid kdl");
        let formatted = format_kdl(doc, &KdlFmtConfig::default(), version);
        assert!(
            formatted.contains("// Inline lowercase"),
            "expected capitalised inline comment, got: {formatted:?}"
        );
        assert!(
            !formatted.contains("  "),
            "expected trailing whitespace stripped from inline comment, got: {formatted:?}"
        );
    }

    #[test]
    fn it_should_justify_first_property() {
        // "short" (5) vs "longer" (6): first entry of "short" gets an extra space.
        let input = "short \"a\"\nlonger \"b\"\n";
        let (doc, version) =
            parse_kdl(input, Some(KdlVersion::V1)).expect("it to parse valid kdl");
        let config = KdlFmtConfig {
            justify_first_property: true,
            ..KdlFmtConfig::default()
        };
        let formatted = format_kdl(doc, &config, version);
        assert!(
            formatted.contains("short  \"a\""),
            "expected 2-space gap after 'short', got: {formatted:?}"
        );
        assert!(
            formatted.contains("longer \"b\""),
            "expected 1-space gap after 'longer', got: {formatted:?}"
        );
    }

    #[test]
    fn it_should_justify_recursively() {
        // Justify applies to children independently of their parent group.
        let input = "parent {\n    a \"x\"\n    bb \"y\"\n}\n";
        let (doc, version) =
            parse_kdl(input, Some(KdlVersion::V1)).expect("it to parse valid kdl");
        let config = KdlFmtConfig {
            justify_first_property: true,
            ..KdlFmtConfig::default()
        };
        let formatted = format_kdl(doc, &config, version);
        // "a" (1) vs "bb" (2): "a" gets 2 spaces, "bb" gets 1.
        assert!(
            formatted.contains("a  \"x\""),
            "expected 2-space gap after 'a', got: {formatted:?}"
        );
        assert!(
            formatted.contains("bb \"y\""),
            "expected 1-space gap after 'bb', got: {formatted:?}"
        );
    }

    #[test]
    fn it_should_add_newlines_before_comments() {
        // With newlines_before_comments = 1, a blank line is inserted before
        // each top-level node whose leading trivia contains a comment.
        let input = "first\n// A comment\nsecond\n";
        let (doc, version) =
            parse_kdl(input, Some(KdlVersion::V1)).expect("it to parse valid kdl");
        let config = KdlFmtConfig {
            newlines_before_comments: 1,
            ..KdlFmtConfig::default()
        };
        let formatted = format_kdl(doc, &config, version);
        // There should be a blank line before the comment.
        assert!(
            formatted.contains("\n\n// A comment"),
            "expected blank line before comment, got: {formatted:?}"
        );
    }
}
