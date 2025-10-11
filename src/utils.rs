use std::error::Error;
use std::{fs, path::Path};

use regex::RegexBuilder;

pub fn read_file_to_string(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|err| format!("Could not read {}: {err}", path.display()))
}

pub fn write_string_to_file(path: impl AsRef<Path>, contents: &str) -> Result<(), String> {
    let path = path.as_ref();
    fs::write(path, contents).map_err(|err| format!("Could not write {}: {err}", path.display()))
}

pub fn detect_suffix(test_dir: &Path) -> String {
    fs::read_dir(test_dir)
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
        .expect("Could not find a Base.* file in the test directory")
}

/// Returns the maximum length of conflict markers in the file
/// (even if they appear in an order incompatible with them being conflict markers).
pub fn max_conflict_marker_length(contents: &str) -> usize {
    let regex = RegexBuilder::new("^(<+|=+|\\|+|>+)")
        .multi_line(true)
        .build()
        .expect("Invalid regex");
    regex
        .find_iter(contents)
        .map(|m| m.len())
        .max()
        .unwrap_or(0)
}

pub(crate) trait InternalError {
    fn debug_panic(self) -> Self;
}

impl<V, E: Error> InternalError for Result<V, E> {
    /// Panic if this result is an error and we are in debug mode.
    /// This is useful for internal errors that are meant to be never reached,
    /// but that we want to be able to gracefully recover from in release mode.
    #[track_caller]
    #[inline]
    fn debug_panic(self) -> Self {
        if cfg!(debug_assertions) {
            Ok(self.unwrap())
        } else {
            self
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn conflict_marker_length() {
        assert_eq!(max_conflict_marker_length("a\n<<< b\nc"), 3);
        assert_eq!(max_conflict_marker_length("a\nb\n\nc\n"), 0);
        assert_eq!(max_conflict_marker_length("a\n<<< b\n== c\nd\n>>>>\n"), 4);
    }
}
