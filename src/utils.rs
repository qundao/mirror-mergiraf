use std::cmp::min;
use std::error::Error;
use std::{fs, path::Path};

pub fn read_file(path: impl AsRef<Path>) -> Result<Vec<u8>, String> {
    let path = path.as_ref();
    fs::read(path).map_err(|err| format!("Could not read '{}': {err}", path.display()))
}

pub fn read_file_to_string(path: impl AsRef<Path>) -> Result<String, String> {
    let path = path.as_ref();
    fs::read_to_string(path).map_err(|err| format!("Could not read '{}': {err}", path.display()))
}

pub fn write_string_to_file(path: impl AsRef<Path>, contents: &str) -> Result<(), String> {
    let path = path.as_ref();
    fs::write(path, contents).map_err(|err| format!("Could not write '{}': {err}", path.display()))
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

// Stolen from https://git.kernel.org/pub/scm/git/git.git/tree/xdiff-interface.c?commit=db6938689f0a9ef5a9d630e8614d7f807758ff39#n197
pub fn buffer_is_binary(buf: &[u8]) -> bool {
    const FIRST_FEW_BYTES: usize = 8000;
    buf[..min(buf.len(), FIRST_FEW_BYTES)].contains(&0)
}

#[cfg(test)]
mod tests {
    #[test]
    #[allow(
        clippy::bool_assert_comparison,
        reason = "the suggestion makes it easy to miss the `!`"
    )]
    fn buffer_is_binary() {
        assert_eq!(super::buffer_is_binary(b"\0\xff\0"), true);
        assert_eq!(super::buffer_is_binary(b"I'm non-binary"), false);
    }
}
