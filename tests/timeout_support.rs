use std::borrow::Cow;
use std::fs::{self};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use diffy_imara::{PatchFormatter, create_patch};
use mergiraf::line_based::line_based_merge;
use mergiraf::line_merge_and_structured_resolution;
use mergiraf::merge::CliOpts;
use mergiraf::settings::DisplaySettings;

#[test]
fn timeout_support() {
    let test_dir = Path::new("examples/java/working/move_and_modify_conflict");
    let ext = "java";
    let fname_base = test_dir.join(format!("Base.{ext}")).leak();
    let contents_base = fs::read_to_string(&fname_base).expect("Unable to read base file");
    let contents_base = Arc::new(Cow::Owned(contents_base));
    let fname_left = test_dir.join(format!("Left.{ext}"));
    let contents_left = fs::read_to_string(fname_left).expect("Unable to read left file");
    let contents_left = Arc::new(Cow::Owned(contents_left));
    let fname_right = test_dir.join(format!("Right.{ext}"));
    let contents_right = fs::read_to_string(fname_right).expect("Unable to read right file");
    let contents_right = Arc::new(Cow::Owned(contents_right));
    let settings = DisplaySettings::default();
    let contents_expected =
        line_based_merge(&contents_base, &contents_left, &contents_right, &settings).contents;

    let merge_result = line_merge_and_structured_resolution(
        contents_base,
        contents_left,
        contents_right,
        fname_base,
        settings,
        true,
        None,
        CliOpts {
            allow_parse_errors: Some(true),
            ..Default::default()
        },
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
        eprintln!("test failed: outputs differ for '{}'", test_dir.display());
        panic!();
    }
}
