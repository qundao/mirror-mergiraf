use std::{
    env, fs,
    path::PathBuf,
    process::{exit, Command},
};

use clap::{ArgAction, Parser, Subcommand};
use itertools::Itertools;
use log::warn;
use mergiraf::{
    attempts::AttemptsCache,
    bug_reporter::report_bug,
    line_merge_and_structured_resolution,
    // XXX: move the uses to lib to avoid making these public?
    newline::{imitate_cr_lf_from_input, normalize_to_lf},
    resolve_merge_cascading,
    settings::DisplaySettings,
    supported_langs::SUPPORTED_LANGUAGES,
    DISABLING_ENV_VAR,
    DISABLING_ENV_VAR_LEGACY,
};

/// Syntax-aware merge driver for Git.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
#[command(propagate_version = true)]
struct CliArgs {
    /// Write debug files to a particular directory to analyze
    /// the internal aspects of the merge
    #[clap(short, long = "debug", global = true)]
    debug_dir: Option<PathBuf>,
    /// Verbosity
    #[clap(short, long, global = true)]
    verbose: bool,
    #[command(subcommand)]
    command: CliCommand,
}

#[derive(Subcommand, Debug)]
enum CliCommand {
    /// Do a three-way merge
    Merge {
        /// The path to the file containing the base revision
        base: String,
        /// The path to the file containing the left revision
        left: String,
        /// The path to the file containing the right revision
        right: String,
        /// Only attempt to merge the files by solving textual conflicts,
        /// without doing a full structured merge from the ground up.
        #[clap(long)]
        fast: bool,
        /// Display compact conflicts, breaking down lines
        #[arg(short, long)]
        compact: Option<bool>,
        #[arg(short = 'l', long)]
        // the choice of 'l' is inherited from Git's merge driver interface
        conflict_marker_size: Option<usize>,
        /// Behave as a git merge driver: overwrite the left revision
        #[clap(short, long)]
        git: bool,
        /// The path to the file to write the merge result to
        #[clap(short, long, conflicts_with = "git")]
        output: Option<String>,
        /// Final path in which the merged result will be stored.
        /// It is used to detect the language of the files using the file extension.
        #[clap(short, long)]
        path_name: Option<String>,
        /// Name to use for the base revision in conflict markers
        #[clap(short = 's', long)]
        // the choice of 's' is inherited from Git's merge driver interface
        base_name: Option<String>,
        /// Name to use for the left revision in conflict markers
        #[clap(short = 'x', long)]
        // the choice of 'x' is inherited from Git's merge driver interface
        left_name: Option<String>,
        /// Name to use for the right revision in conflict markers
        #[clap(short = 'y', long)]
        // the choice of 'y' is inherited from Git's merge driver interface
        right_name: Option<String>,
    },
    /// Solve the conflicts in a merged file
    Solve {
        /// Path to a file containing merge conflicts
        conflicts: String,
        /// Display compact conflicts, breaking down lines
        #[clap(short, long)]
        compact: Option<bool>,
        #[arg(short = 'l', long)]
        // the choice of 'l' is inherited from Git's merge driver interface
        conflict_marker_size: Option<usize>,
        /// Keep file untouched and show the results of resolution on standard output instead
        #[clap(short, long)]
        keep: bool,
        /// Create a copy of the original file by adding the `.orig` suffix to it (`true` by default)
        #[clap(
            long,
            default_missing_value = "true",
            default_value_t = true,
            num_args = 0..=1,
            require_equals = true,
            action = ArgAction::Set,
        )]
        keep_backup: bool,
    },
    /// Review the resolution of a merge by showing the differences with a line-based merge
    Review {
        /// Identifier of the merge case
        merge_id: String,
    },
    /// Create a bug report for a bad merge
    Report {
        /// Identifier of the merge case (if it did not return conflicts) or path to file with merge conflicts
        merge_id_or_file: String,
    },
    /// Show the supported languages
    Languages {
        /// Print the list in a format suitable for inclusion in gitattributes
        #[arg(long, default_value_t = false)]
        gitattributes: bool,
    },
}

fn main() {
    let args = CliArgs::parse();

    stderrlog::new()
        .module(module_path!())
        .verbosity(if args.verbose { 3 } else { 2 })
        .init()
        .unwrap();

    match real_main(args) {
        Ok(exit_code) => exit(exit_code),
        Err(error) => {
            eprintln!("Mergiraf: {error}");
            exit(-1)
        }
    }
}

