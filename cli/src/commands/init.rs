use crate::{
    cli::InitCommandArguments,
    config::KdlFmtConfig,
    error::KdlFmtError,
    kdl::format_kdl,
};

#[inline]
pub fn run(args: &InitCommandArguments) -> Result<(), KdlFmtError> {
    let config_path = std::path::Path::new(KdlFmtConfig::filename());

    if !args.force && config_path.try_exists().map_err(KdlFmtError::Io)? {
        return Err(KdlFmtError::ConfigAlreadyExist);
    }

    let config = KdlFmtConfig::default();

    let mut doc = kdl::KdlDocument::new();

    let mut indent_size_node = kdl::KdlNode::new(KdlFmtConfig::indent_size_key());
    indent_size_node.push(kdl::KdlEntry::from(kdl::KdlValue::Integer(
        config.indent.len() as i128,
    )));
    doc.nodes_mut().push(indent_size_node);

    let mut use_tab_node = kdl::KdlNode::new(KdlFmtConfig::use_tabs_key());
    use_tab_node.push(kdl::KdlEntry::from(kdl::KdlValue::Bool(config.use_tabs)));
    doc.nodes_mut().push(use_tab_node);

    let mut newlines_node = kdl::KdlNode::new(KdlFmtConfig::newlines_before_comments_key());
    let mut newlines_children = kdl::KdlDocument::new();
    for (i, &n) in config.newlines_before_comments.iter().enumerate() {
        let mut level_node = kdl::KdlNode::new(format!("level{}", i + 1));
        level_node.push(kdl::KdlEntry::from(kdl::KdlValue::Integer(n as i128)));
        newlines_children.nodes_mut().push(level_node);
    }
    newlines_node.set_children(newlines_children);
    doc.nodes_mut().push(newlines_node);

    let mut justify_node = kdl::KdlNode::new(KdlFmtConfig::justify_first_property_key());
    justify_node.push(kdl::KdlEntry::from(kdl::KdlValue::Bool(
        config.justify_first_property,
    )));
    doc.nodes_mut().push(justify_node);

    let mut trailing_node = kdl::KdlNode::new(KdlFmtConfig::remove_trailing_blank_lines_key());
    trailing_node.push(kdl::KdlEntry::from(kdl::KdlValue::Bool(
        config.remove_trailing_blank_lines_in_blocks,
    )));
    doc.nodes_mut().push(trailing_node);

    let mut collapse_node = kdl::KdlNode::new(KdlFmtConfig::collapse_empty_blocks_key());
    collapse_node.push(kdl::KdlEntry::from(kdl::KdlValue::Bool(
        config.collapse_empty_blocks,
    )));
    doc.nodes_mut().push(collapse_node);

    let doc = format_kdl(doc, &KdlFmtConfig::default(), args.kdl_version.unwrap_or_default());

    std::fs::write(config_path, &doc).map_err(KdlFmtError::Io)
}
