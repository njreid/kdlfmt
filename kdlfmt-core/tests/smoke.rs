#[test]
fn crate_compiles_and_exports_version() {
    assert!(!kdlfmt_core::VERSION.is_empty());
}
