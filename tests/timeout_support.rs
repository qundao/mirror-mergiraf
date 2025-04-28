use std::fs::{self};
use std::path::Path;
use std::time::Duration;

use diffy_imara::{PatchFormatter, create_patch};
use mergiraf::line_based::line_based_merge;
use mergiraf::settings::DisplaySettings;
use mergiraf::{PathBufExt, line_merge_and_structured_resolution};

#[test]
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
        None,
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
