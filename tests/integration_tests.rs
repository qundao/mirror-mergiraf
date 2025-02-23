use core::str;
use std::fs::{self, read_dir};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use diffy_imara::{create_patch, PatchFormatter};
use mergiraf::line_based::line_based_merge;
use mergiraf::newline::normalize_to_lf;
use mergiraf::settings::DisplaySettings;
use mergiraf::{
    line_merge_and_structured_resolution, resolve_merge_cascading, PathBufExt, DISABLING_ENV_VAR,
};
use rstest::rstest;

fn run_git(args: &[&str], repo_dir: &Path) {
    let command_str = format!("git {}", args.join(" "));
    let mut command = Command::new("git");
    command.current_dir(repo_dir);
    command.args(args);
    command.env_remove("HOME"); // disable ~/.gitconfig to isolate the test better
    let output = command.output().expect("Failed to execute git command");
    if !output.status.success() {
        eprintln!("{}", str::from_utf8(&output.stderr).unwrap());
        panic!("git command failed: {command_str}");
    }
}

fn write_file_from_rev(
    repo_dir: &Path,
    test_dir: &Path,
    revision: &str,
    extension: &str,
) -> PathBuf {
    let file_name = format!("file.{extension}");
    let fname_base = test_dir.join(format!("{revision}.{extension}"));
    let contents = fs::read_to_string(&fname_base).expect("Unable to read left file");
    fs::write(repo_dir.join(&file_name), contents)
        .expect("failed to write test file to git repository");
    PathBuf::from(file_name)
}

fn detect_extension(test_dir: &Path) -> String {
    read_dir(test_dir)
        .expect("Could not list files in test directory")
        .find_map(|filename| {
            filename
                .unwrap()
                .file_name()
                .into_string()
                .expect("Unable to read filename in test directory")
                .strip_prefix("Base.")
                .map(|s| s.to_owned())
        })
        .expect("Could not find a Base.* file in the test directory")
}

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
    command.args(
        [
            "-c",
            &format!("merge.conflictstyle={conflict_style}"),
            "rebase",
            "first_branch",
            "--no-gpg-sign",
        ]
        .into_iter(),
    );
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

#[rstest]
fn timeout_support() {
    let test_dir = Path::new("examples/java/working/move_and_modify_conflict");
    let ext = "java";
    #[expect(unstable_name_collisions)]
    let fname_base = test_dir.join(format!("Base.{ext}")).leak();
    let contents_base = fs::read_to_string(&fname_base)
        .expect("Unable to read left file")
        .leak();
    let fname_left = test_dir.join(format!("Left.{ext}"));
    let contents_left = fs::read_to_string(fname_left)
        .expect("Unable to read left file")
        .leak();
    let fname_right = test_dir.join(format!("Right.{ext}"));
    let contents_right = fs::read_to_string(fname_right)
        .expect("Unable to read right file")
        .leak();
    let settings = DisplaySettings::default();
    let contents_expected =
        line_based_merge(contents_base, contents_left, contents_right, &settings).contents;

    let merge_result = line_merge_and_structured_resolution(
        contents_base,
        contents_left,
        contents_right,
        fname_base,
        settings,
        true,
        None,
        None,
        Duration::from_millis(1), // very small timeout: structured merging should never be that fast
    );

    let expected = contents_expected.trim();
    let actual = merge_result.contents.trim();
    if expected != actual {
        let patch = create_patch(expected, actual);
        let f = PatchFormatter::new().with_color();
        print!("{}", f.fmt_patch(&patch));
        eprintln!("test failed: outputs differ for {}", test_dir.display());
        panic!();
    }
}

fn run_test_from_dir(test_dir: &Path) {
    let ext = detect_extension(test_dir);
    #[expect(unstable_name_collisions)]
    let fname_base = test_dir.join(format!("Base.{ext}")).leak();
    let contents_base = fs::read_to_string(&fname_base)
        .expect("Unable to read left file")
        .leak();
    let fname_left = test_dir.join(format!("Left.{ext}"));
    let contents_left = fs::read_to_string(fname_left)
        .expect("Unable to read left file")
        .leak();
    let fname_right = test_dir.join(format!("Right.{ext}"));
    let contents_right = fs::read_to_string(fname_right)
        .expect("Unable to read right file")
        .leak();
    let fname_expected = test_dir.join(format!("Expected.{ext}"));
    let contents_expected = fs::read_to_string(fname_expected).expect("Unable to read right file");

    let merge_result = line_merge_and_structured_resolution(
        contents_base,
        contents_left,
        contents_right,
        fname_base,
        DisplaySettings::default(),
        true,
        None,
        None,
        Duration::from_millis(0),
    );

    let expected = contents_expected.trim();
    let actual = merge_result.contents.trim();
    if expected != actual {
        let patch = create_patch(expected, actual);
        let f = PatchFormatter::new().with_color();
        print!("{}", f.fmt_patch(&patch));
        eprintln!("test failed: outputs differ for {}", test_dir.display());
        panic!();
    }
}

/// End-to-end tests for the "mergiraf merge" command
#[rstest]
fn integration(#[files("examples/*/working/*")] path: PathBuf) {
    run_test_from_dir(&path);
}

// use this test to debug a specific test case by changing the path in it.
#[rstest]
fn debug_test() {
    run_test_from_dir(Path::new("examples/go/working/remove_and_add_imports"));
}
