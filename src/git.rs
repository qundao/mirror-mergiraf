use core::str;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

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

pub(crate) struct GitTempFiles {
    pub base: GitTempFile,
    pub left: GitTempFile,
    pub right: GitTempFile,
}

/// Extract the contents of all revisions (base, left, right) of a file to temporary files.
pub(crate) fn extract_all_revisions_from_git(
    repo_dir: &Path,
    path: &Path,
) -> Result<GitTempFiles, String> {
    let mut command = Command::new("git");
    command
        .arg("checkout-index")
        .arg("--stage=all")
        .arg(path)
        .current_dir(repo_dir);
    let output = command.output().map_err(|err| err.to_string())?;

    if !output.status.success() {
        let error_str = str::from_utf8(&output.stderr).map_err(|err| err.to_string())?;
        return Err(format!(
            "error while retrieving all revisions for {}:\n{}",
            path.display(),
            error_str
        ));
    }
    let output_str = str::from_utf8(&output.stdout).map_err(|err| err.to_string())?;
    // The format when using `--stage=all` is `stage1temp SP stage2temp SP stage3tmp TAB path RS`
    let mut lines = output_str
        .split_ascii_whitespace()
        // > stage fields are set to `.` if there is no entry for that stage
        // so cut off at the first `.` entry
        .take_while(|&p| p != ".")
        .map(|p| GitTempFile {
            path: repo_dir.join(p),
        });
    let base = lines.next().ok_or_else(|| {
        format!(
            "git did not return a temporary file path for base revision of {}",
            path.display()
        )
    })?;
    let left = lines.next().ok_or_else(|| {
        format!(
            "git did not return a temporary file path for left revision of {}",
            path.display()
        )
    })?;
    let right = lines.next().ok_or_else(|| {
        format!(
            "git did not return a temporary file path for right revision of {}",
            path.display()
        )
    })?;
    Ok(GitTempFiles { base, left, right })
}
