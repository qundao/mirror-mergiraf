use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use diffy_imara::{PatchFormatter, create_patch};
use mergiraf::settings::DisplaySettings;
use mergiraf::{PathBufExt, line_merge_and_structured_resolution};
use rstest::rstest;

mod common;
use common::detect_extension;

#[derive(Clone, Copy)]
enum FailingTestResult {
    /// test failed in the expected manner
    FailsCorrectly,
    /// test failed to fail, and is now correct!
    NowCorrect,
    /// test failed, but in a new way
    FailsIncorrectly,
}

/// for failing tests, we store two "expected outputs"
/// - `ExpectedCurrently.{ext}` -- the output we see currently
/// - `ExpectedIdeally.{ext}` -- the output we hope to the output we'd like the see eventually, when the test no longer fails
///
/// later, output can change in 3 ways:
/// - becomes identical to `expected_ideally`
///   - great! the test no longer fails, so we can move it to `working` (well, almost -- see below)
/// - comes closer to `expected_ideally` (e.g. if we had multiple bugs, and fixed one of them)
///   - good! can update `ExpectedCurrently.{ext}`
/// - becomes even worse than before
///   - this could be seen as a regression, and a reason to ditch a PR, for example
///
/// The logic is complicated by the fact that, for some tests, we store not only the normal
/// expected output, but also the one in `--compact` mode. In those tests, getting one of the
/// outputs correct is not enough -- instead, we need to get both in order to move the test to
/// `working`. Note that this is no hard-and-fast rule -- we could have theoretically split those tests
/// in compact and non-compact versions -- but that would mean duplicating `{Base,Left,Right}.{ext}`, which is not ideal
#[rstest]
fn integration_failing(#[files("examples/*/failing/*")] test_dir: PathBuf) {
    let ext = detect_extension(&test_dir);
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

    let fname_expected_currently = test_dir.join(format!("ExpectedCurrently.{ext}"));
    let contents_expected_currently = fs::read_to_string(&fname_expected_currently)
        .expect("Unable to read expected-currently file");
    let fname_expected_ideally = test_dir.join(format!("ExpectedIdeally.{ext}"));
    let contents_expected_ideally =
        fs::read_to_string(fname_expected_ideally).expect("Unable to read expected-ideally file");

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

    let actual = merge_result.contents.trim();

    let expected_currently = contents_expected_currently.trim();
    let expected_ideally = contents_expected_ideally.trim();

    let result = if expected_currently == expected_ideally {
        FailingTestResult::NowCorrect
    } else if actual == expected_currently {
        FailingTestResult::FailsCorrectly
    } else if actual == expected_ideally {
        FailingTestResult::NowCorrect
    } else {
        FailingTestResult::FailsIncorrectly
    };

    // only run the following part if the file exists
    let fname_expected_compact_currently = test_dir.join(format!("ExpectedCompactCurrently.{ext}"));
    let Ok(contents_expected_compact_currently) =
        fs::read_to_string(&fname_expected_compact_currently)
    else {
        match result {
            FailingTestResult::FailsCorrectly => {
                // test failed in the expected manner
            }
            FailingTestResult::NowCorrect => {
                // if you find yourself seeing this message:
                // 1. move the test to `working` subdirectory
                // 2. rename `ExpectedIdeally.<extension>` to `Expected.<extension>`
                // 3. delete `ExpectedCurrently.<extension>`
                panic!(
                    "test for {} failed to fail -- it works now!",
                    test_dir.display()
                );
            }
            FailingTestResult::FailsIncorrectly => {
                let patch = create_patch(expected_currently, actual);
                let f = PatchFormatter::new().with_color();
                print!("{}", f.fmt_patch(&patch));
                eprintln!(
                    "\
non-compact test for {} failed, but output differs from what we currently expect
please examine the new output and update ExpectedCurrently.{ext} if it looks okay",
                    test_dir.display(),
                );
                panic!();
            }
        }
        return;
    };
    let fname_expected_compact_ideally = test_dir.join(format!("ExpectedCompactIdeally.{ext}"));
    let contents_expected_compact_ideally = fs::read_to_string(fname_expected_compact_ideally)
        .expect("could not read expected-compact-ideally file");

    let merge_result = line_merge_and_structured_resolution(
        contents_base,
        contents_left,
        contents_right,
        fname_base,
        DisplaySettings::default_compact(),
        true,
        None,
        None,
        Duration::from_millis(0),
    );

    let actual_compact = merge_result.contents.trim();

    let expected_compact_currently = contents_expected_compact_currently.trim();
    let expected_compact_ideally = contents_expected_compact_ideally.trim();

    let result_compact = if expected_compact_currently == expected_compact_ideally {
        FailingTestResult::NowCorrect
    } else if actual_compact == expected_compact_currently {
        FailingTestResult::FailsCorrectly
    } else if actual_compact == expected_compact_ideally {
        FailingTestResult::NowCorrect
    } else {
        FailingTestResult::FailsIncorrectly
    };

    match (result, result_compact) {
        (FailingTestResult::FailsCorrectly, FailingTestResult::FailsCorrectly) => {
            // both tests failed in the expected manner
        }
        (FailingTestResult::FailsCorrectly, FailingTestResult::NowCorrect)
        | (FailingTestResult::NowCorrect, FailingTestResult::FailsCorrectly) => {
            // one of the tests is still failing, so the whole test does so as well
        }
        (FailingTestResult::NowCorrect, FailingTestResult::NowCorrect) => {
            panic!(
                "both compact and non-compact cases are now correct!
the test can now be moved to under `working` as follows:
1. rename `ExpectedIdeally.{ext}` to `Expected.{ext}`
2. rename `ExpectedCompactIdeally.{ext}` to `ExpectedCompact.{ext}`
3. delete `ExpectedCurrently.{ext}` and `ExpectedCompactCurrently.{ext}`
"
            )
        }
        (FailingTestResult::FailsIncorrectly, _) | (_, FailingTestResult::FailsIncorrectly) => {
            // at least one of compact and non-compact failed in a new way

            if let FailingTestResult::FailsIncorrectly = result {
                let patch = create_patch(expected_currently, actual);
                let f = PatchFormatter::new().with_color();
                println!(
                    "the non-compact test fails, but in a new way
please examine the new output and update ExpectedCurrently.{ext} if it looks okay:
{}",
                    f.fmt_patch(&patch)
                );
            }

            if let FailingTestResult::FailsIncorrectly = result_compact {
                let patch = create_patch(expected_compact_currently, actual_compact);
                let f = PatchFormatter::new().with_color();
                println!(
                    "the compact test fails, but in a new way
please examine the new output and update ExpectedCompactCurrently.{ext} if it looks okay:
{}",
                    f.fmt_patch(&patch)
                );
            }

            panic!()
        }
    }
}
