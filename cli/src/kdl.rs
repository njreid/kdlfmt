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

    // In KDL v2, node-space excludes single-line-comment, so the parser stores
    // inline comments inside `terminator` (e.g. "// comment\n") rather than
    // `before_terminator`. autoformat_config() discards them by replacing any
    // non-newline terminator with "\n". Rescue them first.
    rescue_terminator_comments(input.nodes_mut());

    input.autoformat_config(&format_config);

    lint_comments_in_doc(&mut input);

    if config.newlines_before_comments != [0; 5] {
        apply_newlines_before_comments(&mut input, &config.newlines_before_comments, 0);
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

    apply_comment_indentation(&mut input, config, 0);

    if let Some(threshold) = config.n_comment_lines_to_multiline {
        apply_comment_lines_to_multiline(&mut input, threshold);
    }

    if config.collapse_empty_blocks {
        apply_collapse_empty_blocks(&mut input);
    }

    if config.newlines_after_close != [0; 5] {
        apply_newlines_after_close(&mut input, &config.newlines_after_close, 0);
    }

    let output = input.to_string();

    if config.remove_trailing_blank_lines_in_blocks {
        remove_trailing_blank_lines_before_braces(&output)
    } else {
        output
    }
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
                let comment = fmt.terminator
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

/// Process a single `//` comment line: trim trailing whitespace, normalize to
/// exactly one space after `//`, and capitalize the first word.
/// Only treats `//` as a comment marker when everything before it is whitespace,
/// so `https://url` in non-comment contexts is left untouched.
fn lint_line_comment(line: &str) -> String {
    if let Some(idx) = line.find("//") {
        let prefix = &line[..idx];
        // Only a real comment if the prefix is purely whitespace.
        if !prefix.chars().all(|c| c.is_whitespace()) {
            return line.to_string();
        }
        let trimmed = line.trim_end();
        let rest = &trimmed[(idx + 2).min(trimmed.len())..];
        let rest_trimmed = rest.trim_start();
        if rest_trimmed.is_empty() {
            format!("{prefix}//")
        } else {
            format!("{prefix}// {}", capitalize_first(rest_trimmed))
        }
    } else {
        line.to_string()
    }
}

/// Apply `lint_line_comment` to every `//` comment line in a trivia string.
/// Lines inside `/* */` blocks are left untouched — they are block content,
/// not `//` comments.
fn lint_trivia(s: &str) -> String {
    let mut result: Vec<String> = Vec::new();
    let mut in_block = false;
    for line in s.split('\n') {
        let trimmed = line.trim_start();
        if !in_block {
            if trimmed.starts_with("/*") {
                if !trimmed[2..].contains("*/") {
                    in_block = true;
                }
                result.push(line.to_string());
            } else {
                result.push(lint_line_comment(line));
            }
        } else {
            if trimmed.starts_with("*/") {
                in_block = false;
            }
            result.push(line.to_string());
        }
    }
    result.join("\n")
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

/// Insert blank lines before nodes whose leading trivia contains a comment,
/// at each depth level. Skips the first node at each level.
fn apply_newlines_before_comments(doc: &mut kdl::KdlDocument, config: &[u32; 5], depth: usize) {
    if depth < 5 {
        let n = config[depth];
        if n > 0 {
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
    }
    for node in doc.nodes_mut() {
        if let Some(children) = node.children_mut() {
            apply_newlines_before_comments(children, config, depth + 1);
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

/// Collapse blocks that contain no child nodes to inline `{}`.
fn apply_collapse_empty_blocks(doc: &mut kdl::KdlDocument) {
    for node in doc.nodes_mut() {
        if let Some(children) = node.children_mut() {
            if children.nodes().is_empty() {
                if let Some(fmt) = children.format_mut() {
                    fmt.leading = String::new();
                    fmt.trailing = String::new();
                }
            } else {
                apply_collapse_empty_blocks(children);
            }
        }
    }
}

/// Insert blank lines after the closing `}` of block nodes at each depth level.
/// `config[0]` applies to top-level (level1) nodes, `config[4]` to level5.
fn apply_newlines_after_close(doc: &mut kdl::KdlDocument, config: &[u32; 5], depth: usize) {
    if depth < 5 {
        let n = config[depth];
        if n > 0 {
            let extra = "\n".repeat(n as usize);
            let nodes = doc.nodes_mut();
            let len = nodes.len();
            for i in 0..len {
                if nodes[i].children().is_some() && i + 1 < len {
                    if let Some(fmt) = nodes[i + 1].format_mut() {
                        fmt.leading = format!("{}{}", extra, fmt.leading);
                    }
                }
            }
        }
    }
    for node in doc.nodes_mut() {
        if let Some(children) = node.children_mut() {
            apply_newlines_after_close(children, config, depth + 1);
        }
    }
}

/// Fix the indentation of comment lines in a trivia string:
/// - `//` lines are re-indented to `indent`
/// - `/*` opening lines are re-indented to `indent`
/// - Content lines (` *`) and closing ` */` inside a block get `indent + " "`
fn fix_comment_indentation_in_trivia(s: &str, indent: &str) -> String {
    let mut result: Vec<String> = Vec::new();
    let mut in_block = false;
    for line in s.split('\n') {
        let trimmed = line.trim_start();
        if !in_block {
            if trimmed.starts_with("//") {
                result.push(format!("{indent}{trimmed}"));
            } else if trimmed.starts_with("/*") {
                result.push(format!("{indent}{trimmed}"));
                // Single-line /* ... */ doesn't open a block state.
                if !trimmed[2..].contains("*/") {
                    in_block = true;
                }
            } else {
                result.push(line.to_string());
            }
        } else {
            // Inside a /* */ block: re-indent and track close.
            if trimmed.starts_with("*/") {
                result.push(format!("{indent} */"));
                in_block = false;
            } else {
                result.push(format!("{indent} {trimmed}"));
            }
        }
    }
    result.join("\n")
}

/// Ensure every `//` comment line in every trivia field is indented to match
/// the depth of its surrounding nodes.
fn apply_comment_indentation(doc: &mut kdl::KdlDocument, config: &KdlFmtConfig, depth: usize) {
    let indent = config.indent.repeat(depth);
    if let Some(fmt) = doc.format_mut() {
        fmt.leading = fix_comment_indentation_in_trivia(&fmt.leading, &indent);
        fmt.trailing = fix_comment_indentation_in_trivia(&fmt.trailing, &indent);
    }
    for node in doc.nodes_mut() {
        if let Some(fmt) = node.format_mut() {
            fmt.leading = fix_comment_indentation_in_trivia(&fmt.leading, &indent);
        }
        if let Some(children) = node.children_mut() {
            apply_comment_indentation(children, config, depth + 1);
        }
    }
}

/// Convert runs of `threshold` or more consecutive `//` comment lines within a
/// trivia string into a `/* … */` block comment, preserving indentation.
fn convert_comment_run_to_multiline(s: &str, threshold: u32) -> String {
    let lines: Vec<&str> = s.split('\n').collect();
    let mut result: Vec<String> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim_start();
        if trimmed.starts_with("//") {
            let line_indent = &lines[i][..lines[i].len() - trimmed.len()];
            let mut run = vec![lines[i]];
            let mut j = i + 1;
            while j < lines.len() && lines[j].trim_start().starts_with("//") {
                run.push(lines[j]);
                j += 1;
            }
            if run.len() >= threshold as usize {
                result.push(format!("{line_indent}/*"));
                for &cl in &run {
                    let t = cl.trim_start();
                    let content = t[2..].trim_start();
                    if content.is_empty() {
                        result.push(format!("{line_indent} *"));
                    } else {
                        result.push(format!("{line_indent} * {content}"));
                    }
                }
                result.push(format!("{line_indent} */"));
            } else {
                for &cl in &run {
                    result.push(cl.to_string());
                }
            }
            i = j;
        } else {
            result.push(lines[i].to_string());
            i += 1;
        }
    }
    result.join("\n")
}

/// Merge adjacent comment items in a trivia string into a single `/* */` block.
/// Two or more consecutive comment items (each `//` line counts as one item,
/// each `/* */` block counts as one item) with no blank line between them are
/// combined. Runs with only one item are left untouched.
fn merge_adjacent_comment_blocks_in_trivia(s: &str) -> String {
    let lines: Vec<&str> = s.split('\n').collect();
    let mut result: Vec<String> = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let trimmed = lines[i].trim_start();

        if trimmed.starts_with("//") || trimmed.starts_with("/*") {
            let indent = &lines[i][..lines[i].len() - trimmed.len()];
            let run_start = i;
            let mut all_contents: Vec<String> = Vec::new();
            let mut item_count: usize = 0;
            let mut has_block = false; // true once a /* */ item is seen
            let mut in_block = false;

            while i < lines.len() {
                let t = lines[i].trim_start();
                if !in_block && t.starts_with("//") {
                    all_contents.push(t[2..].trim_start().to_string());
                    item_count += 1;
                    i += 1;
                } else if !in_block && t.starts_with("/*") {
                    let after_open = t[2..].trim_start();
                    if let Some(close) = after_open.find("*/") {
                        // Single-line /* content */
                        let content = after_open[..close].trim_end().to_string();
                        if !content.is_empty() {
                            all_contents.push(content);
                        }
                        item_count += 1;
                        has_block = true;
                        i += 1;
                    } else {
                        // Opening of a multi-line block
                        in_block = true;
                        if !after_open.is_empty() {
                            all_contents.push(after_open.to_string());
                        }
                        i += 1;
                    }
                } else if in_block {
                    if t.starts_with("*/") {
                        in_block = false;
                        item_count += 1;
                        has_block = true;
                        i += 1;
                    } else if t.starts_with("* ") {
                        all_contents.push(t[2..].to_string());
                        i += 1;
                    } else if t == "*" {
                        all_contents.push(String::new());
                        i += 1;
                    } else {
                        all_contents.push(t.to_string());
                        i += 1;
                    }
                } else {
                    break;
                }
            }
            if in_block {
                item_count += 1; // unclosed — edge case guard
            }

            if item_count >= 2 && has_block {
                result.push(format!("{indent}/*"));
                for content in &all_contents {
                    if content.is_empty() {
                        result.push(format!("{indent} *"));
                    } else {
                        result.push(format!("{indent} * {content}"));
                    }
                }
                result.push(format!("{indent} */"));
            } else {
                for j in run_start..i {
                    result.push(lines[j].to_string());
                }
            }
        } else {
            result.push(lines[i].to_string());
            i += 1;
        }
    }

    result.join("\n")
}

/// Apply `convert_comment_run_to_multiline` and `merge_adjacent_comment_blocks_in_trivia`
/// to all trivia fields in the document.
fn apply_comment_lines_to_multiline(doc: &mut kdl::KdlDocument, threshold: u32) {
    if let Some(fmt) = doc.format_mut() {
        fmt.leading = convert_comment_run_to_multiline(&fmt.leading, threshold);
        fmt.leading = merge_adjacent_comment_blocks_in_trivia(&fmt.leading);
        fmt.trailing = convert_comment_run_to_multiline(&fmt.trailing, threshold);
        fmt.trailing = merge_adjacent_comment_blocks_in_trivia(&fmt.trailing);
    }
    for node in doc.nodes_mut() {
        if let Some(fmt) = node.format_mut() {
            fmt.leading = convert_comment_run_to_multiline(&fmt.leading, threshold);
            fmt.leading = merge_adjacent_comment_blocks_in_trivia(&fmt.leading);
        }
        if let Some(children) = node.children_mut() {
            apply_comment_lines_to_multiline(children, threshold);
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
    fn it_should_rescue_and_lint_inline_comment_v2() {
        // In KDL v2 the parser puts inline comments into `terminator`, not
        // `before_terminator`. Our rescue pass moves them before autoformat discards them.
        let input = "node // inline lowercase  \n";
        let (doc, version) =
            parse_kdl(input, Some(KdlVersion::V2)).expect("it to parse valid kdl");
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
            parse_kdl(input, Some(KdlVersion::V2)).expect("it to parse valid kdl");
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
    fn it_should_add_newlines_after_close_level1() {
        // With newlines_after_close level1=1, a blank line is inserted after
        // the closing brace of each top-level block node.
        let input = "first {\n    child\n}\nsecond\nthird {\n    child\n}\nfourth\n";
        let (doc, version) =
            parse_kdl(input, Some(KdlVersion::V1)).expect("it to parse valid kdl");
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
            parse_kdl(input, Some(KdlVersion::V1)).expect("it to parse valid kdl");
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
            parse_kdl(input, Some(KdlVersion::V1)).expect("it to parse valid kdl");
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
            formatted.contains(" * Line one"),
            "expected * Line one, got: {formatted:?}"
        );
        assert!(
            formatted.contains(" */"),
            "expected multiline comment close, got: {formatted:?}"
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
            parse_kdl(input, Some(KdlVersion::V1)).expect("it to parse valid kdl");
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
    fn it_should_merge_two_adjacent_multiline_blocks() {
        // Two /* */ blocks with no blank line → merged into one.
        let input = "/* First block */\n/* Second block */\nnode\n";
        let (doc, version) =
            parse_kdl(input, Some(KdlVersion::V1)).expect("it to parse valid kdl");
        let config = KdlFmtConfig {
            n_comment_lines_to_multiline: Some(3),
            ..KdlFmtConfig::default()
        };
        let formatted = format_kdl(doc, &config, version);
        assert_eq!(formatted.matches("/*").count(), 1, "expected single /* open, got: {formatted:?}");
        assert!(formatted.contains(" * First block"), "got: {formatted:?}");
        assert!(formatted.contains(" * Second block"), "got: {formatted:?}");
    }

    #[test]
    fn it_should_merge_single_line_comment_adjacent_to_block() {
        // A // line immediately before a /* */ block → merged.
        // threshold=3 so the lone // would not be converted on its own.
        let input = "// Preface\n/* Body\n * content\n */\nnode\n";
        let (doc, version) =
            parse_kdl(input, Some(KdlVersion::V1)).expect("it to parse valid kdl");
        let config = KdlFmtConfig {
            n_comment_lines_to_multiline: Some(3),
            ..KdlFmtConfig::default()
        };
        let formatted = format_kdl(doc, &config, version);
        assert_eq!(formatted.matches("/*").count(), 1, "expected single block, got: {formatted:?}");
        assert!(formatted.contains(" * Preface"), "got: {formatted:?}");
        assert!(formatted.contains(" * content"), "got: {formatted:?}");
    }

    #[test]
    fn it_should_merge_comments_even_across_autoformat_blank_removal() {
        // autoformat removes blank lines inside leading trivia, so a blank line
        // between a // comment and a /* */ block does not prevent merging.
        let input = "// First\n\n/* Second */\nnode\n";
        let (doc, version) =
            parse_kdl(input, Some(KdlVersion::V1)).expect("it to parse valid kdl");
        let config = KdlFmtConfig {
            n_comment_lines_to_multiline: Some(3),
            ..KdlFmtConfig::default()
        };
        let formatted = format_kdl(doc, &config, version);
        // autoformat collapses the blank line, so both end up adjacent and merge.
        assert_eq!(formatted.matches("/*").count(), 1, "expected single merged block, got: {formatted:?}");
        assert!(formatted.contains(" * First"), "got: {formatted:?}");
        assert!(formatted.contains(" * Second"), "got: {formatted:?}");
    }

    #[test]
    fn it_should_not_convert_comment_lines_below_threshold() {
        // Two // lines do not meet threshold of 3 — stay as //.
        let input = "// Line one\n// Line two\nnode\n";
        let (doc, version) =
            parse_kdl(input, Some(KdlVersion::V1)).expect("it to parse valid kdl");
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
            parse_kdl(input, Some(KdlVersion::V1)).expect("it to parse valid kdl");
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
            parse_kdl(input, Some(KdlVersion::V1)).expect("it to parse valid kdl");
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
            parse_kdl(input, Some(KdlVersion::V1)).expect("it to parse valid kdl");
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
            parse_kdl(input, Some(KdlVersion::V1)).expect("it to parse valid kdl");
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
            parse_kdl(input, Some(KdlVersion::V1)).expect("it to parse valid kdl");
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
            parse_kdl(input, Some(KdlVersion::V1)).expect("it to parse valid kdl");
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
            formatted.contains("     * Content line"),
            "expected content line at correct indent, got: {formatted:?}"
        );
    }

    #[test]
    fn it_should_indent_comments_to_node_level() {
        // A badly-indented comment inside a block should be fixed to match the child depth.
        let input = "parent {\n// bad indent\n    child\n}\n";
        let (doc, version) =
            parse_kdl(input, Some(KdlVersion::V1)).expect("it to parse valid kdl");
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
            parse_kdl(input, Some(KdlVersion::V1)).expect("it to parse valid kdl");
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
            parse_kdl(input, Some(KdlVersion::V1)).expect("it to parse valid kdl");
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
}
