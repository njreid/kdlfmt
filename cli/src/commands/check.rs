use rayon::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::{
    cli::{FormatCommandArguments, read_stdin},
    error::KdlFmtError,
    fs::{KDL_FILE_EXTENSION, setup_walker},
    terminal::print_check_changed_file,
};
use kdlfmt_core::{KdlFmtConfig, format_kdl, parse_kdl};

#[inline]
fn run_from_stdin(args: &FormatCommandArguments, config: &KdlFmtConfig) -> Result<(), KdlFmtError> {
    let input = read_stdin().map_err(KdlFmtError::ReadStdin)?;

    let (parsed, version) = parse_kdl(&input, args.kdl_version, !args.no_expression_strings)
        .map_err(|error| KdlFmtError::ParseKdl(None, error.into()))?;

    let cache = dashmap::DashMap::new();
    let actual_config =
        config.get_editorconfig_or_default(&std::path::PathBuf::from("dummy.kdl"), &cache);

    let formatted = format_kdl(parsed, &actual_config, version);

    if input == formatted {
        Ok(())
    } else {
        Err(KdlFmtError::CheckModeChanges)
    }
}

#[inline]
pub fn run_from_args(
    args: &FormatCommandArguments,
    config: &KdlFmtConfig,
) -> Result<(), KdlFmtError> {
    let mut paths = Vec::new();

    for path in &args.input {
        paths.push(std::path::PathBuf::from(path));
    }

    if paths.is_empty() {
        return Ok(());
    }

    let walker = setup_walker(paths);

    let file_count = AtomicUsize::new(0);
    let cache = dashmap::DashMap::new();

    walker
        .par_bridge()
        .map(|entry| {
            let file_path = entry.path();

            if file_path.is_file()
                && file_path
                    .extension()
                    .is_some_and(|ft| ft == KDL_FILE_EXTENSION)
            {
                let input = std::fs::read_to_string(file_path).map_err(KdlFmtError::Io)?;

                let (parsed, version) =
                    parse_kdl(&input, args.kdl_version, !args.no_expression_strings).map_err(
                        |error| KdlFmtError::ParseKdl(Some(file_path.to_path_buf()), error.into()),
                    )?;

                let actual_config = config.get_editorconfig_or_default(file_path, &cache);

                let formatted = format_kdl(parsed, &actual_config, version);

                if formatted != input {
                    print_check_changed_file(file_path);

                    file_count.fetch_add(1, Ordering::SeqCst);
                }
            }
            Ok(())
        })
        .collect::<Result<Vec<()>, KdlFmtError>>()?;

    if file_count.into_inner() == 0 {
        Ok(())
    } else {
        Err(KdlFmtError::CheckModeChanges)
    }
}

#[inline]
pub fn run(args: &FormatCommandArguments, config: &KdlFmtConfig) -> Result<(), KdlFmtError> {
    if args.stdin || (args.input.len() == 1 && args.input.first().is_some_and(|v| v == "-")) {
        run_from_stdin(args, config)
    } else {
        run_from_args(args, config)
    }
}
