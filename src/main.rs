use std::{
    borrow::Cow,
    env, fs,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
    time::Duration,
};

use clap::{ArgAction, Args, Parser, Subcommand};
use log::{error, warn};
use mergiraf::{
    ENABLING_ENV_VAR, EXIT_MERGE_HAS_CONFLICTS, EXIT_SOLVE_FAILED, EXIT_SOLVE_HAS_CONFLICTS,
    EXIT_SUCCESS,
    attempts::AttemptsCache,
    bug_reporter::report_bug,
    languages, line_merge_and_structured_resolution, merge,
    newline::{imitate_newline_style, infer_newline_style, normalize_to_lf},
    settings::{ConflictRegexes, DisplaySettings},
    solve,
    utils::{buffer_is_binary, read_file, read_file_to_string, write_string_to_file},
};

/// Syntax-aware merge driver for Git.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
#[command(propagate_version = true)]
#[deny(missing_docs)]
struct CliArgs {
    /// Verbosity
    #[arg(short, long, global = true)]
    verbose: bool,
    #[command(subcommand)]
    command: CliCommand,
}

/// `mergiraf merge` and `mergiraf solve` share a lot of flags which other subcommands don't.
/// to avoid duplication between [`CliCommand::Merge`] and [`CliCommand::Solve`], we define all
/// those flags here, and `flatten` them in the above subcommands
#[deny(missing_docs)]
#[derive(Debug, Args)]
struct MergeOrSolveArgs {
    /// Write debug files to a particular directory to analyze
    /// the internal aspects of the merge
    #[arg(short, long = "debug", global = true)]
    debug_dir: Option<PathBuf>,
    /// Display compact conflicts, breaking down lines
    #[arg(short, long, default_missing_value = "true", num_args = 0..=1, require_equals = true)]
    compact: Option<bool>,
    /// Length of conflict markers
    #[arg(short = 'l', long)]
    // the choice of 'l' is inherited from Git's merge driver interface
    conflict_marker_size: Option<usize>,
    /// Override automatic language detection.
    #[arg(short = 'L', long)]
    language: Option<String>,
    /// Enable syntax-aware merging despite the presence of syntax errors
    #[arg(long, default_missing_value = "true", num_args = 0..=1, require_equals = true)]
    allow_parse_errors: Option<bool>,
}

