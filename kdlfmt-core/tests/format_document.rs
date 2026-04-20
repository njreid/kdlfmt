use kdlfmt_core::{KdlFmtConfig, KdlVersion, format_check, format_document};

#[test]
fn format_document_returns_formatted_string() {
    let input = "node 1   2    3\n";
    let out =
        format_document(input, KdlVersion::V2, &KdlFmtConfig::default()).expect("formats cleanly");
    let out2 = format_document(&out, KdlVersion::V2, &KdlFmtConfig::default())
        .expect("second pass formats cleanly");
    assert_eq!(out, out2);
}

#[test]
fn format_check_reports_needs_format() {
    let input = "node   1\n";
    let check =
        format_check(input, KdlVersion::V2, &KdlFmtConfig::default()).expect("checks cleanly");
    assert!(!check.was_formatted, "raw input is not already formatted");
    assert_ne!(check.would_write, input);
}

#[test]
fn format_check_reports_already_formatted() {
    let input = "node 1\n";
    let normalized = format_document(input, KdlVersion::V2, &KdlFmtConfig::default()).unwrap();
    let check = format_check(&normalized, KdlVersion::V2, &KdlFmtConfig::default()).unwrap();
    assert!(
        check.was_formatted,
        "normalized output is already formatted"
    );
    assert_eq!(check.would_write, normalized);
}
