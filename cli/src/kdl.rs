use crate::{cli::KdlVersion, config::KdlFmtConfig};

#[inline]
pub fn parse_kdl(
    input: &str,
    version: Option<KdlVersion>,
    allow_expression_strings: bool,
) -> miette::Result<(kdl::KdlDocument, KdlVersion)> {
    let (document, version) = match version {
        Some(KdlVersion::V1) => (kdl::KdlDocument::parse_v1(input)?, KdlVersion::V1),
        Some(KdlVersion::V2) => (kdl::KdlDocument::parse_v2(input)?, KdlVersion::V2),
        None => {
            if let Ok(v2) = kdl::KdlDocument::parse_v2(input) {
                (v2, KdlVersion::V2)
            } else {
                (kdl::KdlDocument::parse_v1(input)?, KdlVersion::V1)
            }
        }
    };

    if !allow_expression_strings {
        reject_expression_strings(&document, input)?;
    }

    Ok((document, version))
}

#[inline]
fn reject_expression_strings(document: &kdl::KdlDocument, input: &str) -> miette::Result<()> {
    struct ExpressionStringLocation {
        span: miette::SourceSpan,
        node_name: String,
        entry_label: String,
    }

    fn visit(document: &kdl::KdlDocument) -> Option<ExpressionStringLocation> {
        for node in document.nodes() {
            for (arg_index, entry) in node.entries().iter().enumerate() {
                if entry.value().is_expression_string() {
                    let entry_label = entry.name().map_or_else(
                        || format!("argument {}", arg_index + 1),
                        |name| format!("property `{}`", name.value()),
                    );

                    return Some(ExpressionStringLocation {
                        span: entry.span(),
                        node_name: node.name().value().to_owned(),
                        entry_label,
                    });
                }
            }

            if let Some(children) = node.children()
                && let Some(location) = visit(children)
            {
                return Some(location);
            }
        }

        None
    }

    if let Some(location) = visit(document) {
        let (line, column) = line_and_column(input, location.span.offset());
        Err(miette::miette!(
            "Expression strings are disabled at line {}, column {} in node `{}` {}. Re-run without `--no-expression-strings` to allow backtick-delimited values.",
            line,
            column,
            location.node_name,
            location.entry_label,
        ))
    } else {
        Ok(())
    }
}

#[inline]
fn line_and_column(input: &str, offset: usize) -> (usize, usize) {
    let mut line = 1usize;
    let mut column = 1usize;

    for ch in input[..offset.min(input.len())].chars() {
        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }

    (line, column)
}

#[inline]
pub fn format_kdl(
    mut input: kdl::KdlDocument,
    config: &KdlFmtConfig,
    version: KdlVersion,
) -> String {
    let format_config = config.get_formatter_config();

    // 1. Rescue comments that would be lost by autoformat (v2 inline comments)
    rescue_terminator_comments(input.nodes_mut());
    rescue_entry_leading_comments(input.nodes_mut());

    // 2. Basic autoformat according to library defaults
    input.autoformat_config(&format_config);

    // 3. Ensure version-specific formatting defaults (e.g. node spaces)
    if KdlVersion::V1 == version {
        input.ensure_v1();
    } else {
        input.ensure_v2();
    }

    // 4. Apply all custom formatting rules in a single recursive pass
    apply_formatting(&mut input, config, 0);

    let output = input.to_string();

    // 5. Final post-processing on the string output
    if config.remove_trailing_blank_lines_in_blocks {
        remove_trailing_blank_lines_before_braces(&output)
    } else {
        output
    }
}

