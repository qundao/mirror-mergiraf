use std::{fs, path::Path};

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
