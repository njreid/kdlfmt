use ec4rs::property::IndentStyle;
use kdl::{FormatConfig, KdlDocument};

use crate::{error::KdlFmtError, kdl::parse_kdl};

#[derive(Debug, Clone)]
pub struct KdlFmtConfig {
    pub(crate) from_kdlfmt_file: bool,
    pub indent: String,
    pub use_tabs: bool,
    /// Extra blank lines before comment-bearing nodes at each depth.
    /// Index 0 = level1 (top-level), index 4 = level5.
    pub newlines_before_comments: [u32; 5],
    pub justify_first_property: bool,
    /// Extra blank lines after the closing `}` of block nodes at each depth.
    /// Index 0 = level1 (top-level), index 4 = level5.
    pub newlines_after_close: [u32; 5],
    /// Convert runs of N or more consecutive `//` comment lines to `/* */` blocks.
    pub n_comment_lines_to_multiline: Option<u32>,
    /// Strip blank lines immediately before the closing `}` of every block.
    pub remove_trailing_blank_lines_in_blocks: bool,
    /// Collapse blocks with no child nodes to inline `{}`.
    pub collapse_empty_blocks: bool,
    /// Remove `/* */` block comments whose content is entirely blank/whitespace.
    pub strip_empty_block_comments: bool,
    /// Convert single-line `/* content */` block comments to `// content`.
    pub normalize_single_line_block_comments: bool,
}

impl Default for KdlFmtConfig {
    #[inline]
    fn default() -> Self {
        Self {
            from_kdlfmt_file: false,
            indent: FormatConfig::default().indent.to_string(),
            use_tabs: false,
            newlines_before_comments: [0; 5],
            justify_first_property: false,
            newlines_after_close: [0; 5],
            n_comment_lines_to_multiline: None,
            remove_trailing_blank_lines_in_blocks: false,
            collapse_empty_blocks: false,
            strip_empty_block_comments: false,
            normalize_single_line_block_comments: false,
        }
    }
}

