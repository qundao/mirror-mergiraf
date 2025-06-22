use core::str;
use itertools::Itertools as _;
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
    pub base: Option<GitTempFile>,
    pub left: Option<GitTempFile>,
    pub right: Option<GitTempFile>,
}

/// Extract the contents of all revisions (base, left, right) of a file to temporary files.
pub(crate) fn extract_all_revisions_from_git(
    repo_dir: &Path,
    path: &Path,
) -> Result<GitTempFiles, String> {
    let output = Command::new("git")
        .arg("checkout-index")
        .arg("--stage=all")
        .arg(path)
        .current_dir(repo_dir)
        .output()
        .map_err(|err| err.to_string())?;

    if !output.status.success() {
        let error_str = str::from_utf8(&output.stderr).map_err(|err| err.to_string())?;
        return Err(format!(
            "error while retrieving all revisions for {}:\n{}",
            path.display(),
            error_str
        ));
    }
    let output_str = str::from_utf8(&output.stdout).map_err(|err| err.to_string())?;
    // No entries are returned for files in stage 0 (in index but not conflicted)
    if output_str.is_empty() {
        return Err(format!("{} is not in a conflicted state.", path.display()));
    }
    // The format when using `--stage=all` is `stage1temp SP stage2temp SP stage3tmp TAB path RS`
    let lines = output_str
        .split_ascii_whitespace()
        .take(3)
        // > stage fields are set to `.` if there is no revision for that stage
        .map(|p| {
            (p != ".").then(|| GitTempFile {
                path: repo_dir.join(p),
            })
        });
    if let Some((base, left, right)) = lines.collect_tuple() {
        Ok(GitTempFiles { base, left, right })
    } else {
        Err(format!("invalid checkout-index output: {output_str}"))
    }
}

fn read_content_from_commit(repo_dir: &Path, oid: &str, file_name: &Path) -> Option<String> {
    Command::new("git")
        .args(["show", &format!("{}:{}", oid, file_name.display())])
        .current_dir(repo_dir)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| output.stdout)
        .and_then(|c| String::from_utf8(c).ok())
}

/// Extracts the content of all revisions of a file from relevant commits by oid.
pub(crate) fn read_content_from_commits(
    repo_dir: &Path,
    oids: (&str, &str, &str),
    file_name: &Path,
) -> Option<(String, String, String)> {
    Some((
        read_content_from_commit(repo_dir, oids.0, file_name)?,
        read_content_from_commit(repo_dir, oids.1, file_name)?,
        read_content_from_commit(repo_dir, oids.2, file_name)?,
    ))
}