/// A single recursive pass that applies all kdlfmt formatting rules.
fn apply_formatting(doc: &mut kdl::KdlDocument, config: &KdlFmtConfig, depth: usize) {
    let tp = TriviaProcessor::new(config, depth);

    if let Some(fmt) = doc.format_mut() {
        fmt.leading = tp.process(&fmt.leading, false);
        fmt.trailing = tp.process(&fmt.trailing, false);
    }

    let nodes = doc.nodes_mut();

    // a. Justify first property if enabled
    if config.justify_first_property {
        let max_name_len = nodes
            .iter()
            .map(|n| n.name().value().len())
            .max()
            .unwrap_or(0);
        for node in nodes.iter_mut() {
            let node_name_len = node.name().value().len();
            if let Some(first_entry) = node.entries_mut().first_mut() {
                let padding = " ".repeat(max_name_len - node_name_len + 1);
                if let Some(fmt) = first_entry.format_mut() {
                    if fmt.leading.contains("//") || fmt.leading.contains("/*") {
                        // If it contains a comment, we want to ensure there is at least
                        // one space before the comment, but we don't want to lose it.
                        // However, standard justification usually puts the padding
                        // BEFORE the entry. If a comment is there, we should probably
                        // keep it and just ensure it's separated.
                        if !fmt.leading.starts_with(' ') {
                            fmt.leading.insert(0, ' ');
                        }
                    } else {
                        fmt.leading = padding;
                    }
                }
            }
        }
    }

    let len = nodes.len();
    for i in 0..len {
        // b. Newlines before comments (skip first node at each level)
        if i > 0 && depth < 5 && config.newlines_before_comments[depth] > 0 {
            let n = config.newlines_before_comments[depth];
            if let Some(fmt) = nodes[i].format_mut() {
                if fmt.leading.contains("//") || fmt.leading.contains("/*") {
                    fmt.leading.insert_str(0, &"\n".repeat(n as usize));
                }
            }
        }

        // c. Newlines after close (if block and has next node)
        if i + 1 < len && depth < 5 && config.newlines_after_close[depth] > 0 {
            if nodes[i].children().is_some() {
                let n = config.newlines_after_close[depth];
                if let Some(fmt) = nodes[i + 1].format_mut() {
                    fmt.leading.insert_str(0, &"\n".repeat(n as usize));
                }
            }
        }

        let node = &mut nodes[i];

        // d. Process node trivia
        if let Some(fmt) = node.format_mut() {
            fmt.leading = tp.process(&fmt.leading, false);
            fmt.before_terminator = tp.process(&fmt.before_terminator, true);
            fmt.trailing = tp.process(&fmt.trailing, false);
        }

        // e. Process entry trivia
        for entry in node.entries_mut() {
            if let Some(fmt) = entry.format_mut() {
                fmt.trailing = tp.process(&fmt.trailing, true);
            }
        }

        // f. Collapse empty blocks
        if config.collapse_empty_blocks {
            if let Some(children) = node.children_mut() {
                if children.nodes().is_empty() {
                    if let Some(fmt) = children.format_mut() {
                        fmt.leading = String::new();
                        fmt.trailing = String::new();
                    }
                }
            }
        }

        // g. Recurse to children
        if let Some(children) = node.children_mut() {
            apply_formatting(children, config, depth + 1);
        }
    }
}

/// Strip blank lines that appear immediately before a closing `}`.
/// Operates on the final formatted string since blank lines may be stored
/// in various trivia fields depending on position.
fn remove_trailing_blank_lines_before_braces(s: &str) -> String {
    let lines: Vec<&str> = s.split('\n').collect();
    let mut result: Vec<String> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim().is_empty() {
            // Look ahead past any further blank lines to the next content line.
            let mut j = i + 1;
            while j < lines.len() && lines[j].trim().is_empty() {
                j += 1;
            }
            // If the next content line is a bare `}`, drop all the blank lines.
            if j < lines.len() && lines[j].trim() == "}" {
                i = j;
                continue;
            }
        }
        result.push(lines[i].to_string());
        i += 1;
    }
    result.join("\n")
}

/// Move inline comments from `terminator` to `before_terminator` so that
/// `autoformat_config` preserves them. In KDL v2, `node-space` does not
/// include `single-line-comment`, so the parser leaves `"// comment\n"` in
/// `terminator`. `autoformat_config` replaces any terminator that doesn't
/// start with `\n` with just `"\n"`, silently dropping the comment.
fn rescue_terminator_comments(nodes: &mut [kdl::KdlNode]) {
    for node in nodes.iter_mut() {
        if let Some(fmt) = node.format_mut() {
            if fmt.terminator.contains("//") || fmt.terminator.contains("/*") {
                // Strip the trailing newline(s) to get just the comment text.
                let comment = fmt
                    .terminator
                    .trim_end_matches('\n')
                    .trim_end_matches('\r')
                    .to_string();
                fmt.before_terminator.push_str(&comment);
                fmt.terminator = "\n".to_string();
            }
        }
        if let Some(children) = node.children_mut() {
            rescue_terminator_comments(children.nodes_mut());
        }
    }
}

