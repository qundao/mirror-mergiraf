#![allow(dead_code, reason = "the functions do get used in integration tests")]

use core::str;
use std::fs::{self, read_dir};
use std::path::{Path, PathBuf};
use std::process::Command;

use itertools::Itertools;

pub(crate) fn run_git(args: &[&str], repo_dir: &Path) {
    let command_str = format!("git {}", args.iter().format(" "));
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

pub(crate) fn write_file_from_rev(
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

pub(crate) fn detect_extension(test_dir: &Path) -> String {
    read_dir(test_dir)
        .expect("Could not list files in test directory")
        .find_map(|filename| {
            filename
                .unwrap()
                .file_name()
                .into_string()
                .expect("Unable to read filename in test directory")
                .strip_prefix("Base.")
                .map(String::from)
        })
        .expect("Could not find a Base.* file in the test directory")
}
