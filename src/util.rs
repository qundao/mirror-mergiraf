use std::{fs, path::Path};

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
