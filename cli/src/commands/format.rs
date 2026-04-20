use rayon::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::{
    cli::{read_stdin, FormatCommandArguments},
    error::KdlFmtError,
    fs::{setup_walker, KDL_FILE_EXTENSION},
    terminal::{print_format_changed_file, print_format_finished, print_format_unchanged_file},
};
use kdlfmt_core::{format_kdl, parse_kdl, KdlFmtConfig};

#[inline]
fn run_from_stdin(args: &FormatCommandArguments, config: &KdlFmtConfig) -> Result<(), KdlFmtError> {
    let input = read_stdin().map_err(KdlFmtError::ReadStdin)?;

    let (parsed, version) = parse_kdl(&input, args.kdl_version, !args.no_expression_strings)
        .map_err(|error| KdlFmtError::ParseKdl(None, error.into()))?;

    let cache = dashmap::DashMap::new();
    let actual_config =
        config.get_editorconfig_or_default(&std::path::PathBuf::from("dummy.kdl"), &cache);

    let formatted = format_kdl(parsed, &actual_config, version);

    print!("{formatted}");

    Ok(())
}

#[inline]
fn run_from_args(args: &FormatCommandArguments, config: &KdlFmtConfig) -> Result<(), KdlFmtError> {
    let mut paths = Vec::new();

    for path in &args.input {
        paths.push(std::path::PathBuf::from(path));
    }

    if paths.is_empty() {
        return Ok(());
    }

    let walker = setup_walker(paths);

    let overall_start_time = std::time::Instant::now();

    let file_count = AtomicUsize::new(0);
    let cache = dashmap::DashMap::new();

    walker
        .par_bridge()
        .map(|entry| {
            let file_start_time = std::time::Instant::now();

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

                let time_elapsed = file_start_time.elapsed();

                if formatted == input {
                    print_format_unchanged_file(file_path, time_elapsed);
                } else {
                    std::fs::write(file_path, &formatted).map_err(KdlFmtError::Io)?;
                    print_format_changed_file(file_path, time_elapsed);
                }

                file_count.fetch_add(1, Ordering::SeqCst);
            }
            Ok(())
        })
        .collect::<Result<Vec<()>, KdlFmtError>>()?;

    print_format_finished(file_count.into_inner(), overall_start_time.elapsed());

    Ok(())
}

#[inline]
pub fn run(args: &FormatCommandArguments, config: &KdlFmtConfig) -> Result<(), KdlFmtError> {
    if args.stdin || (args.input.len() == 1 && args.input.first().is_some_and(|v| v == "-")) {
        run_from_stdin(args, config)
    } else {
        run_from_args(args, config)
    }
}
