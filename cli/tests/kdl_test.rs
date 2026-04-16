use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
};

const VALID_V2_SKIPLIST: &[&str] = &[
    "zero_space_before_slashdash_arg.kdl",
    "zero_space_before_slashdash_children.kdl",
    "zero_space_before_slashdash_prop.kdl",
];

const INVALID_V2_SKIPLIST: &[&str] = &[
    "multiline_raw_string_single_line_err.kdl",
    "unicode_delete.kdl",
];

fn kdlfmt_command() -> assert_cmd::Command {
    assert_cmd::cargo_bin_cmd!("kdlfmt")
}

fn fixture_paths(dir: &str, skiplist: &[&str]) -> std::io::Result<Vec<PathBuf>> {
    let mut paths = fs::read_dir(fixtures_root().join(dir))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()?;

    paths.retain(|path| path.extension() == Some(OsStr::new("kdl")));
    paths.retain(|path| {
        let name = path.file_name().and_then(OsStr::to_str).unwrap_or_default();

        !skiplist.contains(&name)
    });
    paths.sort();

    Ok(paths)
}

fn fixtures_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("kdl-test")
        .join("test_cases")
}

fn fixture_name(path: &Path) -> &str {
    path.file_name()
        .and_then(OsStr::to_str)
        .unwrap_or("<unknown>")
}

#[test]
fn kdl_test_valid_cases_format_as_v2() -> std::io::Result<()> {
    for path in fixture_paths("valid", VALID_V2_SKIPLIST)? {
        let input = fs::read_to_string(&path)?;
        let case_name = fixture_name(&path).to_owned();

        let output = kdlfmt_command()
            .arg("format")
            .arg("--kdl-version")
            .arg("v2")
            .arg("--stdin")
            .write_stdin(input)
            .output()?;

        assert!(
            output.status.success(),
            "{} should format successfully: {}",
            case_name,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(())
}

#[test]
fn kdl_test_invalid_cases_fail_to_format_as_v2() -> std::io::Result<()> {
    for path in fixture_paths("invalid", INVALID_V2_SKIPLIST)? {
        let input = fs::read_to_string(&path)?;
        let case_name = fixture_name(&path).to_owned();

        let output = kdlfmt_command()
            .arg("format")
            .arg("--kdl-version")
            .arg("v2")
            .arg("--stdin")
            .write_stdin(input)
            .output()?;

        assert!(
            !output.status.success(),
            "{} should fail to format, but succeeded with stdout: {}",
            case_name,
            String::from_utf8_lossy(&output.stdout)
        );
    }

    Ok(())
}
