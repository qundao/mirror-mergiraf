use std::fs;
use std::path::Path;
use std::process::Command;

use mergiraf::newline::normalize_to_lf;
use mergiraf::settings::DisplaySettings;
use mergiraf::{resolve_merge_cascading, DISABLING_ENV_VAR};
use rstest::rstest;

mod common;
use common::{detect_extension, run_git, write_file_from_rev};

/// End-to-end test for the "mergiraf solve" command
#[rstest]
#[case("merge")]
#[case("diff3")]
fn solve_command(#[case] conflict_style: &str) {
    let test_dir = Path::new("examples/java/working/demo");
    let extension = detect_extension(test_dir);

    // create temp directory
    let repo_dir = tempfile::tempdir().expect("failed to create the temp dir");
    let repo_dir = repo_dir.path();
    // init git repository
    run_git(&["init", "."], repo_dir);
    run_git(&["checkout", "-b", "first_branch"], repo_dir);
    let file_name = write_file_from_rev(repo_dir, test_dir, "Base", &extension);
    run_git(&["add", &file_name.to_string_lossy()], repo_dir);
    run_git(
        &[
            "-c",
            "user.email=author@example.com",
            "-c",
            "user.name=Author",
            "commit",
            "--no-gpg-sign",
            "-m",
            "initial_commit",
        ],
        repo_dir,
    );
    write_file_from_rev(repo_dir, test_dir, "Left", &extension);
    run_git(
        &[
            "-c",
            "user.email=author@example.com",
            "-c",
            "user.name=Author",
            "commit",
            "--no-gpg-sign",
            "-am",
            "second_commit",
        ],
        repo_dir,
    );
    run_git(&["checkout", "HEAD~"], repo_dir);
    run_git(&["checkout", "-b", "second_branch"], repo_dir);
    write_file_from_rev(repo_dir, test_dir, "Right", &extension);
    run_git(
        &[
            "-c",
            "user.email=author@example.com",
            "-c",
            "user.name=Author",
            "commit",
            "--no-gpg-sign",
            "-am",
            "third_commit",
        ],
        repo_dir,
    );

    // do a rebase
    let mut command = Command::new("git");
    command.current_dir(repo_dir);
    command.args([
        "-c",
        &format!("merge.conflictstyle={conflict_style}"),
        "rebase",
        "first_branch",
        "--no-gpg-sign",
    ]);
    // in case Git is configured to use Mergiraf
    command.env(DISABLING_ENV_VAR, "0");
    let output = command.output().expect("Failed to execute git command");
    assert!(!output.status.success(), "expected a rebase conflict");

    // call mergiraf to the rescue
    let conflicts_contents =
        fs::read_to_string(repo_dir.join(&file_name)).expect("could not read the conflicts");
    let conflicts_contents = normalize_to_lf(conflicts_contents);
    let merge_result = resolve_merge_cascading(
        &conflicts_contents,
        &file_name,
        DisplaySettings::default(),
        None,
        repo_dir,
    )
    .expect("solving the conflicts returned an error");

    let expected_result = fs::read_to_string(test_dir.join(format!("Expected.{extension}")))
        .expect("could not read the expected results");
    let expected_result = normalize_to_lf(expected_result);
    assert_eq!(merge_result.contents, expected_result);
}