#[derive(Subcommand, Debug)]
enum CliCommand {
    /// Do a three-way merge
    Merge {
        /// Path to the file containing the base revision
        base: PathBuf,
        /// Path to the file containing the left revision
        left: PathBuf,
        /// Path to the file containing the right revision
        right: PathBuf,
        /// Only attempt to merge the files by solving textual conflicts,
        /// without doing a full structured merge from the ground up.
        #[arg(long)]
        fast: bool,
        #[command(flatten)]
        merge_or_solve: MergeOrSolveArgs,
        /// Behave as a git merge driver: overwrite the left revision
        #[arg(short, long)]
        git: bool,
        /// The path to the file to write the merge result to
        #[arg(short, long, conflicts_with = "git")]
        output: Option<PathBuf>,
        /// Final path in which the merged result will be stored.
        /// It is used to detect the language of the files using the file extension.
        #[arg(short, long)]
        path_name: Option<PathBuf>,
        /// Name to use for the base revision in conflict markers
        #[arg(short = 's', long)]
        // the choice of 's' is inherited from Git's merge driver interface
        base_name: Option<String>,
        /// Name to use for the left revision in conflict markers
        #[arg(short = 'x', long)]
        // the choice of 'x' is inherited from Git's merge driver interface
        left_name: Option<String>,
        /// Name to use for the right revision in conflict markers
        #[arg(short = 'y', long)]
        // the choice of 'y' is inherited from Git's merge driver interface
        right_name: Option<String>,
        /// Maximum number of milliseconds to try doing the merging for, after which we fall back on git's own algorithm. Set to 0 to disable this limit.
        #[arg(short, long)]
        timeout: Option<u64>,
    },
    /// Solve the conflicts in a merged file
    Solve {
        /// Path to the file containing merge conflicts
        conflicts: PathBuf,
        #[command(flatten)]
        merge_or_solve: MergeOrSolveArgs,
        /// Keep file untouched and show the results of resolution on standard output instead
        // TODO(0.13.0): remove the alias
        #[arg(short = 'p', long, alias = "keep")]
        stdout: bool,
        /// Create a copy of the original file by adding the `.orig` suffix to it
        #[arg(
            long,
            default_missing_value = "true",
            default_value_t = true,
            num_args = 0..=1,
            require_equals = true,
            action = ArgAction::Set,
            conflicts_with = "stdout",
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

#[derive(thiserror::Error, Debug)]
pub enum CliError {
    #[error("`git merge-file` returned exit code {exit_code}")]
    GitMergeFile { exit_code: i32 },
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Other(String),
}

fn main() -> ExitCode {
    let args = CliArgs::parse();

    stderrlog::new()
        .module(module_path!())
        .verbosity(if args.verbose { 3 } else { 2 })
        .init()
        .unwrap();

    match real_main(args) {
        Ok(exit_code) => exit_code,
        // If we got an internal error from `git merge-file`, we translate that
        // to a conflicted merge, so that we don't abort the overall merge.
        // See https://codeberg.org/mergiraf/mergiraf/issues/812.
        Err(CliError::GitMergeFile { .. }) => ExitCode::from(EXIT_MERGE_HAS_CONFLICTS),
        Err(error) => {
            eprintln!("Mergiraf: {error}");
            ExitCode::from(255)
        }
    }
}

fn real_main(args: CliArgs) -> Result<ExitCode, CliError> {
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
            merge_or_solve:
                MergeOrSolveArgs {
                    debug_dir,
                    compact,
                    conflict_marker_size,
                    language,
                    allow_parse_errors,
                },
            timeout,
        } => {
            let old_git_detected = base_name.as_deref().is_some_and(|n| n == "%S");

            let base = base.leak();
            let left = left.leak();
            let right = right.leak();

            // NOTE: reborrow to turn `&mut Path` returned by `PathBuf::leak` into `&Path`
            let path_name = path_name.map(|s| &*s.leak());
            let debug_dir = debug_dir.map(|s| &*s.leak());

            let settings: DisplaySettings<'static> = DisplaySettings::new(
                compact,
                conflict_marker_size,
                match base_name {
                    Some(name) if name == "%S" => None,
                    Some(name) => Some(Cow::Owned(name)),
                    None => Some(base.to_string_lossy()),
                },
                match left_name {
                    Some(name) if name == "%X" => None,
                    Some(name) => Some(Cow::Owned(name)),
                    None => Some(left.to_string_lossy()),
                },
                match right_name {
                    Some(name) if name == "%Y" => None,
                    Some(name) => Some(Cow::Owned(name)),
                    None => Some(right.to_string_lossy()),
                },
            );

            {
                let mergiraf_disabled = env::var(ENABLING_ENV_VAR).as_deref() == Ok("0");

                if mergiraf_disabled {
                    return fallback_to_git_merge_file(base, left, right, git, &output, &settings);
                }
            }

            if let Some(debug_dir) = debug_dir {
                fs::create_dir_all(debug_dir).map_err(|err| {
                    CliError::Other(format!("could not create the debug directory: {err}"))
                })?;
            }

            let fname_base = &*base;
            let fname_left = &*left;
            let fname_right = &*right;

            let original_contents_base = read_file(fname_base).map_err(CliError::Other)?;
            let original_contents_left = read_file(fname_left).map_err(CliError::Other)?;
            let original_contents_right = read_file(fname_right).map_err(CliError::Other)?;

            if [
                &*original_contents_base,
                &*original_contents_left,
                &*original_contents_base,
            ]
            .into_iter()
            .any(buffer_is_binary)
            {
                // Don't `return Err` here, as that would make `main` exit with 255, which would
                // abort the overall merge. See https://codeberg.org/mergiraf/mergiraf/pulls/81
                error!("cannot merge binary files");
                return Ok(ExitCode::from(EXIT_MERGE_HAS_CONFLICTS));
            }

            let (
                Ok(original_contents_base),
                Ok(original_contents_left),
                Ok(original_contents_right),
            ) = (
                String::from_utf8(original_contents_base),
                String::from_utf8(original_contents_left),
                String::from_utf8(original_contents_right),
            )
            else {
                // if you change this warning message, update `misc::test_git_merge_file_fallback_on_files`
                warn!("input files are not UTF-8, falling back to Git");
                return fallback_to_git_merge_file(base, left, right, git, &output, &settings);
            };

            {
                let ConflictRegexes {
                    diff2: re_diff2,
                    diff3: re_diff3,
                    diff3_no_newline: re_diff3_no_newline,
                    ..
                } = settings.conflict_regexes();
                for (side, contents) in [
                    ("base", &original_contents_base),
                    ("left", &original_contents_left),
                    ("right", &original_contents_right),
                ] {
                    if re_diff3.is_match(contents)
                        || re_diff2.is_match(contents)
                        || re_diff3_no_newline.is_match(contents)
                    {
                        warn!("{side} side contains conflict markers, falling back to Git");
                        return fallback_to_git_merge_file(
                            base, left, right, git, &output, &settings,
                        );
                    }
                }
            }

            let original_newline_style = infer_newline_style(&original_contents_left);

            let contents_base = normalize_to_lf(original_contents_base);
            let contents_left = normalize_to_lf(original_contents_left);
            let contents_right = normalize_to_lf(original_contents_right);

            let attempts_cache = AttemptsCache::new(None, None).ok();

            let fname_base = path_name.unwrap_or(fname_base);

            let working_dir = env::current_dir().expect("Invalid current directory");

            let mut merge_result = line_merge_and_structured_resolution(
                contents_base,
                contents_left,
                contents_right,
                fname_base,
                settings,
                !fast,
                attempts_cache.as_ref(),
                merge::CliOpts {
                    allow_parse_errors,
                    language: language.as_deref(),
                },
                Some(&working_dir),
                debug_dir,
                Duration::from_millis(timeout.unwrap_or(if fast { 5000 } else { 10000 })),
            );
            merge_result.contents =
                imitate_newline_style(&merge_result.contents, original_newline_style);
            if let Some(fname_out) = output {
                write_string_to_file(&fname_out, &merge_result.contents)
                    .map_err(CliError::Other)?;
            } else if git {
                write_string_to_file(fname_left, &merge_result.contents)
                    .map_err(CliError::Other)?;
            } else {
                print!("{}", merge_result.contents);
            }

            if merge_result.conflict_count > 0 {
                if old_git_detected {
                    warn!(
                        "Using Git v2.44.0 or above is recommended to get meaningful revision names on conflict markers when using Mergiraf."
                    );
                }
                EXIT_MERGE_HAS_CONFLICTS
            } else {
                EXIT_SUCCESS
            }
        }
        CliCommand::Solve {
            conflicts: fname_conflicts,
            merge_or_solve:
                MergeOrSolveArgs {
                    debug_dir,
                    compact,
                    conflict_marker_size,
                    language,
                    allow_parse_errors,
                },
            stdout,
            keep_backup,
        } => {
            if let Some(debug_dir) = &debug_dir {
                fs::create_dir_all(debug_dir).map_err(|err| {
                    CliError::Other(format!("could not create the debug directory: {err}"))
                })?;
            }

            // Unlike `mergiraf merge`, there is no `git merge-file` we can fall back on in case of
            // non-UTF-8 input, so just bail out.
            let original_conflict_contents =
                read_file_to_string(&fname_conflicts).map_err(CliError::Other)?;

            if file_seems_to_have_a_jj_conflict(&fname_conflicts, &original_conflict_contents) {
                // Our current logger doesn't handle multiline messages well, so we split them manually.
                // Ideally, the output of this would be something like:
                // ```
                // error: you seem to be using Jujutsu instead of Git
                //  help: please use `jj resolve --tool mergiraf [file]`
                //  note: Jujutsu has its own style of conflict markers, which Mergiraf doesn't understand
                //  note: Jujutsu users shouldn't call `mergiraf solve` directly, because Jujutsu has
                //        a builtin configuration to resolve conflicts manually using `mergiraf merge`
                // ```
                warn!(
                    "You seem to be using Jujutsu instead of Git. Please use `jj resolve --tool mergiraf [file]`."
                );
                warn!(
                    "Jujutsu has its own style of conflict markers, which Mergiraf doesn't understand."
                );
                warn!(
                    "Jujutsu users shouldn't call `mergiraf solve` directly, because Jujutsu has \
                    a builtin configuration to resolve conflicts manually using `mergiraf merge`."
                );
            }

            let working_dir = env::current_dir().expect("Invalid current directory");
            let postprocessed = solve::solve(
                &fname_conflicts,
                &original_conflict_contents,
                solve::CliOpts {
                    allow_parse_errors,
                    compact,
                    conflict_marker_size,
                    language: language.as_deref(),
                },
                &working_dir,
                debug_dir.as_deref(),
            );
            match postprocessed {
                Ok(merged) => {
                    if stdout {
                        print!("{}", merged.contents);
                    } else {
                        write_string_to_file(&fname_conflicts, &merged.contents)
                            .map_err(CliError::Other)?;
                        if keep_backup {
                            write_string_to_file(
                                fname_conflicts.with_added_extension("orig"),
                                &original_conflict_contents,
                            )
                            .map_err(CliError::Other)?;
                        }
                    };
                    if merged.conflict_count > 0 {
                        EXIT_SOLVE_HAS_CONFLICTS
                    } else {
                        EXIT_SUCCESS
                    }
                }
                Err(e) => {
                    warn!("Mergiraf: {e}");
                    EXIT_SOLVE_FAILED
                }
            }
        }
        CliCommand::Review { merge_id } => {
            let attempts_cache = AttemptsCache::new(None, None).map_err(CliError::Other)?;
            attempts_cache
                .review_merge(&merge_id)
                .map_err(CliError::Other)?;
            EXIT_SUCCESS
        }
        CliCommand::Languages { gitattributes } => {
            let res = languages(gitattributes);
            println!("{res}");
            EXIT_SUCCESS
        }
        CliCommand::Report { merge_id_or_file } => {
            report_bug(&merge_id_or_file).map_err(CliError::Other)?;
            EXIT_SUCCESS
        }
    };
    Ok(ExitCode::from(return_code))
}

