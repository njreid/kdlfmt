use kdlfmt_core::{format_kdl, parse_kdl, KdlFmtConfig, KdlVersion};

#[test]
fn parse_and_format_roundtrips_simple_kdl() {
    let input = "node 1 2 3\n";
    let (doc, version) = parse_kdl(input, Some(KdlVersion::V2), false).expect("valid kdl parses");
    let cfg = KdlFmtConfig::default();
    let out = format_kdl(doc, &cfg, version);
    assert!(out.contains("node"));
}

#[test]
fn reject_expression_strings_catches_backtick_args() {
    let input = "node `expr`\n";
    let err = parse_kdl(input, Some(KdlVersion::V2), false);
    assert!(
        err.is_err(),
        "backtick arg should be rejected when allow_expression_strings=false"
    );
}