fn real_main(args: CliArgs) -> Result<i32, String> {
    let return_code = match args.command {
        CliCommand::Merge {
            base,
            left,
            right,
            fast,
            path_name,
            git,
            output,
            base_name,
            left_name,
            right_name,
            compact,
            conflict_marker_size,
        } => {
            let old_git_detected = base_name.as_deref().is_some_and(|n| n == "%S");

            let settings = DisplaySettings {
                compact,
                conflict_marker_size,
                base_revision_name: match base_name.as_deref() {
                    Some("%S") => None,
                    Some(name) => Some(name),
                    None => Some(&base),
                },
                left_revision_name: match left_name.as_deref() {
                    Some("%X") => None,
                    Some(name) => Some(name),
                    None => Some(&left),
                },
                right_revision_name: match right_name.as_deref() {
                    Some("%Y") => None,
                    Some(name) => Some(name),
                    None => Some(&right),
                },
                ..Default::default()
            };

            {
                let mergiraf_disabled = env::var(DISABLING_ENV_VAR).is_ok_and(|v| v == "0")
                    || env::var(DISABLING_ENV_VAR_LEGACY).is_ok_and(|v| !v.is_empty());

                if mergiraf_disabled {
                    return fallback_to_git_merge_file(&base, &left, &right, git, &settings);
                }
            }

            let fname_base = &base;
            let original_contents_base = read_file_to_string(fname_base)?;
            let contents_base = normalize_to_lf(original_contents_base);

            let fname_left = &left;
            let original_contents_left = read_file_to_string(fname_left)?;
            let contents_left = normalize_to_lf(&original_contents_left);

            let fname_right = &right;
            let original_contents_right = read_file_to_string(fname_right)?;
            let contents_right = normalize_to_lf(original_contents_right);

            let attempts_cache = AttemptsCache::new(None, None).ok();

            let fname_base = path_name.as_deref().unwrap_or(fname_base);

            let merge_result = line_merge_and_structured_resolution(
                &contents_base,
                &contents_left,
                &contents_right,
                fname_base,
                &settings,
                !fast,
                attempts_cache.as_ref(),
                args.debug_dir.as_deref(),
            );
            if let Some(fname_out) = output {
                write_string_to_file(&fname_out, &merge_result.contents)?;
            } else if git {
                write_string_to_file(fname_left, &merge_result.contents)?;
            } else {
                print!(
                    "{}",
                    imitate_cr_lf_from_input(&original_contents_left, &merge_result.contents)
                );
            }

            if merge_result.conflict_count > 0 {
                if old_git_detected {
                    warn!("Using Git v2.44.0 or above is recommended to get meaningful revision names on conflict markers when using Mergiraf.");
                }
                1
            } else {
                0
            }
        }
        CliCommand::Solve {
            conflicts: fname_conflicts,
            compact,
            conflict_marker_size,
            keep,
            keep_backup,
        } => {
            let settings = DisplaySettings {
                compact,
                // NOTE: the names will be recognized in `resolve_merge_cascading` (if possible)
                base_revision_name: None,
                left_revision_name: None,
                right_revision_name: None,
                conflict_marker_size,
                ..Default::default()
            };

            let original_conflict_contents = read_file_to_string(&fname_conflicts)?;
            let conflict_contents = normalize_to_lf(&original_conflict_contents);
            let working_dir = env::current_dir().expect("Invalid current directory");

            let postprocessed = resolve_merge_cascading(
                &conflict_contents,
                &fname_conflicts,
                settings,
                args.debug_dir.as_deref(),
                &working_dir,
            );
            match postprocessed {
                Ok(merged) => {
                    if keep {
                        print!(
                            "{}",
                            imitate_cr_lf_from_input(&original_conflict_contents, &merged.contents)
                        );
                    } else {
                        write_string_to_file(&fname_conflicts, &merged.contents)?;
                        if keep_backup {
                            write_string_to_file(&(fname_conflicts + ".orig"), &conflict_contents)?;
                        }
                    };
                    0
                }
                Err(e) => {
                    warn!("Mergiraf: {e}");
                    1
                }
            }
        }
        CliCommand::Review { merge_id } => {
            let attempts_cache = AttemptsCache::new(None, None)?;
            attempts_cache.review_merge(&merge_id)?;
            0
        }
        CliCommand::Languages { gitattributes } => {
            for lang_profile in &*SUPPORTED_LANGUAGES {
                if gitattributes {
                    for extension in &lang_profile.extensions {
                        println!("*{extension} merge=mergiraf");
                    }
                } else {
                    println!(
                        "{} ({})",
                        lang_profile.name,
                        lang_profile
                            .extensions
                            .iter()
                            .map(|ext| format!("*{ext}"))
                            .join(", ")
                    );
                }
            }
            0
        }
        CliCommand::Report { merge_id_or_file } => {
            report_bug(&merge_id_or_file)?;
            0
        }
    };
    Ok(return_code)
}

