use std::fs::{self, read_dir};
use std::path::PathBuf;

use diffy::{create_patch, PatchFormatter};
use mergiraf::line_merge_and_structured_resolution;
use mergiraf::settings::DisplaySettings;
use rstest::rstest;

fn run_test_from_dir(test_dir: &PathBuf) {
    let ext = read_dir(test_dir)
        .expect("Could not list files in test directory")
        .into_iter()
        .map(|filename| {
            filename
                .unwrap()
                .file_name()
                .into_string()
                .expect("Unable to read filename in test directory")
                .strip_prefix("Base.")
                .map(|s| s.to_owned())
        })
        .flatten()
        .next()
        .expect("Could not find a Base.* file in the test directory");
    let test_dir = test_dir.display();
    let fname_base = format!("{}/Base.{}", test_dir, ext);
    let contents_base = fs::read_to_string(&fname_base).expect("Unable to read left file");
    let fname_left = format!("{}/Left.{}", test_dir, ext);
    let contents_left = fs::read_to_string(fname_left).expect("Unable to read left file");
    let fname_right = format!("{}/Right.{}", test_dir, ext);
    let contents_right = fs::read_to_string(fname_right).expect("Unable to read right file");
    let fname_expected = format!("{}/Expected.{}", test_dir, ext);
    let contents_expected = fs::read_to_string(fname_expected).expect("Unable to read right file");

    let merge_result = line_merge_and_structured_resolution(
        &contents_base,
        &contents_left,
        &contents_right,
        &fname_base,
        &DisplaySettings::default(),
        true,
        None,
        &None,
    );

    let expected = contents_expected.trim();
    let actual = merge_result.contents.trim();
    if expected != actual {
        let patch = create_patch(expected, actual);
        let f = PatchFormatter::new().with_color();
        print!("{}", f.fmt_patch(&patch));
        eprintln!("test failed: outputs differ for {test_dir}");
        panic!();
    }
}

#[rstest]
fn integration(#[files("examples/*/working/*")] path: PathBuf) {
    run_test_from_dir(&path);
}

// use this test to debug a specific test case by changing the path in it.
#[rstest]
fn debug_test() {
    run_test_from_dir(&PathBuf::from("examples/go/working/remove_and_add_imports"))
}
