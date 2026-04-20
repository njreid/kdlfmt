use kdlfmt_core::KdlFmtConfig;

#[test]
fn default_config_has_four_space_indent() {
    let cfg = KdlFmtConfig::default();
    assert_eq!(cfg.indent, "    ");
    assert!(!cfg.use_tabs);
}

#[test]
fn config_filename_constant_is_kdlfmt_kdl() {
    assert_eq!(KdlFmtConfig::filename(), "kdlfmt.kdl");
}
