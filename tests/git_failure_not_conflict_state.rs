/// Test cases where a file has a conflict but is not in a conflicted state in Git's index for various reasons
use mergiraf::resolve_merge_cascading;
use mergiraf::settings::DisplaySettings;
use std::fs;

mod common;

#[test]
fn not_in_repo() {
    let temp_dir = tempfile::tempdir().unwrap();
    let repo_dir = temp_dir.path();
    let file_path = repo_dir.join("conflicted.py");
    let conflict_content = r#"def foo():
<<<<<<< ours
    a = 4
||||||| base
    a = 5
=======
    a = 6
>>>>>>> theirs
    return a
"#;
    fs::write(&file_path, conflict_content).unwrap();

    let handle = caplog::get_handle();
    // The file is just present in the working tree so it's not on a conflicted state
    let result = resolve_merge_cascading(
        conflict_content,
        &file_path,
        DisplaySettings::default(),
        None,
        repo_dir,
        None,
    );
    assert!(
        result.is_ok(),
        "Structured resolution errors are not terminal."
    );
    // nb: git error so could be localized?
    assert!(handle.any_msg_contains("not a git repository"));
    assert!(handle.any_msg_contains("Couldn't retrieve the original revisions from Git. This limits Mergiraf's ability to solve certain types of conflicts."));
}

#[test]
fn not_in_index() {
    let temp_dir = tempfile::tempdir().unwrap();
    let repo_dir = temp_dir.path();
    let file_path = repo_dir.join("conflicted.py");
    let conflict_content = r#"def foo():
<<<<<<< ours
    a = 4
||||||| base
    a = 5
=======
    a = 6
>>>>>>> theirs
    return a
"#;
    fs::write(&file_path, conflict_content).unwrap();
    common::run_git(&["init", "."], repo_dir);

    let handle = caplog::get_handle();
    // The file is just present in the working tree so it's not on a conflicted state
    let result = resolve_merge_cascading(
        conflict_content,
        &file_path,
        DisplaySettings::default(),
        None,
        repo_dir,
        None,
    );
    assert!(
        result.is_ok(),
        "Structured resolution errors are not terminal."
    );
    assert!(handle.any_msg_contains("conflicted.py is not in the cache"));
    assert!(handle.any_msg_contains("Couldn't retrieve the original revisions from Git. This limits Mergiraf's ability to solve certain types of conflicts."));
}

#[test]
fn committed_conflict() {
    let temp_dir = tempfile::tempdir().unwrap();
    let repo_dir = temp_dir.path();
    let file_path = repo_dir.join("conflicted.py");
    let conflict_content = r#"def foo():
<<<<<<< ours
    a = 4
||||||| base
    a = 5
=======
    a = 6
>>>>>>> theirs
    return a
"#;
    fs::write(&file_path, conflict_content).unwrap();
    common::run_git(&["init", "."], repo_dir);
    common::run_git(&["add", &file_path.to_string_lossy()], repo_dir);
    common::run_git(
        &[
            "-c",
            "user.name=Example",
            "-c",
            "user.email=example@example.org",
            "commit",
            "-m",
            "Initial commit",
        ],
        repo_dir,
    );

    let handle = caplog::get_handle();
    // The file is just present in the working tree so it's not on a conflicted state
    let result = resolve_merge_cascading(
        conflict_content,
        &file_path,
        DisplaySettings::default(),
        None,
        repo_dir,
        None,
    );
    assert!(
        result.is_ok(),
        "Structured resolution errors are not terminal."
    );
    assert!(handle.any_msg_contains("conflicted.py is not in a conflicted state."));
    assert!(handle.any_msg_contains("Couldn't retrieve the original revisions from Git. This limits Mergiraf's ability to solve certain types of conflicts."));
}
