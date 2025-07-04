#![allow(dead_code, reason = "the functions do get used in integration tests")]

use core::str;
use std::fs::{self, read_dir, read_to_string};
use std::path::{Path, PathBuf};
use std::process::Command;

use itertools::Itertools;
use mergiraf::lang_profile::LangProfile;

pub(crate) fn run_git(args: &[&str], repo_dir: &Path) {
    let command_str = format!("git {}", args.iter().format(" "));
    let mut command = Command::new("git");
    command.current_dir(repo_dir);
    command.args(args);
    command.env_remove("HOME"); // disable ~/.gitconfig to isolate the test better
    let output = command.output().expect("Failed to execute git command");
    if !output.status.success() {
        panic!(
            "git command failed: {command_str}\n{}",
            str::from_utf8(&output.stdout).unwrap()
        );
    }
}

pub(crate) fn write_file_from_rev(
    repo_dir: &Path,
    test_dir: &Path,
    revision: &str,
    suffix: &str,
) -> PathBuf {
    let file_name = format!("file{suffix}");
    let fname_base = test_dir.join(format!("{revision}{suffix}"));
    let contents = fs::read_to_string(&fname_base).expect("Unable to read left file");
    fs::write(repo_dir.join(&file_name), contents)
        .expect("failed to write test file to git repository");
    PathBuf::from(file_name)
}

/// Detect the suffix (including period) used by the revision files in a test case,
/// if any. Test files without extensions should declare the language to use in
/// a separate `language` file and just use bare `Base`, `Left` and `Right`
/// revision files (and similarly for expected outputs).
pub(crate) fn detect_test_suffix(test_dir: &Path) -> String {
    read_dir(test_dir)
        .expect("Could not list files in test directory")
        .find_map(|filename| {
            filename
                .unwrap()
                .file_name()
                .into_string()
                .expect("Unable to read filename in test directory")
                .strip_prefix("Base")
                .map(String::from)
        })
        .expect("Could not find a Base* file in the test directory")
}

/// Returns the language name specified in a test case (if any).
/// This is the contents of the `language` file in the test directory.
pub(crate) fn language_override_for_test(test_dir: &Path) -> Option<&'static str> {
    let contents = read_to_string(test_dir.join("language")).ok()?;
    let language_name = contents.trim();
    let lang_profile = LangProfile::find_by_name(language_name)
        .unwrap_or_else(|| panic!("Invalid identifier in 'language' file: '{language_name:?}'"));
    Some(lang_profile.name)
}
