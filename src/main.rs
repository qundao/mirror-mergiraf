use std::{
    borrow::Cow,
    env, fs, io,
    path::{Path, PathBuf},
    process::{Command, exit},
    sync::Arc,
    time::Duration,
};

use clap::{ArgAction, Args, Parser, Subcommand};
use log::warn;
use mergiraf::{
    DISABLING_ENV_VAR, PathBufExt,
    attempts::AttemptsCache,
    bug_reporter::report_bug,
    languages, line_merge_and_structured_resolution,
    newline::{imitate_cr_lf_from_input, normalize_to_lf},
    resolve_merge_cascading,
    settings::DisplaySettings,
    util::{read_file_to_string, write_string_to_file},
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
            merge_or_solve:
                MergeOrSolveArgs {
                    debug_dir,
                    compact,
                    conflict_marker_size,
                    language,
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

            let settings: DisplaySettings<'static> = DisplaySettings {
                compact,
                conflict_marker_size,
                base_revision_name: match base_name {
                    Some(name) if name == "%S" => None,
                    Some(name) => Some(Cow::Owned(name)),
                    None => Some(base.to_string_lossy()),
                },
                left_revision_name: match left_name {
                    Some(name) if name == "%X" => None,
                    Some(name) => Some(Cow::Owned(name)),
                    None => Some(left.to_string_lossy()),
                },
                right_revision_name: match right_name {
                    Some(name) if name == "%Y" => None,
                    Some(name) => Some(Cow::Owned(name)),
                    None => Some(right.to_string_lossy()),
                },
                ..Default::default()
            };

            {
                let mergiraf_disabled = env::var(DISABLING_ENV_VAR).as_deref() == Ok("0");

                if mergiraf_disabled {
                    return fallback_to_git_merge_file(base, left, right, git, &settings)
                        .map_err(|e| format!("error when calling git-merge-file: {e}"));
                }
            }

            if let Some(debug_dir) = debug_dir {
                fs::create_dir_all(debug_dir)
                    .map_err(|err| format!("could not create the debug directory: {err}"))?;
            }

            let fname_base = &*base;
            let original_contents_base = read_file_to_string(fname_base)?;
            let contents_base = normalize_to_lf(original_contents_base);
            let contents_base = Arc::new(contents_base);

            let fname_left = &left;
            let original_contents_left = read_file_to_string(fname_left)?;
            let contents_left = normalize_to_lf(&original_contents_left);
            let contents_left = contents_left.into_owned().leak();

            let fname_right = &right;
            let original_contents_right = read_file_to_string(fname_right)?;
            let contents_right = normalize_to_lf(original_contents_right);
            let contents_right = Arc::new(contents_right);

            let attempts_cache = AttemptsCache::new(None, None).ok();

            let fname_base = path_name.unwrap_or(fname_base);

            let merge_result = line_merge_and_structured_resolution(
                contents_base,
                contents_left,
                contents_right,
                fname_base,
                settings,
                !fast,
                attempts_cache.as_ref(),
                debug_dir,
                Duration::from_millis(timeout.unwrap_or(if fast { 5000 } else { 10000 })),
                language.as_deref(),
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
                    warn!(
                        "Using Git v2.44.0 or above is recommended to get meaningful revision names on conflict markers when using Mergiraf."
                    );
                }
                1
            } else {
                0
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
                },
            stdout,
            keep_backup,
        } => {
            if conflict_location_looks_like_jj_repo(&fname_conflicts) {
                return Err(
                    "\
                    You seem to be using Jujutsu instead of Git.\n\
                    Please use `jj resolve --tool mergiraf [file]`.\n\
                    \n\
                    Jujutsu has its own style of conflict markers, which Mergiraf doesn't understand. \
                    Jujutsu users shouldn't call `mergiraf solve` directly, because Jujutsu has \
                    a builtin configuration to resolve conflicts manually using `mergiraf merge`."
                    .into()
                );
            }

            let settings = DisplaySettings {
                compact,
                // NOTE: the names will be recognized in `resolve_merge_cascading` (if possible)
                base_revision_name: None,
                left_revision_name: None,
                right_revision_name: None,
                conflict_marker_size,
                ..Default::default()
            };

            if let Some(debug_dir) = &debug_dir {
                fs::create_dir_all(debug_dir)
                    .map_err(|err| format!("could not create the debug directory: {err}"))?;
            }

            let original_conflict_contents = read_file_to_string(&fname_conflicts)?;
            let conflict_contents = normalize_to_lf(&original_conflict_contents);
            let working_dir = env::current_dir().expect("Invalid current directory");

            let postprocessed = resolve_merge_cascading(
                &conflict_contents,
                &fname_conflicts,
                settings,
                debug_dir.as_deref(),
                &working_dir,
                language.as_deref(),
            );
            match postprocessed {
                Ok(merged) => {
                    if stdout {
                        print!(
                            "{}",
                            imitate_cr_lf_from_input(&original_conflict_contents, &merged.contents)
                        );
                    } else {
                        write_string_to_file(&fname_conflicts, &merged.contents)?;
                        if keep_backup {
                            write_string_to_file(
                                fname_conflicts.with_added_extension("orig"),
                                &conflict_contents,
                            )?;
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
            let res = languages(gitattributes);
            println!("{res}");
            0
        }
        CliCommand::Report { merge_id_or_file } => {
            report_bug(&merge_id_or_file)?;
            0
        }
    };
    Ok(return_code)
}

fn fallback_to_git_merge_file(
    base: &Path,
    left: &Path,
    right: &Path,
    git: bool,
    settings: &DisplaySettings,
) -> io::Result<i32> {
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

    let exit_code = command
        .arg("--marker-size")
        .arg(settings.conflict_marker_size_or_default().to_string())
        .args([left, base, right])
        .spawn()?
        .wait()?
        .code()
        .unwrap_or(0);

    Ok(exit_code)
}

/// Check if user is using Jujutsu instead of Git, which can lead to issues when running
/// `mergiraf solve`
fn conflict_location_looks_like_jj_repo(fname_conflicts: &Path) -> bool {
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

    fn create_file_for_solve(repo_path: &Path) -> PathBuf {
        let test_file_name = "test.txt";

        let test_file_abs_path = repo_path.join(test_file_name);
        fs::write(&test_file_abs_path, "<<<<<<< LEFT\n[1, 2, 3, 4]\n||||||| BASE\n[1, 2, 3]\n=======\n[0, 1, 2, 3]\n>>>>>>> RIGHT\n")
            .expect("failed to write test file to git repository");

        test_file_abs_path
    }

    fn create_files_for_merge(repo_path: &Path) -> (PathBuf, PathBuf, PathBuf, PathBuf) {
        let base_file_name = "base.txt";
        let left_file_name = "left.txt";
        let right_file_name = "right.txt";
        let output_file_name = "output.txt";

        let base_file_abs_path = repo_path.join(base_file_name);
        fs::write(&base_file_abs_path, "[1, 2, 3]\n")
            .expect("failed to write test base file to git repository");
        let left_file_abs_path = repo_path.join(left_file_name);
        fs::write(&left_file_abs_path, "[1, 2, 3, 4]\n")
            .expect("failed to write test left file to git repository");
        let right_file_abs_path = repo_path.join(right_file_name);
        fs::write(&right_file_abs_path, "[0, 1, 2, 3]\n")
            .expect("failed to write test right file to git repository");
        let output_file_abs_path = repo_path.join(output_file_name);

        (
            base_file_abs_path,
            left_file_abs_path,
            right_file_abs_path,
            output_file_abs_path,
        )
    }

    #[test]
    fn manual_language_selection_for_solve() {
        let repo_dir = tempfile::tempdir().expect("failed to create the temp dir");
        let repo_path = repo_dir.path();

        let test_file_abs_path = create_file_for_solve(repo_path);

        // first try without specifying a language
        let return_code = real_main(CliArgs::parse_from([
            "mergiraf",
            "solve",
            test_file_abs_path.to_str().unwrap(),
        ]))
        .expect("failed to execute `mergiraf solve`");
        assert_eq!(
            return_code, 1,
            "running `mergiraf solve` should fail because the language can't be detected"
        );

        // then try with a language specified on the CLI
        let return_code = real_main(CliArgs::parse_from([
            "mergiraf",
            "solve",
            "--language=json",
            test_file_abs_path.to_str().unwrap(),
        ]))
        .expect("failed to execute `mergiraf solve`");
        assert_eq!(
            return_code, 0,
            "`mergiraf solve` should execute successfully with a specified language"
        );

        let merge_result =
            fs::read_to_string(test_file_abs_path).expect("couldn't read the merge result");
        assert_eq!(merge_result, "[0, 1, 2, 3, 4]\n");
    }

    #[test]
    fn manual_language_selection_for_merge() {
        let repo_dir = tempfile::tempdir().expect("failed to create the temp dir");
        let repo_path = repo_dir.path();

        let (base_file_abs_path, left_file_abs_path, right_file_abs_path, output_file_abs_path) =
            create_files_for_merge(repo_path);

        // first try without specifying a language
        let return_code = real_main(CliArgs::parse_from([
            "mergiraf",
            "merge",
            base_file_abs_path.to_str().unwrap(),
            left_file_abs_path.to_str().unwrap(),
            right_file_abs_path.to_str().unwrap(),
            "--output",
            output_file_abs_path.to_str().unwrap(),
        ]))
        .expect("failed to execute `mergiraf merge`");
        assert_eq!(
            return_code, 1,
            "running `mergiraf merge` should fail because the language can't be detected"
        );

        // then try with a language specified on the CLI
        let return_code = real_main(CliArgs::parse_from([
            "mergiraf",
            "merge",
            "--language=json",
            base_file_abs_path.to_str().unwrap(),
            left_file_abs_path.to_str().unwrap(),
            right_file_abs_path.to_str().unwrap(),
            "--output",
            output_file_abs_path.to_str().unwrap(),
        ]))
        .expect("failed to execute `mergiraf merge`");
        assert_eq!(
            return_code, 0,
            "`mergiraf merge` should execute successfully with a specified language"
        );

        let merge_result =
            fs::read_to_string(output_file_abs_path).expect("couldn't read the merge result");
        assert_eq!(merge_result, "[0, 1, 2, 3, 4]\n");
    }

    #[test]
    fn debug_dir_is_created_for_solve() {
        let repo_dir = tempfile::tempdir().expect("failed to create the temp dir");
        let repo_path = repo_dir.path();

        let test_file_abs_path = create_file_for_solve(repo_path);

        let debug_dir = tempfile::tempdir().unwrap();
        let debug_dir_path = debug_dir.path().to_path_buf();
        // hopefully no one creates a tmp file with the same exact name directly after we've
        // deleted our one
        debug_dir.close().unwrap();

        _ = real_main(CliArgs::parse_from([
            "mergiraf",
            "solve",
            test_file_abs_path.to_str().unwrap(),
            "--debug",
            debug_dir_path.to_str().unwrap(),
        ]));

        assert!(fs::exists(debug_dir_path).unwrap());
    }

    #[test]
    fn debug_dir_is_created_for_merge() {
        let repo_dir = tempfile::tempdir().expect("failed to create the temp dir");
        let repo_path = repo_dir.path();

        let (base_file_abs_path, left_file_abs_path, right_file_abs_path, _) =
            create_files_for_merge(repo_path);

        let debug_dir = tempfile::tempdir().unwrap();
        let debug_dir_path = debug_dir.path().to_path_buf();
        // hopefully no one creates a tmp file with the same exact name directly after we've
        // deleted our one
        debug_dir.close().unwrap();

        _ = real_main(CliArgs::parse_from([
            "mergiraf",
            "merge",
            base_file_abs_path.to_str().unwrap(),
            left_file_abs_path.to_str().unwrap(),
            right_file_abs_path.to_str().unwrap(),
            "--debug",
            debug_dir_path.to_str().unwrap(),
        ]));

        assert!(fs::exists(debug_dir_path).unwrap());
    }
}