impl KdlFmtConfig {
    #[inline]
    pub const fn filename() -> &'static str {
        "kdlfmt.kdl"
    }

    #[inline]
    fn parse_config(config: &str) -> miette::Result<KdlDocument> {
        parse_kdl(config, None, true).map(|(doc, _version)| doc)
    }

    #[inline]
    pub fn load(path: Option<&std::path::PathBuf>) -> Result<Self, KdlFmtError> {
        let mut config = Self::default();

        let config_result = path.map_or_else(
            || std::fs::read_to_string(Self::filename()),
            std::fs::read_to_string,
        );

        if let Ok(config_str) = config_result {
            // TODO: custom parse error
            let doc = Self::parse_config(&config_str)
                .map_err(|error| KdlFmtError::ParseKdl(None, error))?;

            if doc
                .get_arg(Self::use_tabs_key())
                .and_then(kdl::KdlValue::as_bool)
                == Some(true)
            {
                config.use_tabs = true;
                config.indent = Self::get_indent(1, true);
                config.from_kdlfmt_file = true;
            }

            if let Some(indent_size) = doc
                .get_arg(Self::indent_size_key())
                .and_then(kdl::KdlValue::as_integer)
            {
                config.indent = Self::get_indent(indent_size.max(1) as usize, config.use_tabs);
                config.from_kdlfmt_file = true;
            }

            if let Some(node) = doc.get(Self::newlines_before_comments_key()) {
                if let Some(children) = node.children() {
                    for i in 1..=5usize {
                        let key = format!("level{i}");
                        if let Some(n) = children.get_arg(&key).and_then(kdl::KdlValue::as_integer)
                        {
                            config.newlines_before_comments[i - 1] = n.max(0) as u32;
                            config.from_kdlfmt_file = true;
                        }
                    }
                }
            }

            if doc
                .get_arg(Self::justify_first_property_key())
                .and_then(kdl::KdlValue::as_bool)
                == Some(true)
            {
                config.justify_first_property = true;
                config.from_kdlfmt_file = true;
            }

            if let Some(node) = doc.get(Self::newlines_after_close_key()) {
                if let Some(children) = node.children() {
                    for i in 1..=5usize {
                        let key = format!("level{i}");
                        if let Some(n) = children.get_arg(&key).and_then(kdl::KdlValue::as_integer)
                        {
                            config.newlines_after_close[i - 1] = n.clamp(0, 10) as u32;
                            config.from_kdlfmt_file = true;
                        }
                    }
                }
            }

            if let Some(n) = doc
                .get_arg(Self::n_comment_lines_to_multiline_key())
                .and_then(kdl::KdlValue::as_integer)
            {
                config.n_comment_lines_to_multiline = Some(n.max(2) as u32);
                config.from_kdlfmt_file = true;
            }

            if doc
                .get_arg(Self::remove_trailing_blank_lines_key())
                .and_then(kdl::KdlValue::as_bool)
                == Some(true)
            {
                config.remove_trailing_blank_lines_in_blocks = true;
                config.from_kdlfmt_file = true;
            }

            if doc
                .get_arg(Self::collapse_empty_blocks_key())
                .and_then(kdl::KdlValue::as_bool)
                == Some(true)
            {
                config.collapse_empty_blocks = true;
                config.from_kdlfmt_file = true;
            }

            if doc
                .get_arg(Self::strip_empty_block_comments_key())
                .and_then(kdl::KdlValue::as_bool)
                == Some(true)
            {
                config.strip_empty_block_comments = true;
                config.from_kdlfmt_file = true;
            }

            if doc
                .get_arg(Self::normalize_single_line_block_comments_key())
                .and_then(kdl::KdlValue::as_bool)
                == Some(true)
            {
                config.normalize_single_line_block_comments = true;
                config.from_kdlfmt_file = true;
            }
        }

        Ok(config)
    }

    #[inline]
    pub fn get_indent(indent: usize, use_tabs: bool) -> String {
        let mut spaces = String::new();

        for _ in 0..indent {
            if use_tabs {
                spaces.push('\t');
            } else {
                spaces.push(' ');
            }
        }

        spaces
    }

    #[inline]
    pub const fn indent_size_key() -> &'static str {
        "indent_size"
    }

    #[inline]
    pub const fn use_tabs_key() -> &'static str {
        "use_tabs"
    }

    #[inline]
    pub const fn newlines_before_comments_key() -> &'static str {
        "newlines_before_comments"
    }

    #[inline]
    pub const fn justify_first_property_key() -> &'static str {
        "justify_first_property"
    }

    #[inline]
    pub const fn newlines_after_close_key() -> &'static str {
        "newlines_after_close"
    }

    #[inline]
    pub const fn n_comment_lines_to_multiline_key() -> &'static str {
        "n_comment_lines_to_multiline"
    }

    #[inline]
    pub const fn remove_trailing_blank_lines_key() -> &'static str {
        "remove_trailing_blank_lines_in_blocks"
    }

    #[inline]
    pub const fn collapse_empty_blocks_key() -> &'static str {
        "collapse_empty_blocks"
    }

    #[inline]
    pub const fn strip_empty_block_comments_key() -> &'static str {
        "strip_empty_block_comments"
    }

    #[inline]
    pub const fn normalize_single_line_block_comments_key() -> &'static str {
        "normalize_single_line_block_comments"
    }

    #[inline]
    pub fn get_editorconfig_or_default(
        &self,
        path: &std::path::Path,
        cache: &dashmap::DashMap<std::path::PathBuf, Self>,
    ) -> Self {
        if self.from_kdlfmt_file {
            return self.clone();
        }

        if let Some(parent) = path.parent() {
            if let Some(cached) = cache.get(parent) {
                return cached.clone();
            }
        }

        if let Ok(mut properties) = ec4rs::properties_of(path) {
            properties.use_fallbacks();

            let use_tabs = properties
                .get::<IndentStyle>()
                .is_ok_and(|indent_style| matches!(indent_style, IndentStyle::Tabs));

            let indent_size = properties.get::<ec4rs::property::IndentSize>().map_or(
                if use_tabs { 1 } else { self.indent.len() },
                |value| match value {
                    ec4rs::property::IndentSize::Value(value) => value,
                    ec4rs::property::IndentSize::UseTabWidth => {
                        if let Ok(ec4rs::property::TabWidth::Value(value)) =
                            properties.get::<ec4rs::property::TabWidth>()
                        {
                            value
                        } else {
                            1
                        }
                    }
                },
            );

            let indent = Self::get_indent(indent_size, use_tabs);

            let config = Self {
                use_tabs,
                indent,
                from_kdlfmt_file: false,
                newlines_before_comments: self.newlines_before_comments,
                justify_first_property: self.justify_first_property,
                newlines_after_close: self.newlines_after_close,
                n_comment_lines_to_multiline: self.n_comment_lines_to_multiline,
                remove_trailing_blank_lines_in_blocks: self.remove_trailing_blank_lines_in_blocks,
                collapse_empty_blocks: self.collapse_empty_blocks,
                strip_empty_block_comments: self.strip_empty_block_comments,
                normalize_single_line_block_comments: self.normalize_single_line_block_comments,
            };

            if let Some(parent) = path.parent() {
                cache.insert(parent.to_path_buf(), config.clone());
            }

            return config;
        }

        self.clone()
    }

    #[inline]
    pub fn get_formatter_config(&self) -> FormatConfig<'_> {
        kdl::FormatConfig::builder().indent(&self.indent).build()
    }
}
