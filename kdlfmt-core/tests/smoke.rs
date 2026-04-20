#[test]
fn crate_compiles_and_exports_version() {
    assert!(!kdlfmt_core::VERSION.is_empty());
}

#[test]
fn kdl_version_is_exported() {
    use kdlfmt_core::KdlVersion;

    let _: KdlVersion = KdlVersion::V1;
    let _: KdlVersion = KdlVersion::V2;
}