fn fallback_to_git_merge_file(
    base: &Path,
    left: &Path,
    right: &Path,
    git: bool,
    output: &Option<PathBuf>,
    settings: &DisplaySettings,
) -> Result<ExitCode, CliError> {
    let mut command = Command::new("git");
    command.arg("merge-file").arg("--diff-algorithm=histogram");
    if !git {
        command.arg("-p");
    }
    if let Some(left_rev_name) = settings.left_revision_name.as_deref() {
        command.args(["-L", left_rev_name]);

        if let Some(base_rev_name) = settings.base_revision_name.as_deref() {
            command.args(["-L", base_rev_name]);

            if let Some(right_rev_name) = settings.right_revision_name.as_deref() {
                command.args(["-L", right_rev_name]);
            }
        }
    }

    let command = command
        .arg("--marker-size")
        .arg(settings.conflict_marker_size_or_default().to_string())
        .args([left, base, right]);

    let code = if let Some(output_path) = output {
        let command_output = command.output()?;
        fs::write(output_path, &command_output.stdout)?;
        command_output.status
    } else {
        command.spawn()?.wait()?
    };
    let code = code.code().unwrap_or(0);
    if code >= 128 {
        Err(CliError::GitMergeFile { exit_code: code })
    } else {
        // we cannot return the exact same exit code as Git returned to us,
        // because Rust exposes that to us as an i32 and we need to return an
        // ExitCode (u8), so we map all other errors to 1 (signalling a conflict state)
        Ok(ExitCode::from(if code == 0 {
            EXIT_SUCCESS
        } else {
            EXIT_MERGE_HAS_CONFLICTS
        }))
    }
}

