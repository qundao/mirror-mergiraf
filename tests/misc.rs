use assert_cmd::{pkg_name, prelude::*};
use mergiraf::{EXIT_SOLVE_HAS_CONFLICTS, git, utils::write_string_to_file};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::{env, fs};

#[track_caller]
fn merge() -> Command {
    let mut cmd = Command::cargo_bin(pkg_name!()).unwrap();
    cmd.arg("merge");
    cmd
}

#[track_caller]
fn solve() -> Command {
    let mut cmd = Command::cargo_bin(pkg_name!()).unwrap();
    cmd.arg("solve");
    cmd
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
    solve()
        .arg("--keep-backup=false")
        .arg(&test_file_abs_path)
        .assert()
        .success();

    assert!(!test_file_orig_file_path.exists());

    // `solve` once again but with backup this time
    solve()
        .arg("--keep-backup=true")
        .arg(&test_file_abs_path)
        .assert()
        .success();

    assert!(test_file_orig_file_path.exists());
}

const DEFAULT_FILE_FOR_SOLVE: &str =
    "<<<<<<< LEFT\n[1, 2, 3, 4]\n||||||| BASE\n[1, 2, 3]\n=======\n[0, 1, 2, 3]\n>>>>>>> RIGHT\n";

fn create_file_for_solve(repo_path: &Path, contents: impl AsRef<[u8]>) -> PathBuf {
    let test_file_name = "test.txt";
    let test_file_abs_path = repo_path.join(test_file_name);
    fs::write(&test_file_abs_path, contents).expect("failed to write test file to git repository");

    test_file_abs_path
}

fn create_files_for_merge(
    repo_path: &Path,
    base_contents: impl AsRef<[u8]>,
    left_contents: impl AsRef<[u8]>,
    right_contents: impl AsRef<[u8]>,
) -> (PathBuf, PathBuf, PathBuf, PathBuf) {
    let base_file_name = "base.txt";
    let left_file_name = "left.txt";
    let right_file_name = "right.txt";
    let output_file_name = "output.txt";

    let base_file_abs_path = repo_path.join(base_file_name);
    fs::write(&base_file_abs_path, base_contents)
        .expect("failed to write test base file to git repository");
    let left_file_abs_path = repo_path.join(left_file_name);
    fs::write(&left_file_abs_path, left_contents)
        .expect("failed to write test left file to git repository");
    let right_file_abs_path = repo_path.join(right_file_name);
    fs::write(&right_file_abs_path, right_contents)
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

    let test_file_abs_path = create_file_for_solve(repo_path, DEFAULT_FILE_FOR_SOLVE);

    // first try without specifying a language -- should fail
    solve()
        // language not specified
        .arg(&test_file_abs_path)
        .assert()
        .failure();

    // then try with a language specified on the CLI -- should succeed
    solve()
        .arg("--language=json")
        .arg(&test_file_abs_path)
        .assert()
        .success();

    let merge_result =
        fs::read_to_string(test_file_abs_path).expect("couldn't read the merge result");
    assert_eq!(merge_result, "[0, 1, 2, 3, 4]\n");
}

#[test]
fn manual_language_selection_for_merge() {
    let repo_dir = tempfile::tempdir().expect("failed to create the temp dir");
    let repo_path = repo_dir.path();

    let (base_file_abs_path, left_file_abs_path, right_file_abs_path, _) =
        create_files_for_merge(repo_path, "[1, 2, 3]\n", "[1, 2, 3, 4]\n", "[0, 1, 2, 3]\n");

    // first try without specifying a language -- should fail
    merge()
        .arg(&base_file_abs_path)
        .arg(&left_file_abs_path)
        .arg(&right_file_abs_path)
        // language not specified
        .assert()
        .failure();

    // then try with a language specified on the CLI -- should succeed
    merge()
        .arg("--language=json")
        .arg(base_file_abs_path)
        .arg(left_file_abs_path)
        .arg(right_file_abs_path)
        .assert()
        .success()
        .stdout("[0, 1, 2, 3, 4]\n");
}

#[test]
fn debug_dir_is_created_for_solve() {
    let repo_dir = tempfile::tempdir().expect("failed to create the temp dir");
    let repo_path = repo_dir.path();

    let test_file_abs_path = create_file_for_solve(repo_path, DEFAULT_FILE_FOR_SOLVE);

    let debug_dir = tempfile::tempdir().unwrap();
    let debug_dir_path = debug_dir.path().to_path_buf();
    // hopefully no one creates a tmp file with the same exact name directly after we've
    // deleted our one
    debug_dir.close().unwrap();

    solve()
        .arg("--language=json")
        .arg(test_file_abs_path)
        .arg("--debug")
        .arg(&debug_dir_path)
        .assert()
        .success();

    assert!(fs::exists(debug_dir_path).unwrap());
}

