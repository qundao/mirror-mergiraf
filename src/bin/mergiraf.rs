use std::{env, fs, process::exit, process::Command};

use clap::{Parser, Subcommand};
use itertools::Itertools;
use log::warn;
use mergiraf::{
    attempts::AttemptsCache,
    bug_reporter::report_bug,
    line_merge_and_structured_resolution, resolve_merge_cascading,
    settings::{imitate_cr_lf_from_input, normalize_to_lf, DisplaySettings},
    supported_langs::supported_languages,
};

const DISABLING_ENV_VAR: &str = "MERGIRAF_DISABLE";

/// Syntax-aware merge driver for Git.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
#[command(propagate_version = true)]
struct CliArgs {
    /// Write debug files to a particular directory to analyze
    /// the internal aspects of the merge
    #[clap(short = 'd', long = "debug", global = true)]
    debug_dir: Option<String>,
    /// Verbosity
    #[clap(short = 'v', long = "verbose", global = true)]
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
        #[arg(short, long, default_value_t = false)]
        compact: bool,
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
        #[clap(short = 'c', long = "compact", default_value_t = false)]
        compact: bool,
        /// Keep file untouched and show the results of resolution on standard output instead
        #[clap(short = 'k', long = "keep")]
        keep: bool,
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
    match real_main(args) {
        Ok(exit_code) => exit(exit_code),
        Err(error) => {
            eprintln!("Mergiraf: {error}");
            exit(-1)
        }
    }
}

fn real_main(args: CliArgs) -> Result<i32, String> {
    stderrlog::new()
        .module(module_path!())
        .verbosity(if args.verbose { 3 } else { 2 })
        .init()
        .unwrap();

    let default_base_name = "base".to_owned();
    let default_left_name = "left".to_owned();
    let default_right_name = "right".to_owned();

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
        } => {
            let old_git_detected = base_name.as_deref().unwrap_or("") == "%S";

            let settings = DisplaySettings {
                diff3: true,
                compact,
                conflict_marker_size: 7,
                base_revision_name: base_name
                    .map(|name| {
                        if name == "%S" {
                            default_base_name
                        } else {
                            name
                        }
                    })
                    .unwrap_or(base.clone()),
                left_revision_name: left_name
                    .map(|name| {
                        if name == "%X" {
                            default_left_name
                        } else {
                            name
                        }
                    })
                    .unwrap_or(left.clone()),
                right_revision_name: right_name
                    .map(|name| {
                        if name == "%Y" {
                            default_right_name
                        } else {
                            name
                        }
                    })
                    .unwrap_or(right.clone()),
            };

            {
                let mergiraf_disabled = env::var("mergiraf").is_ok_and(|v| v == "0")
                    || env::var(DISABLING_ENV_VAR).is_ok_and(|v| !v.is_empty()); // TODO: deprecate

                if mergiraf_disabled {
                    return fallback_to_git_merge_file(base, left, right, git, &settings);
                }
            }

            let fname_base = &base;
            let contents_base = normalize_to_lf(&read_file_to_string(fname_base)?);
            let fname_left = &left;
            let original_contents_left = read_file_to_string(fname_left)?;
            let contents_left = normalize_to_lf(&original_contents_left);
            let fname_right = &right;
            let contents_right = normalize_to_lf(&read_file_to_string(fname_right)?);

            let attempts_cache = AttemptsCache::new(None, None).ok();

            let merge_result = line_merge_and_structured_resolution(
                &contents_base,
                &contents_left,
                &contents_right,
                &path_name.unwrap_or(fname_base.to_owned()),
                &settings,
                !fast,
                attempts_cache.as_ref(),
                &args.debug_dir,
            );
            if let Some(fname_out) = output {
                write_string_to_file(&fname_out, &merge_result.contents)?
            } else if git {
                write_string_to_file(fname_left, &merge_result.contents)?
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
            keep,
        } => {
            let settings = DisplaySettings {
                diff3: true,
                compact,
                conflict_marker_size: 7,
                base_revision_name: default_base_name, // TODO detect from file
                left_revision_name: default_left_name,
                right_revision_name: default_right_name,
            };

            let original_conflict_contents = read_file_to_string(&fname_conflicts)?;
            let conflict_contents = normalize_to_lf(&original_conflict_contents);
            let working_dir = env::current_dir().expect("Invalid current directory");

            let postprocessed = resolve_merge_cascading(
                &conflict_contents,
                &fname_conflicts.clone(),
                &settings,
                &args.debug_dir,
                &working_dir,
            );
            match postprocessed {
                Ok(merged) => {
                    if merged.method == "original" {
                        1
                    } else {
                        if keep {
                            print!(
                                "{}",
                                imitate_cr_lf_from_input(
                                    &original_conflict_contents,
                                    &merged.contents
                                )
                            );
                        } else {
                            write_string_to_file(
                                &(fname_conflicts.clone() + ".orig"),
                                &conflict_contents,
                            )?;
                            write_string_to_file(&fname_conflicts, &merged.contents)?;
                        };
                        0
                    }
                }
                Err(e) => {
                    warn!("Mergiraf: {}", e);
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
            for lang_profile in supported_languages() {
                if gitattributes {
                    for extension in lang_profile.extensions {
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
            report_bug(merge_id_or_file)?;
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
    base: String,
    left: String,
    right: String,
    git: bool,
    settings: &DisplaySettings,
) -> Result<i32, String> {
    let mut command = Command::new("git");
    command.arg("merge-file").arg("--diff-algorithm=histogram");
    if !git {
        command.arg("-p");
    }
    command
        .arg("-L")
        .arg(&settings.left_revision_name)
        .arg("-L")
        .arg(&settings.base_revision_name)
        .arg("-L")
        .arg(&settings.right_revision_name)
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

#[test]
fn verify_cli() {
    use clap::CommandFactory;
    CliArgs::command().debug_assert();
}