/// Check if user is using Jujutsu instead of Git, which can lead to issues when running
/// `mergiraf solve`
fn file_seems_to_have_a_jj_conflict(fname_conflicts: &Path, contents: &str) -> bool {
    /// A marker that is used in the default jj-style conflicts, but not in git-style ones.
    const JJ_CONFLICT_MARKER: &str = "%%%%%%% ";

    // First, check if we are in a jj repo -- if not, then the conflict is very unlikely to come
    // from jj.
    if let Ok(conflict_path) = fname_conflicts.canonicalize()
        && let Some(conflict_dir) = conflict_path.parent()
        && let Ok(output) = Command::new("jj")
            .arg("root")
            .current_dir(conflict_dir)
            .output()
        && output.status.success()
        // output of `jj root` contains a trailing newline
        && let stdout = output.stdout.trim_ascii_end()
        && let Ok(repo_path) = str::from_utf8(stdout)
        // There's a JSON stream editor also called `jj`, which, when called with `jj root`,
        // actually returns an empty stdout (even though when running interactively, it seems to
        // just hang). And out latter check for `fs::exists` actually doesn't recognize that,
        // because "empty path" + "/.jj" gives a relative path ".jj", which just happens to be
        // valid (if the repos are colocated). So we sanity-check that the output is not empty.
        //
        // One could imagine a program that returns _something_ on `jj root`, even an
        // "unknown subcommand: root", but the hope is that the path created by joining "/.jj" onto
        // that will end up being invalid, which `fs::exists` will catch
        && !repo_path.is_empty()
        && let jj_root = Path::new(repo_path).join(".jj")
        && let Ok(true) = fs::exists(jj_root)
        // Next, see if the file _actually_ has a jj-style conflict. This is to cater to a case where
        // the user merges using git and tries to resolve the resulting git-style conflicts using
        // `mergiraf solve` -- see https://codeberg.org/mergiraf/mergiraf/issues/797.
        && contents.lines().any(|l| l.starts_with(JJ_CONFLICT_MARKER))
    {
        true
    } else {
        false
    }
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
    fn compact_flag() {
        // works on `merge`:

        // `true` when passed without value
        // (and doesn't try to parse `foo.c` as value because of `require_equals`)
        let CliCommand::Merge {
            merge_or_solve: MergeOrSolveArgs { compact, .. },
            ..
        } = CliArgs::parse_from(["mergiraf", "merge", "--compact", "foo.c", "bar.c", "baz.c"])
            .command
        else {
            unreachable!("`mergiraf merge` should invoke the `Merge` subcommand")
        };
        assert_eq!(compact, Some(true));

        // works on `solve`:

        // `true` when passed without value
        // (and doesn't try to parse `foo.c` as value because of `require_equals`)
        let CliCommand::Solve {
            merge_or_solve: MergeOrSolveArgs { compact, .. },
            ..
        } = CliArgs::parse_from(["mergiraf", "solve", "--compact", "foo.c"]).command
        else {
            unreachable!("`mergiraf solve` should invoke the `Solve` subcommand")
        };
        assert_eq!(compact, Some(true));
    }

    #[test]
    fn allow_parse_errors_flag() {
        // works on `merge`:

        // `true` when passed without value
        // (and doesn't try to parse `foo.c` as value because of `require_equals`)
        let CliCommand::Merge {
            merge_or_solve: MergeOrSolveArgs {
                allow_parse_errors, ..
            },
            ..
        } = CliArgs::parse_from([
            "mergiraf",
            "merge",
            "--allow-parse-errors",
            "foo.c",
            "bar.c",
            "baz.c",
        ])
        .command
        else {
            unreachable!("`mergiraf merge` should invoke the `Merge` subcommand")
        };
        assert_eq!(allow_parse_errors, Some(true));

        // works on `solve`:

        // `true` when passed without value
        // (and doesn't try to parse `foo.c` as value because of `require_equals`)
        let CliCommand::Solve {
            merge_or_solve: MergeOrSolveArgs {
                allow_parse_errors, ..
            },
            ..
        } = CliArgs::parse_from(["mergiraf", "solve", "--allow-parse-errors", "foo.c"]).command
        else {
            unreachable!("`mergiraf solve` should invoke the `Solve` subcommand")
        };
        assert_eq!(allow_parse_errors, Some(true));
    }

    #[test]
    fn keep_backup_flag() {
        // `true` when nothing passed
        let CliCommand::Solve { keep_backup, .. } =
            CliArgs::parse_from(["mergiraf", "solve", "foo.c"]).command
        else {
            unreachable!("`mergiraf solve` should invoke the `Solve` subcommand")
        };
        assert!(keep_backup);

        // `true` when passed without value
        // (and doesn't try to parse `foo.c` as value because of `require_equals`)
        let CliCommand::Solve { keep_backup, .. } =
            CliArgs::parse_from(["mergiraf", "solve", "--keep-backup", "foo.c"]).command
        else {
            unreachable!("`mergiraf solve` should invoke the `Solve` subcommand")
        };
        assert!(keep_backup);

        // `true` when passed with explicit `=true`
        let CliCommand::Solve { keep_backup, .. } =
            CliArgs::parse_from(["mergiraf", "solve", "--keep-backup=true", "foo.c"]).command
        else {
            unreachable!("`mergiraf solve` should invoke the `Solve` subcommand")
        };
        assert!(keep_backup);

        // `false` when passed with explicit `=false`
        let CliCommand::Solve { keep_backup, .. } =
            CliArgs::parse_from(["mergiraf", "solve", "--keep-backup=false", "foo.c"]).command
        else {
            unreachable!("`mergiraf solve` should invoke the `Solve` subcommand")
        };
        assert!(!keep_backup);
    }
}
