use std::fs;
use std::process::Command;

use itertools::Itertools as _;
use mergiraf::resolve_merge_cascading;
use mergiraf::settings::DisplaySettings;

mod common;
use common::run_git;

static BASE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/examples/rust/working/move_to_method/Base.rs"
));
static LEFT: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/examples/rust/working/move_to_method/Left.rs"
));
static RIGHT: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/examples/rust/working/move_to_method/Right.rs"
));
static EXPECTED: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/examples/rust/working/move_to_method/Expected.rs"
));

#[test]
fn oid_fallback_extracts_revisions_and_solves() {
    let repo_dir = tempfile::tempdir().expect("failed to create temp dir");
    let repo_dir = repo_dir.path();
    run_git(&["init", "-b", "main"], repo_dir);
    run_git(&["config", "user.email", "test@example.com"], repo_dir);
    run_git(&["config", "user.name", "Test User"], repo_dir);

    let file_name = "file.rs";
    let file = repo_dir.join(file_name);
    fs::write(&file, BASE).unwrap();
    run_git(&["add", file_name], repo_dir);
    run_git(&["commit", "-m", "base"], repo_dir);

    run_git(&["switch", "-c", "left"], repo_dir);
    fs::write(&file, LEFT).unwrap();
    run_git(&["commit", "-am", "left change"], repo_dir);

    run_git(&["switch", "-c", "right", "main"], repo_dir);
    fs::write(&file, RIGHT).unwrap();
    run_git(&["commit", "-am", "right change"], repo_dir);

    let output = Command::new("git")
        .args(["rev-parse", "main", "left", "right"])
        .current_dir(repo_dir)
        .output()
        .expect("failed to get OIDs for branches");

    let out = String::from_utf8_lossy(&output.stdout);
    let (base_oid, left_oid, right_oid) = out.lines().collect_tuple().unwrap();
    //Generate the merge result with OID markers
    let merge_tree_output = Command::new("git")
        .args(["-c", "merge.conflictStyle=diff3"])
        .arg("merge-tree")
        .args(["--merge-base", base_oid, left_oid, right_oid])
        .current_dir(repo_dir)
        .output()
        .unwrap();
    assert!(
        !merge_tree_output.status.success(),
        "git merge-tree succeeded unexpectedly"
    );

    let out = String::from_utf8_lossy(&merge_tree_output.stdout);
    let tree_oid = out.lines().next().unwrap();

    // Retrieve blob with conflict markers
    let output = Command::new("git")
        .arg("show")
        .arg(format!("{tree_oid}:{file_name}"))
        .current_dir(repo_dir)
        .output()
        .unwrap();
    assert!(output.status.success());
    let conflicted_content = std::str::from_utf8(&output.stdout).unwrap();

    // validate that if we're outside a git repository the resolution fails
    let not_repo = tempfile::tempdir().unwrap();
    let merge_result = resolve_merge_cascading(
        conflicted_content,
        std::path::Path::new(file_name),
        DisplaySettings::default(),
        None,
        not_repo.path(),
        None,
    )
    .unwrap();
    assert_eq!(merge_result.conflict_count, 1);

    // Check that if the revisions *can* be looked up by oid the merge succeeds
    let merge_result = resolve_merge_cascading(
        conflicted_content,
        std::path::Path::new(file_name),
        DisplaySettings::default(),
        None,
        repo_dir,
        None,
    )
    .unwrap();
    assert_eq!(merge_result.conflict_count, 0);
    assert_eq!(merge_result.contents, EXPECTED);
}
