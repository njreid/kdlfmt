use kdlfmt_core::{KdlFmtConfig, KdlVersion, format_kdl, parse_kdl};

#[test]
fn it_should_preserve_comments_before_first_entry_during_justification() {
    // If justify_first_property is true, it shouldn't delete comments between
    // the node name and the first entry.
    let input = "node /* important */ key=\"val\"\n";
    let (doc, version) =
        parse_kdl(input, Some(KdlVersion::V1), true).expect("it to parse valid kdl");
    let mut config = KdlFmtConfig::default();
    config.justify_first_property = true;
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