fn read_file_to_string(path: &str) -> Result<String, String> {
    fs::read_to_string(path).map_err(|err| format!("Could not read {path}: {err}"))
}

fn write_string_to_file(path: &str, contents: &str) -> Result<(), String> {
    fs::write(path, contents).map_err(|err| format!("Could not write {path}: {err}"))
}

fn fallback_to_git_merge_file(
    base: &str,
    left: &str,
    right: &str,
    git: bool,
    settings: &DisplaySettings,
) -> Result<i32, String> {
    let mut command = Command::new("git");
    command.arg("merge-file").arg("--diff-algorithm=histogram");
    if !git {
        command.arg("-p");
    }
    if let (Some(base_rev_name), Some(left_rev_name), Some(right_rev_name)) = (
        settings.base_revision_name,
        settings.left_revision_name,
        settings.right_revision_name,
    ) {
        command
            .arg("-L")
            .arg(left_rev_name)
            .arg("-L")
            .arg(base_rev_name)
            .arg("-L")
            .arg(right_rev_name);
    };

    command
        .arg("--marker-size")
        .arg(settings.conflict_marker_size_or_default().to_string())
        .arg(left)
        .arg(base)
        .arg(right)
        .spawn()
        .and_then(|mut process| {
            process
                .wait()
                .map(|exit_status| exit_status.code().unwrap_or(0))
        })
        .map_err(|err| err.to_string())
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn verify_cli() {
        use clap::CommandFactory;
        CliArgs::command().debug_assert();
    }

    #[test]
    fn keep_backup_flag() {
        // `true` when nothing passed
        let CliCommand::Solve { keep_backup, .. } =
            CliArgs::parse_from(["mergiraf", "solve", "foo.c"]).command
        else {
            unreachable!("`mergiraf solve` should invoke the `Solve` submcommand")
        };
        assert!(keep_backup);

        // `true` when passed without value
        // (and doesn't try to parse `foo.c` as value because of `require_equals`)
        let CliCommand::Solve { keep_backup, .. } =
            CliArgs::parse_from(["mergiraf", "solve", "--keep-backup", "foo.c"]).command
        else {
            unreachable!("`mergiraf solve` should invoke the `Solve` submcommand")
        };
        assert!(keep_backup);

        // `true` when passed with explicit `=true`
        let CliCommand::Solve { keep_backup, .. } =
            CliArgs::parse_from(["mergiraf", "solve", "--keep-backup=true", "foo.c"]).command
        else {
            unreachable!("`mergiraf solve` should invoke the `Solve` submcommand")
        };
        assert!(keep_backup);

        // `false` when passed with explicit `=false`
        let CliCommand::Solve { keep_backup, .. } =
            CliArgs::parse_from(["mergiraf", "solve", "--keep-backup=false", "foo.c"]).command
        else {
            unreachable!("`mergiraf solve` should invoke the `Solve` submcommand")
        };
        assert!(!keep_backup);
    }

    #[test]
    fn keep_backup_keeps_backup() {
        let repo_dir = tempfile::tempdir().expect("failed to create the temp dir");
        let repo_path = repo_dir.path();

        let test_file_name = "test.c";

        let test_file_abs_path = repo_path.join(test_file_name);
        fs::write(&test_file_abs_path, "hello\nworld\n")
            .expect("failed to write test file to git repository");

        let test_file_orig_file_path = repo_path.join(format!("{test_file_name}.orig"));

        // `solve` without keeping backup
        real_main(CliArgs::parse_from([
            "mergiraf",
            "solve",
            "--keep-backup=false",
            test_file_abs_path.to_str().unwrap(),
        ]))
        .expect("failed to execute `mergiraf solve`");

        assert!(!test_file_orig_file_path.exists());

        // `solve` once again but with backup this time
        real_main(CliArgs::parse_from([
            "mergiraf",
            "solve",
            "--keep-backup=true",
            test_file_abs_path.to_str().unwrap(),
        ]))
        .expect("failed to execute `mergiraf solve`");

        assert!(test_file_orig_file_path.exists());
    }
}