/// Preserve entry formatting when its leading trivia contains comments.
///
/// `autoformat_config` drops entry formatting by default, which also drops any
/// comments stored in an entry's leading trivia. Mark those entries to keep
/// their leading comment trivia through the autoformat pass.
fn rescue_entry_leading_comments(nodes: &mut [kdl::KdlNode]) {
    for node in nodes.iter_mut() {
        for entry in node.entries_mut() {
            let has_leading_comment = entry
                .format()
                .is_some_and(|fmt| fmt.leading.contains("//") || fmt.leading.contains("/*"));

            if has_leading_comment {
                entry.keep_format();
            }
        }

        if let Some(children) = node.children_mut() {
            rescue_entry_leading_comments(children.nodes_mut());
        }
    }
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

/// A unified processor for trivia strings (comments and whitespace).
/// Consolidates linting, indentation, and normalization into a single pass.
struct TriviaProcessor<'a> {
    config: &'a KdlFmtConfig,
    indent: String,
}

impl<'a> TriviaProcessor<'a> {
    fn new(config: &'a KdlFmtConfig, depth: usize) -> Self {
        Self {
            config,
            indent: config.indent.repeat(depth),
        }
    }

    /// Process a trivia string by applying all enabled rules line-by-line.
    fn process(&self, s: &str, is_inline: bool) -> String {
        if s.is_empty() {
            return String::new();
        }

        let mut lines: Vec<String> = Vec::new();
        let mut in_block = false;
        let mut i = 0;
        let raw_lines: Vec<&str> = s.split('\n').collect();

        while i < raw_lines.len() {
            let line = raw_lines[i];
            let trimmed = line.trim_start();

            if !in_block {
                // 1. Strip empty block comments
                if self.config.strip_empty_block_comments && trimmed.starts_with("/*") {
                    if let Some(close_idx) = trimmed[2..].find("*/") {
                        if trimmed[2..2 + close_idx].trim().is_empty() {
                            i += 1;
                            continue;
                        }
                    } else {
                        // Potential multi-line empty block
                        let mut j = i + 1;
                        let mut all_blank = true;
                        while j < raw_lines.len() {
                            let t = raw_lines[j].trim_start();
                            if t.starts_with("*/") {
                                break;
                            }
                            if !t.is_empty() {
                                all_blank = false;
                                break;
                            }
                            j += 1;
                        }
                        if all_blank && j < raw_lines.len() {
                            i = j + 1;
                            continue;
                        }
                    }
                }

                // 2. Normalize single-line block comments to line comments
                if !is_inline
                    && self.config.normalize_single_line_block_comments
                    && trimmed.starts_with("/*")
                {
                    if let Some(close_idx) = trimmed[2..].find("*/") {
                        let content = trimmed[2..2 + close_idx].trim();
                        if !content.is_empty() {
                            let linted = format!("// {}", capitalize_first(content));
                            lines.push(format!("{}{}", self.indent, linted));
                            i += 1;
                            continue;
                        }
                    }
                }

                // 3. Convert runs of line comments to multiline
                if !is_inline && self.config.n_comment_lines_to_multiline.is_some() {
                    let threshold = self.config.n_comment_lines_to_multiline.unwrap() as usize;
                    if trimmed.starts_with("//") {
                        let mut run = vec![line];
                        let mut j = i + 1;
                        while j < raw_lines.len() && raw_lines[j].trim_start().starts_with("//") {
                            run.push(raw_lines[j]);
                            j += 1;
                        }
                        if run.len() >= threshold {
                            lines.push(format!("{}/*", self.indent));
                            for run_line in run {
                                let content = run_line.trim_start()[2..].trim_start();
                                if content.is_empty() {
                                    lines.push(self.indent.clone());
                                } else {
                                    lines.push(format!("{}{}", self.indent, content));
                                }
                            }
                            lines.push(format!("{}*/", self.indent));
                            i = j;
                            continue;
                        }
                    }
                }

                // 4. Regular line comment processing (lint + indent)
                if trimmed.starts_with("//") {
                    let prefix = &line[..line.len() - trimmed.len()];
                    // Only a real comment if prefix is purely whitespace
                    if prefix.chars().all(|c| c.is_whitespace()) {
                        let comment_content = trimmed[2..].trim();
                        let linted = if comment_content.is_empty() {
                            "//".to_string()
                        } else {
                            format!("// {}", capitalize_first(comment_content))
                        };
                        lines.push(format!("{}{}", self.indent, linted));
                    } else {
                        lines.push(line.to_string());
                    }
                } else if trimmed.starts_with("/*") {
                    lines.push(format!("{}{}", self.indent, trimmed));
                    if !trimmed[2..].contains("*/") {
                        in_block = true;
                    }
                } else if trimmed.starts_with("/-") {
                    lines.push(format!("{}{}", self.indent, trimmed));
                } else {
                    lines.push(line.to_string());
                }
            } else {
                // Inside a /* */ block
                if trimmed.starts_with("*/") {
                    lines.push(format!("{}*/", self.indent));
                    in_block = false;
                } else {
                    let content = if let Some(rest) = trimmed.strip_prefix("* ") {
                        rest
                    } else if let Some(rest) = trimmed.strip_prefix('*') {
                        rest
                    } else {
                        trimmed
                    };
                    if content.is_empty() {
                        lines.push(self.indent.clone());
                    } else {
                        lines.push(format!("{}{}", self.indent, content));
                    }
                }
            }
            i += 1;
        }

        lines.join("\n")
    }
}