#[test]
fn debug_dir_is_created_for_merge() {
    let repo_dir = tempfile::tempdir().expect("failed to create the temp dir");
    let repo_path = repo_dir.path();

    let (base_file_abs_path, left_file_abs_path, right_file_abs_path, _) =
        create_files_for_merge(repo_path, "[1, 2, 3]", "[1, 2, 3, 4]", "[0, 1, 2, 3]");

    let debug_dir = tempfile::tempdir().unwrap();
    let debug_dir_path = debug_dir.path().to_path_buf();
    // hopefully no one creates a tmp file with the same exact name directly after we've
    // deleted our one
    debug_dir.close().unwrap();

    merge()
        .arg("--language=json")
        .arg(base_file_abs_path)
        .arg(left_file_abs_path)
        .arg(right_file_abs_path)
        .arg("--debug")
        .arg(&debug_dir_path)
        .assert()
        .success();

    assert!(fs::exists(debug_dir_path).unwrap());
}

#[test]
fn line_ending_preservation_for_solve() {
    let repo_dir = tempfile::tempdir().expect("failed to create the temp dir");
    let repo_path = repo_dir.path();

    let test_file_abs_path = create_file_for_solve(
        repo_path,
        "<<<<<<< LEFT\r\n[1, 2, 3, 4]\r\n||||||| BASE\r\n[1, 2, 3]\r\n=======\r\n[0, 1, 2, 3]\r\n>>>>>>> RIGHT\r\n",
    );

    solve()
        .arg("--language=json")
        .arg(&test_file_abs_path)
        .assert()
        .success();

    let merge_result =
        fs::read_to_string(&test_file_abs_path).expect("couldn't read the merge result");
    assert_eq!(merge_result, "[0, 1, 2, 3, 4]\r\n");

    let backup_contents = fs::read_to_string(test_file_abs_path.with_extension("txt.orig"))
        .expect("couldn't read the backup file");
    assert!(backup_contents.contains("\r\n"));
}

#[test]
fn line_ending_preservation_for_merge() {
    let repo_dir = tempfile::tempdir().expect("failed to create the temp dir");
    let repo_path = repo_dir.path();

    let (base_file_abs_path, left_file_abs_path, right_file_abs_path, _) = create_files_for_merge(
        repo_path,
        "[1, 2, 3]\r\n",
        "[1, 2, 3, 4]\r\n",
        "[0, 1, 2, 3]\r\n",
    );

    merge()
        .arg("--language=json")
        .arg(base_file_abs_path)
        .arg(left_file_abs_path)
        .arg(right_file_abs_path)
        .assert()
        .success()
        .stdout("[0, 1, 2, 3, 4]\r\n");
}

fn create_iso8859_input_files(repo_path: &Path) -> (PathBuf, PathBuf, PathBuf, PathBuf) {
    create_files_for_merge(
        repo_path,
        b"\x68\xe9\x0a\x6c\xe0\x0a",
        b"\x68\xe9\x0a\x6c\xe0\x0a\x74\x6f\x69\x0a",
        b"\x79\x6f\x0a\x68\xe9\x0a\x6c\xe0\x0a",
    )
}

#[test]
fn merging_non_utf8_files_line_based_git_mode() {
    let repo_dir = tempfile::tempdir().expect("failed to create the temp dir");
    let repo_path = repo_dir.path();

    let (base_file_abs_path, left_file_abs_path, right_file_abs_path, _) =
        create_iso8859_input_files(repo_path);

    merge()
        .arg("--git")
        .arg("--language=json") // pretend those are JSON files so that we attempt to read them
        .arg(base_file_abs_path)
        .arg(&left_file_abs_path)
        .arg(right_file_abs_path)
        .assert()
        .success();

    let merge_result = fs::read(left_file_abs_path).expect("couldn't read the merge result");
    assert_eq!(
        merge_result,
        b"\x79\x6f\x0a\x68\xe9\x0a\x6c\xe0\x0a\x74\x6f\x69\x0a"
    );
}

#[test]
fn merging_non_utf8_files_line_based_with_output_file() {
    let repo_dir = tempfile::tempdir().expect("failed to create the temp dir");
    let repo_path = repo_dir.path();

    let (base_file_abs_path, left_file_abs_path, right_file_abs_path, output_file_abs_path) =
        create_iso8859_input_files(repo_path);

    // `mergiraf merge` should do line-based merges via git for files in ISO-8859-1
    merge()
        .arg("--language=json") // pretend those are JSON files so that we attempt to read them
        .arg(base_file_abs_path)
        .arg(left_file_abs_path)
        .arg(right_file_abs_path)
        .arg("--output")
        .arg(&output_file_abs_path)
        .assert()
        .success();

    let merge_result = fs::read(output_file_abs_path).expect("couldn't read the merge result");
    assert_eq!(
        merge_result,
        b"\x79\x6f\x0a\x68\xe9\x0a\x6c\xe0\x0a\x74\x6f\x69\x0a"
    );
}

