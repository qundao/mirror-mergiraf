use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use diffy_imara::{PatchFormatter, create_patch};
use mergiraf::settings::DisplaySettings;
use mergiraf::{PathBufExt, line_merge_and_structured_resolution};
use rstest::rstest;

mod common;
use common::detect_extension;

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
#[test]
fn debug_test() {
    run_test_from_dir(Path::new("examples/go/working/remove_and_add_imports"));
}