#[cfg(test)]
mod test {
    use super::parse_kdl;
    use crate::{cli::KdlVersion, config::KdlFmtConfig, kdl::format_kdl};

    #[test]
    fn it_should_be_reversible() {
        let input = "world {\n    child \"1\"\n    child \"2\"\n}\n";
        let (doc, version) =
            parse_kdl(input, Some(KdlVersion::V1), true).expect("it to parse valid kdl");
        let formatted = format_kdl(doc, &KdlFmtConfig::default(), version);
        assert_eq!(input, formatted);
    }

    #[test]
    fn it_should_lint_leading_comment_uppercase() {
        // Leading comment before a node: lowercase first word → capitalised.
        let input = "// this starts lowercase\nnode\n";
        let (doc, version) =
            parse_kdl(input, Some(KdlVersion::V1), true).expect("it to parse valid kdl");
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
            parse_kdl(input, Some(KdlVersion::V1), true).expect("it to parse valid kdl");
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
            parse_kdl(input, Some(KdlVersion::V1), true).expect("it to parse valid kdl");
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
            parse_kdl(input, Some(KdlVersion::V1), true).expect("it to parse valid kdl");
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
    fn it_should_rescue_and_lint_inline_comment_v2() {
        // In KDL v2 the parser puts inline comments into `terminator`, not
        // `before_terminator`. Our rescue pass moves them before autoformat discards them.
        let input = "node // inline lowercase  \n";
        let (doc, version) =
            parse_kdl(input, Some(KdlVersion::V2), true).expect("it to parse valid kdl");
        let formatted = format_kdl(doc, &KdlFmtConfig::default(), version);
        assert!(
            formatted.contains("// Inline lowercase"),
            "expected rescued+capitalised v2 inline comment, got: {formatted:?}"
        );
    }

    #[test]
    fn it_should_rescue_inline_comment_v2_with_entries() {
        // Inline comment after an entry value should also be rescued.
        let input = "node \"val\" // entry comment  \n";
        let (doc, version) =
            parse_kdl(input, Some(KdlVersion::V2), true).expect("it to parse valid kdl");
        let formatted = format_kdl(doc, &KdlFmtConfig::default(), version);
        assert!(
            formatted.contains("// Entry comment"),
            "expected rescued+capitalised entry-line comment, got: {formatted:?}"
        );
    }

    #[test]
    fn it_should_justify_first_property() {
        // "short" (5) vs "longer" (6): first entry of "short" gets an extra space.
        let input = "short \"a\"\nlonger \"b\"\n";
        let (doc, version) =
            parse_kdl(input, Some(KdlVersion::V1), true).expect("it to parse valid kdl");
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
            parse_kdl(input, Some(KdlVersion::V1), true).expect("it to parse valid kdl");
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
    fn it_should_add_newlines_after_close_level1() {
        // With newlines_after_close level1=1, a blank line is inserted after
        // the closing brace of each top-level block node.
        let input = "first {\n    child\n}\nsecond\nthird {\n    child\n}\nfourth\n";
        let (doc, version) =
            parse_kdl(input, Some(KdlVersion::V1), true).expect("it to parse valid kdl");
        let config = KdlFmtConfig {
            newlines_after_close: [1, 0, 0, 0, 0],
            ..KdlFmtConfig::default()
        };
        let formatted = format_kdl(doc, &config, version);
        assert!(
            formatted.contains("}\n\nsecond"),
            "expected blank line after first block close, got: {formatted:?}"
        );
        assert!(
            formatted.contains("}\n\nfourth"),
            "expected blank line after third block close, got: {formatted:?}"
        );
        assert!(
            formatted.contains("second\nthird"),
            "expected no extra line between non-block nodes, got: {formatted:?}"
        );
    }

    #[test]
    fn it_should_add_newlines_after_close_level2() {
        // With level2=1, blank lines appear after nested block closes but not top-level.
        let input = "parent {\n    a {\n        leaf\n    }\n    b\n}\n";
        let (doc, version) =
            parse_kdl(input, Some(KdlVersion::V1), true).expect("it to parse valid kdl");
        let config = KdlFmtConfig {
            newlines_after_close: [0, 1, 0, 0, 0],
            ..KdlFmtConfig::default()
        };
        let formatted = format_kdl(doc, &config, version);
        assert!(
            formatted.contains("}\n\n    b"),
            "expected blank line after nested block close, got: {formatted:?}"
        );
    }

    #[test]
    fn it_should_convert_comment_lines_to_multiline() {
        // Three consecutive // lines meet the threshold of 3 and become /* */.
        let input = "// Line one\n// Line two\n// Line three\nnode\n";
        let (doc, version) =
            parse_kdl(input, Some(KdlVersion::V1), true).expect("it to parse valid kdl");
        let config = KdlFmtConfig {
            n_comment_lines_to_multiline: Some(3),
            ..KdlFmtConfig::default()
        };
        let formatted = format_kdl(doc, &config, version);
        assert!(
            formatted.contains("/*"),
            "expected multiline comment open, got: {formatted:?}"
        );
        assert!(
            formatted.contains("\nLine one\n"),
            "expected content without * prefix, got: {formatted:?}"
        );
        assert!(
            formatted.contains("\n*/\n"),
            "expected multiline comment close without space, got: {formatted:?}"
        );
        assert!(
            !formatted.contains("//"),
            "expected no remaining // comments, got: {formatted:?}"
        );
    }

    #[test]
    fn it_should_indent_multiline_comment_in_block() {
        // /* and */ lines must be indented to match the depth of their block.
        let input = "parent {\n    // Line one\n    // Line two\n    child\n}\n";
        let (doc, version) =
            parse_kdl(input, Some(KdlVersion::V1), true).expect("it to parse valid kdl");
        let config = KdlFmtConfig {
            n_comment_lines_to_multiline: Some(2),
            ..KdlFmtConfig::default()
        };
        let formatted = format_kdl(doc, &config, version);
        assert!(
            formatted.contains("    /*"),
            "expected 4-space-indented /* in block, got: {formatted:?}"
        );
        assert!(
            !formatted.contains("\n/*"),
            "expected no unindented /* at top of line, got: {formatted:?}"
        );
    }

    #[test]
    fn it_should_not_convert_comment_lines_below_threshold() {
        // Two // lines do not meet threshold of 3 — stay as //.
        let input = "// Line one\n// Line two\nnode\n";
        let (doc, version) =
            parse_kdl(input, Some(KdlVersion::V1), true).expect("it to parse valid kdl");
        let config = KdlFmtConfig {
            n_comment_lines_to_multiline: Some(3),
            ..KdlFmtConfig::default()
        };
        let formatted = format_kdl(doc, &config, version);
        assert!(
            formatted.contains("//"),
            "expected // comments preserved below threshold, got: {formatted:?}"
        );
        assert!(
            !formatted.contains("/*"),
            "expected no multiline comment, got: {formatted:?}"
        );
    }

    #[test]
    fn it_should_not_treat_url_as_comment_marker() {
        // :// in a URL inside a /* */ block must not be mistaken for a comment.
        let input = "/* See https://example.com for details */\nnode\n";
        let (doc, version) =
            parse_kdl(input, Some(KdlVersion::V1), true).expect("it to parse valid kdl");
        let formatted = format_kdl(doc, &KdlFmtConfig::default(), version);
        assert!(
            formatted.contains("https://example.com"),
            "expected URL preserved, got: {formatted:?}"
        );
        assert!(
            !formatted.contains("https:// Example.com"),
            "expected URL not split by lint, got: {formatted:?}"
        );
    }

    #[test]
    fn it_should_not_lint_inside_block_comment() {
        // Content lines inside /* */ blocks must not be capitalised or reformatted.
        let input = "/* see https://example.com\n * another line\n */\nnode\n";
        let (doc, version) =
            parse_kdl(input, Some(KdlVersion::V1), true).expect("it to parse valid kdl");
        let formatted = format_kdl(doc, &KdlFmtConfig::default(), version);
        assert!(
            formatted.contains("https://example.com"),
            "expected URL inside block comment preserved, got: {formatted:?}"
        );
        assert!(
            !formatted.contains("Https://"),
            "expected no capitalised URL, got: {formatted:?}"
        );
    }

    #[test]
    fn it_should_remove_trailing_blank_lines_in_blocks() {
        let input = "parent {\n    child\n\n\n}\n";
        let (doc, version) =
            parse_kdl(input, Some(KdlVersion::V1), true).expect("it to parse valid kdl");
        let config = KdlFmtConfig {
            remove_trailing_blank_lines_in_blocks: true,
            ..KdlFmtConfig::default()
        };
        let formatted = format_kdl(doc, &config, version);
        assert!(
            formatted.contains("child\n}"),
            "expected no blank line before closing brace, got: {formatted:?}"
        );
    }

    #[test]
    fn it_should_collapse_empty_blocks() {
        let input = "parent {\n\n}\nsibling\n";
        let (doc, version) =
            parse_kdl(input, Some(KdlVersion::V1), true).expect("it to parse valid kdl");
        let config = KdlFmtConfig {
            collapse_empty_blocks: true,
            ..KdlFmtConfig::default()
        };
        let formatted = format_kdl(doc, &config, version);
        assert!(
            formatted.contains("parent {}"),
            "expected collapsed empty block, got: {formatted:?}"
        );
        assert!(
            !formatted.contains("parent {\n"),
            "expected no multi-line empty block, got: {formatted:?}"
        );
    }

    #[test]
    fn it_should_not_collapse_non_empty_blocks() {
        let input = "parent {\n    child\n}\n";
        let (doc, version) =
            parse_kdl(input, Some(KdlVersion::V1), true).expect("it to parse valid kdl");
        let config = KdlFmtConfig {
            collapse_empty_blocks: true,
            ..KdlFmtConfig::default()
        };
        let formatted = format_kdl(doc, &config, version);
        assert!(
            formatted.contains("parent {\n    child\n}"),
            "expected non-empty block unchanged, got: {formatted:?}"
        );
    }

    #[test]
    fn it_should_fix_multiline_block_indent_in_block() {
        // A /* */ block whose opening line has the wrong indent should be
        // re-indented to match its depth, including content and closing lines.
        let input = "parent {\n /*\n     * Content line\n     */\n    child\n}\n";
        let (doc, version) =
            parse_kdl(input, Some(KdlVersion::V1), true).expect("it to parse valid kdl");
        let formatted = format_kdl(doc, &KdlFmtConfig::default(), version);
        assert!(
            formatted.contains("    /*"),
            "expected 4-space-indented /*, got: {formatted:?}"
        );
        assert!(
            !formatted.contains("\n /*\n"),
            "expected no single-space-indented /* line, got: {formatted:?}"
        );
        assert!(
            formatted.contains("    Content line"),
            "expected content line at correct indent (no * prefix), got: {formatted:?}"
        );
    }

    #[test]
    fn it_should_indent_comments_to_node_level() {
        // A badly-indented comment inside a block should be fixed to match the child depth.
        let input = "parent {\n// bad indent\n    child\n}\n";
        let (doc, version) =
            parse_kdl(input, Some(KdlVersion::V1), true).expect("it to parse valid kdl");
        let formatted = format_kdl(doc, &KdlFmtConfig::default(), version);
        assert!(
            formatted.contains("    // Bad indent"),
            "expected comment indented to child level, got: {formatted:?}"
        );
    }

    #[test]
    fn it_should_add_newlines_before_comments_level1() {
        // With newlines_before_comments level1=1, a blank line is inserted before
        // each top-level node whose leading trivia contains a comment.
        let input = "first\n// A comment\nsecond\n";
        let (doc, version) =
            parse_kdl(input, Some(KdlVersion::V1), true).expect("it to parse valid kdl");
        let config = KdlFmtConfig {
            newlines_before_comments: [1, 0, 0, 0, 0],
            ..KdlFmtConfig::default()
        };
        let formatted = format_kdl(doc, &config, version);
        assert!(
            formatted.contains("\n\n// A comment"),
            "expected blank line before comment, got: {formatted:?}"
        );
    }

    #[test]
    fn it_should_add_newlines_before_comments_level2() {
        // level1=0, level2=1: no extra line at top-level, but blank before
        // comment-bearing child nodes.
        let input = "parent {\n    a\n    // Child comment\n    b\n}\n";
        let (doc, version) =
            parse_kdl(input, Some(KdlVersion::V1), true).expect("it to parse valid kdl");
        let config = KdlFmtConfig {
            newlines_before_comments: [0, 1, 0, 0, 0],
            ..KdlFmtConfig::default()
        };
        let formatted = format_kdl(doc, &config, version);
        assert!(
            formatted.contains("\n\n    // Child comment"),
            "expected blank line before child comment, got: {formatted:?}"
        );
    }

    #[test]
    fn it_should_fix_slashdash_indentation() {
        // A /- node with wrong indentation inside a block should be re-indented
        // to match sibling nodes at that depth.
        let input = "parent {\n /- bad-indent\n    child\n}\n";
        let (doc, version) =
            parse_kdl(input, Some(KdlVersion::V1), true).expect("it to parse valid kdl");
        let formatted = format_kdl(doc, &KdlFmtConfig::default(), version);
        assert!(
            formatted.contains("    /- bad-indent"),
            "expected 4-space-indented /- node, got: {formatted:?}"
        );
        assert!(
            !formatted.contains("\n /- "),
            "expected no 1-space-indented /- node, got: {formatted:?}"
        );
    }

    #[test]
    fn it_should_strip_empty_block_comments() {
        // A /* */ block whose only content is blank lines should be removed.
        let input = "/*\n\n\n*/\nnode\n";
        let (doc, version) =
            parse_kdl(input, Some(KdlVersion::V1), true).expect("it to parse valid kdl");
        let config = KdlFmtConfig {
            strip_empty_block_comments: true,
            ..KdlFmtConfig::default()
        };
        let formatted = format_kdl(doc, &config, version);
        assert!(
            !formatted.contains("/*"),
            "expected empty block comment removed, got: {formatted:?}"
        );
    }

    #[test]
    fn it_should_strip_single_line_empty_block_comment() {
        // A /* */ on one line with only whitespace inside should be removed.
        let input = "/* */\nnode\n";
        let (doc, version) =
            parse_kdl(input, Some(KdlVersion::V1), true).expect("it to parse valid kdl");
        let config = KdlFmtConfig {
            strip_empty_block_comments: true,
            ..KdlFmtConfig::default()
        };
        let formatted = format_kdl(doc, &config, version);
        assert!(
            !formatted.contains("/*"),
            "expected empty single-line block comment removed, got: {formatted:?}"
        );
    }

    #[test]
    fn it_should_not_strip_non_empty_block_comment() {
        // A /* */ block with real content must not be stripped.
        let input = "/* Some content */\nnode\n";
        let (doc, version) =
            parse_kdl(input, Some(KdlVersion::V1), true).expect("it to parse valid kdl");
        let config = KdlFmtConfig {
            strip_empty_block_comments: true,
            ..KdlFmtConfig::default()
        };
        let formatted = format_kdl(doc, &config, version);
        assert!(
            formatted.contains("Some content"),
            "expected non-empty block comment preserved, got: {formatted:?}"
        );
    }

    #[test]
    fn it_should_normalize_single_line_block_to_line_comment() {
        // /* content */ on one line becomes // content.
        let input = "/* This is a note */\nnode\n";
        let (doc, version) =
            parse_kdl(input, Some(KdlVersion::V1), true).expect("it to parse valid kdl");
        let config = KdlFmtConfig {
            normalize_single_line_block_comments: true,
            ..KdlFmtConfig::default()
        };
        let formatted = format_kdl(doc, &config, version);
        assert!(
            formatted.contains("// This is a note"),
            "expected // comment, got: {formatted:?}"
        );
        assert!(
            !formatted.contains("/*"),
            "expected no /* remaining, got: {formatted:?}"
        );
    }

    #[test]
    fn it_should_normalize_and_lint_converted_line_comment() {
        // After normalize, the new // line should be linted (capitalised).
        let input = "/* lowercase start */\nnode\n";
        let (doc, version) =
            parse_kdl(input, Some(KdlVersion::V1), true).expect("it to parse valid kdl");
        let config = KdlFmtConfig {
            normalize_single_line_block_comments: true,
            ..KdlFmtConfig::default()
        };
        let formatted = format_kdl(doc, &config, version);
        assert!(
            formatted.contains("// Lowercase start"),
            "expected capitalised // comment after normalize, got: {formatted:?}"
        );
    }

    #[test]
    fn it_should_not_normalize_multiline_block_comment() {
        // A /* */ that spans multiple lines must not be converted to //.
        let input = "/*\nMulti-line content\n*/\nnode\n";
        let (doc, version) =
            parse_kdl(input, Some(KdlVersion::V1), true).expect("it to parse valid kdl");
        let config = KdlFmtConfig {
            normalize_single_line_block_comments: true,
            ..KdlFmtConfig::default()
        };
        let formatted = format_kdl(doc, &config, version);
        assert!(
            formatted.contains("/*"),
            "expected multi-line block comment kept as /*, got: {formatted:?}"
        );
        assert!(
            !formatted.contains("// Multi"),
            "expected no // conversion of multi-line block, got: {formatted:?}"
        );
    }

    #[test]
    fn it_should_preserve_comments_before_first_entry_during_justification() {
        // If justify_first_property is true, it shouldn't delete comments between
        // the node name and the first entry.
        let input = "node /* important */ key=\"val\"\n";
        let (doc, version) =
            parse_kdl(input, Some(KdlVersion::V1), true).expect("it to parse valid kdl");
        let config = KdlFmtConfig {
            justify_first_property: true,
            ..KdlFmtConfig::default()
        };
        let formatted = format_kdl(doc, &config, version);
        assert!(
            formatted.contains("/* important */"),
            "expected comment preserved, got: {formatted:?}"
        );
    }

    #[test]
    fn it_should_handle_deeply_nested_nodes_beyond_config_limit() {
        // Config only goes to level 5. Ensure level 6 doesn't crash and uses reasonable defaults.
        let input = "l1 {\n l2 {\n  l3 {\n   l4 {\n    l5 {\n     l6\n    }\n   }\n  }\n }\n}\n";
        let (doc, version) =
            parse_kdl(input, Some(KdlVersion::V1), true).expect("it to parse valid kdl");
        let config = KdlFmtConfig::default();
        let _formatted = format_kdl(doc, &config, version);
        // If it didn't panic, it's at least safe.
    }

    #[test]
    fn it_should_format_slashdash_nodes() {
        let input = "parent {\n    /- commented-node {\n        child\n    }\n    active-node\n}\n";
        let (doc, version) =
            parse_kdl(input, Some(KdlVersion::V1), true).expect("it to parse valid kdl");
        let config = KdlFmtConfig::default();
        let formatted = format_kdl(doc, &config, version);
        assert!(
            formatted.contains("/- commented-node"),
            "expected commented node preserved, got: {formatted:?}"
        );
        assert!(
            formatted.contains("child"),
            "expected content inside commented node preserved, got: {formatted:?}"
        );
    }

    #[test]
    fn it_should_round_trip_expression_strings_in_v2() {
        let input = "rule `request.auth.claims.sub` match=```\nrequest.auth != nil\n```\n";
        let (doc, version) =
            parse_kdl(input, Some(KdlVersion::V2), true).expect("it to parse valid kdl");
        let formatted = format_kdl(doc, &KdlFmtConfig::default(), version);

        assert_eq!(formatted, input);
    }

    #[test]
    fn it_should_round_trip_expression_string_properties_in_v2() {
        let input =
            "policy subject=`request.auth.claims.sub` match=```\nrequest.auth != nil\n```\n";
        let (doc, version) =
            parse_kdl(input, Some(KdlVersion::V2), true).expect("it to parse valid kdl");
        let formatted = format_kdl(doc, &KdlFmtConfig::default(), version);

        assert_eq!(formatted, input);
    }

    #[test]
    fn it_should_round_trip_escaped_backticks_in_expression_strings() {
        let input = "rule `has(\\`quoted\\`) && ok`\n";
        let (doc, version) =
            parse_kdl(input, Some(KdlVersion::V2), true).expect("it to parse valid kdl");
        let formatted = format_kdl(doc, &KdlFmtConfig::default(), version);

        assert_eq!(formatted, input);
    }

    #[test]
    fn it_should_keep_multiline_expression_strings_multiline_when_single_line_content() {
        let input = "rule match=```\nrequest.auth != nil\n```\n";
        let (doc, version) =
            parse_kdl(input, Some(KdlVersion::V2), true).expect("it to parse valid kdl");
        let formatted = format_kdl(doc, &KdlFmtConfig::default(), version);

        assert_eq!(formatted, input);
    }
}
