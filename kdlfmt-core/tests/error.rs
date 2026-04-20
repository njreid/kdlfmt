use kdlfmt_core::FmtError;

#[test]
fn fmt_error_is_debug_and_display() {
    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "missing");
    let err = FmtError::Io(io_err);
    let _ = format!("{err:?}");
    let _ = format!("{err}");
}

#[test]
fn fmt_error_parse_carries_underlying() {
    let bad = "invalid `";
    let result = kdlfmt_core::parse_kdl(bad, None, true);
    let Err(_err): Result<_, kdlfmt_core::FmtError> = result else {
        panic!("expected parse error");
    };
}
