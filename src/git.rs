use core::str;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use crate::pcs::Revision;

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
    }
}

/// Extract the contents of a file at a particular revision, to a temporary file.
pub(crate) fn extract_revision_from_git(
    repo_dir: &Path,
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
        .current_dir(repo_dir);
    let output = command.output().map_err(|err| err.to_string())?;

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
        path: repo_dir.join(temp_file_path),
    })
}