#[test]
fn merging_non_existing_files() {
    let repo_dir = tempfile::tempdir().expect("failed to create the temp dir");
    let repo_path = repo_dir.path();

    let base_file_abs_path = repo_path.join("does_not_exist_1.json");
    let left_file_abs_path = repo_path.join("does_not_exist_2.json");
    let right_file_abs_path = repo_path.join("does_not_exist_3.json");

    merge()
        .arg("--git")
        .arg(base_file_abs_path)
        .arg(left_file_abs_path)
        .arg(right_file_abs_path)
        .assert()
        .append_context(
            "main",
            "`mergiraf merge` should return exit code 255 when passed non-existing files",
        )
        .code(255);
}

#[test]
fn merging_files_with_conflict_markers_cause_fallback_to_git_merge_file() {
    let contents_base = "\
/**
 * Doc comment
 */
class MyClass {
}";
    let contents_left = "\
/**
<<<<<<< HEAD
 * Doc comment
=======
 * Better docs
>>>>>>> origin/main
 */
class MyClass {
}";
    let contents_right = "\
/**
 * Doc comment
 */
class MyClass {
}

class OtherClass {
}";
    let contents_expected = "\
/**
<<<<<<< HEAD
 * Doc comment
=======
 * Better docs
>>>>>>> origin/main
 */
class MyClass {
}

class OtherClass {
}";

    let repo_dir = tempfile::tempdir().expect("failed to create the temp dir");
    let repo_path = repo_dir.path();

    let (base_file_abs_path, left_file_abs_path, right_file_abs_path, _) =
        create_files_for_merge(repo_path, contents_base, contents_left, contents_right);

    merge()
        .arg("--language=java")
        .arg(base_file_abs_path)
        .arg(left_file_abs_path)
        .arg(right_file_abs_path)
        .assert()
        .success()
        .stdout(contents_expected)
        .stderr("WARN left side contains conflict markers, falling back to Git\n");
}

#[test]
fn verify_cli_solve_has_conflicts() {
    let repo_dir = tempfile::tempdir().expect("failed to create the temp dir");
    let repo_path = repo_dir.path();

    let content = r#"
<html>
   <body>
       <foo
<<<<<<< LEFT
          iota="i"
||||||| BASE
=======
          iota="i2"
>>>>>>> RIGHT
          alpha="a"
          beta="b"
          gamma="c"
          delta="d"
       />
       <bar />
   </body>
</html>
         "#;
    let test_file_abs_path = create_file_for_solve(repo_path, content);

    solve()
        .arg(&test_file_abs_path)
        .arg("--language=html")
        .arg("--stdout")
        .assert()
        .code(EXIT_SOLVE_HAS_CONFLICTS)
        .stdout(content);
}

#[test]
fn solve_respects_conflict_marker_size_attr() {
    let repo_dir = tempfile::tempdir().expect("failed to create the temp dir");
    let repo_path = repo_dir.path();
    git::init(repo_path);

    let contents = "\
<<<<<<<<<< LEFT
[1, 2]
|||||||||| BASE
[1, 1]
==========
[2, 1]
>>>>>>>>>> RIGHT
";
    let contents_after_solve = "\
[2, 2]
";

    let conflict_path = create_file_for_solve(repo_path, contents);

    solve()
        .arg("--language=json")
        .arg(&conflict_path)
        .arg("--stdout")
        .current_dir(repo_path)
        .assert()
        .append_context(
            "main",
            "should fail to solve a conflict (and therefore leave it unchanged) with a non-standard conflict marker size",
        )
        // FIXME: this should arguably have been 1, as the conflict wasn't solved.
        // But to be fair to Mergiraf, it can't even see that this is a conflict,
        // precisely because of the non-standard conflict marker sizes
        .code(0)
        .stdout(contents);

    write_string_to_file(
        repo_path.join(".gitattributes"),
        "* conflict-marker-size=10",
    )
    .unwrap();

    solve()
        .arg("--language=json")
        .arg(conflict_path)
        .arg("--stdout")
        .current_dir(repo_path)
        .assert()
        .append_context(
            "main",
            "should be able to solve a conflict with a provided conflict marker size",
        )
        .code(0)
        .stdout(contents_after_solve);
}
