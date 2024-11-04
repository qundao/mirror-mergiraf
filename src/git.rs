use core::str;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use crate::{pcs::Revision, settings::DisplaySettings};

pub(crate) struct GitTempFile {
    path: PathBuf,
}

impl GitTempFile {
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for GitTempFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
        ()
    }
}

/// Extract the contents of a file at a particular revision, to a temporary file.
pub(crate) fn extract_revision_from_git(
    path: &Path,
    revision: Revision,
) -> Result<GitTempFile, String> {
    let mut command = Command::new("git");
    command
        .arg("checkout-index")
        .arg("--temp")
        .arg(match revision {
            Revision::Base => "--stage=1",
            Revision::Left => "--stage=2",
            Revision::Right => "--stage=3",
        })
        .arg(path)
        .output()
        .map_err(|err| err.to_string())
        .and_then(|output| {
            if !output.status.success() {
                let error_str = str::from_utf8(&output.stderr).map_err(|err| err.to_string())?;
                return Err(format!(
                    "error while retrieving {} revision for {}:\n{}",
                    revision,
                    path.display(),
                    error_str
                ));
            }
            let output_str = str::from_utf8(&output.stdout).map_err(|err| err.to_string())?;
            let temp_file_path = output_str.split_ascii_whitespace().next().ok_or_else(|| {
                format!(
                    "git did not return a temporary file path for {} revision of {}",
                    revision,
                    path.display()
                )
            })?;
            Ok(GitTempFile {
                path: PathBuf::from(temp_file_path),
            })
        })
}

pub(crate) fn fallback_to_git_merge_file(
    base: String,
    left: String,
    right: String,
    git: bool,
    settings: &DisplaySettings,
) -> Result<i32, String> {
    let mut command = Command::new("git");
    command.arg("merge-file").arg("--diff-algorithm=histogram");
    if !git {
        command.arg("-p");
    }
    command
        .arg("-L")
        .arg(&settings.left_revision_name)
        .arg("-L")
        .arg(&settings.base_revision_name)
        .arg("-L")
        .arg(&settings.right_revision_name)
        .arg(left)
        .arg(base)
        .arg(right)
        .spawn()
        .and_then(|mut process| {
            process
                .wait()
                .map(|exit_status| exit_status.code().unwrap_or(0))
        })
        .map_err(|err| err.to_string())
}
