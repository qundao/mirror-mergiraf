use core::str;
use std::{
    borrow::Cow,
    fmt::Display,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use etcetera::{AppStrategy, AppStrategyArgs, choose_app_strategy};
use log::warn;
use rand::distr::{Alphanumeric, SampleString};

use crate::line_based::LINE_BASED_METHOD;

/// An identifier of an attempt to merge a file
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct Attempt<'a> {
    pub(crate) file_name: &'a str,
    pub(crate) uid: Cow<'a, str>,
    extension: &'a str,
    dir: PathBuf,
}

const DEFAULT_FILE_EXTENSION: &str = "txt";
const BEST_MERGE_FILENAME: &str = "best_merge.txt";
const ATTEMPTS_DIRECTORY: &str = "merges";
const DEFAULT_CACHE_SIZE: usize = 128;

impl Display for Attempt<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Attempt[{}_{}]", self.file_name, self.uid)
    }
}

impl Attempt<'_> {
    pub(crate) fn write(&self, file_name: &str, contents: &str) -> Result<(), String> {
        let path = self.path(file_name);
        fs::write(&path, contents)
            .map_err(|err| format!("Could not write {}: {err}", &path.display()))
    }

    pub(crate) fn write_best_merge_id(&self, method: &str) -> Result<(), String> {
        let path = self.dir.join(BEST_MERGE_FILENAME);
        fs::write(&path, method)
            .map_err(|err| format!("Could not write {}: {err}", &path.display()))
    }

    pub(crate) fn best_merge_id(&self) -> Result<String, String> {
        let path = self.dir.join(BEST_MERGE_FILENAME);
        fs::read_to_string(&path)
            .map_err(|err| format!("Could not read best merge id at {}: {err}", &path.display()))
    }

    pub(crate) fn path(&self, file_name: &str) -> PathBuf {
        self.dir.join(format!("{}.{}", file_name, self.extension))
    }

    pub(crate) fn id(&self) -> String {
        format!("{}_{}", self.file_name, self.uid)
    }
}

/// A cache holding a collection of merge attempts to enable their review
#[derive(Debug, Clone)]
pub struct AttemptsCache {
    base_dir: PathBuf,
    max_size: usize,
}

impl AttemptsCache {
    /// Create a new cache. If no path is supplied, a location will be picked based
    /// on the operating system's conventions.
    /// Returns an error when that fails or the directory cannot be recursively created.
    pub fn new(base_dir: Option<&Path>, max_size: Option<usize>) -> Result<Self, String> {
        let cache_dir = base_dir
            .map(Path::to_path_buf)
            .or_else(|| {
                let strategy = choose_app_strategy(AppStrategyArgs {
                    top_level_domain: "org".to_owned(),
                    author: "Mergiraf contributors".to_owned(),
                    app_name: "Mergiraf".to_owned(),
                })
                .ok();
                strategy.map(|project_dir| project_dir.data_dir().clone().join(ATTEMPTS_DIRECTORY))
            })
            .ok_or_else(|| {
                "Could not determine a suitable application data directory to store merge attempts"
                    .to_string()
            })?;
        fs::create_dir_all(&cache_dir).map_err(|err| {
            format!(
                "Error while creating merge attempts directory {}: {err}",
                cache_dir.display()
            )
        })?;
        Ok(Self {
            base_dir: cache_dir,
            max_size: max_size.unwrap_or(DEFAULT_CACHE_SIZE),
        })
    }

    /// Registers a new attempt and stores the contents of the revisions in it
    pub(crate) fn new_attempt<'a>(
        &'a self,
        final_path: &'a Path,
        contents_base: &str,
        contents_left: &str,
        contents_right: &str,
    ) -> Result<Attempt<'a>, String> {
        let file_name = final_path
            .file_name()
            .and_then(|file_name| file_name.to_str())
            .unwrap_or("file");
        let extension = final_path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or(DEFAULT_FILE_EXTENSION);

        let uid = Alphanumeric.sample_string(&mut rand::rng(), 8);
        let dir_name = format!("{file_name}_{uid}");
        let dir = self.base_dir.join(dir_name);
        fs::create_dir_all(&dir).map_err(|err| {
            format!(
                "Error while creating merge attempt directory {}: {err}",
                dir.display()
            )
        })?;

        let attempt = Attempt {
            file_name,
            uid: Cow::from(uid),
            dir,
            extension,
        };
        attempt.write("Base", contents_base)?;
        attempt.write("Left", contents_left)?;
        attempt.write("Right", contents_right)?;
        self.prune()?;
        Ok(attempt)
    }

    pub(crate) fn parse_attempt_id<'a>(
        &'a self,
        attempt_id: &'a str,
    ) -> Result<Attempt<'a>, String> {
        let (file_name, uid) = attempt_id
            .rsplit_once('_')
            .ok_or_else(|| "Invalid attempt id, should contain a '_' character".to_owned())?;

        let extension = file_name
            .rsplit_once('.')
            .map_or(DEFAULT_FILE_EXTENSION, |(_, extension)| extension);

        let dir_name = attempt_id;
        let dir = self.base_dir.join(dir_name);
        if !dir.exists() {
            return Err(format!("Could not find merge attempt with id {attempt_id}"));
        }
        Ok(Attempt {
            file_name,
            uid: Cow::from(uid),
            extension,
            dir,
        })
    }

    /// Reviews an attempt by showing the diff between the line-based merge and Mergiraf's
    pub fn review_merge(&self, attempt_id: &str) -> Result<(), String> {
        let attempt = self.parse_attempt_id(attempt_id)?;
        let path_line_based = attempt.path(LINE_BASED_METHOD);
        let best_merge_file_path = attempt.dir.join(BEST_MERGE_FILENAME);
        let best_merge_id = fs::read_to_string(&best_merge_file_path)
            .map_err(|err| {
                format!(
                    "Failed to read best merge method in {}: {}",
                    best_merge_file_path.display(),
                    err
                )
            })
            .expect("Failed to read best merge id");
        let path_best_merge = attempt.path(best_merge_id.trim());
        if !path_best_merge.exists() {
            return Err(format!("Could not read {}", path_best_merge.display()));
        }
        Command::new("git")
            .arg("diff")
            .arg("--no-index")
            .arg(path_line_based)
            .arg(path_best_merge)
            .spawn()
            .map(|mut process| {
                let _ = process.wait();
            })
            .map_err(|err| err.to_string())
    }

    /// Removes older attempts so that the cache doesn't grow too much
    fn prune(&self) -> Result<(), String> {
        let dir_listing = fs::read_dir(&self.base_dir).map_err(|err| err.to_string())?;
        let subdirs = {
            let mut subdirs: Vec<_> = dir_listing
                .flatten()
                .filter_map(|f| {
                    let metadata = f.metadata().ok()?;
                    let mtime = metadata.modified().ok()?;
                    Some((f, mtime))
                })
                .collect();
            // sort by mtime _descending_ (oldest subdirs are at the end)
            subdirs.sort_by(|(_, mtime_a), (_, mtime_b)| mtime_b.cmp(mtime_a));
            subdirs
        };
        if let Some(old_subdirs) = subdirs.get(self.max_size..) {
            for (f, _) in old_subdirs {
                if let Err(err) = fs::remove_dir_all(f.path()) {
                    warn!("Could not delete cached attempt {:?}: {err}", f.file_name());
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{array, thread, time::Duration};

    use super::*;

    #[test]
    fn lifecycle() {
        let tmpdir = tempfile::tempdir().expect("Could not create a temporary directory");

        let cache = AttemptsCache::new(Some(tmpdir.path()), Some(2))
            .expect("Could not create attempts cache");

        let attempts_dir = tmpdir.path();

        let attempt = cache
            .new_attempt(
                Path::new("foo/bar/MyFile.java"),
                "hello base",
                "hello left",
                "bye right",
            )
            .expect("Failed to create attempt in cache");
        assert_eq!(attempt.file_name, "MyFile.java");
        assert_eq!(attempt.extension, "java");

        let attempt_dir = attempts_dir.join(attempt.id());
        assert_eq!(
            fs::read_to_string(attempt_dir.join("Base.java"))
                .expect("Could not read Base.java file from attempt dir"),
            "hello base"
        );

        let attempt_id = attempt.id();
        let parsed_attempt = cache
            .parse_attempt_id(&attempt_id)
            .expect("Could not parse attempt id");

        assert_eq!(attempt, parsed_attempt);

        attempt
            .write_best_merge_id("some_merge_method")
            .expect("Failed to write best merge id in attempt");
        assert!(attempt.dir.join(BEST_MERGE_FILENAME).exists());
    }

    #[test]
    fn no_extension() {
        let tmpdir = tempfile::tempdir().expect("Could not create a temporary directory");

        let cache = AttemptsCache::new(Some(tmpdir.path()), Some(2))
            .expect("Could not create attempts cache");

        let attempts_dir = tmpdir.path();

        let attempt = cache
            .new_attempt(
                Path::new("foo/bar/MyFile"),
                "hello base",
                "hello left",
                "bye right",
            )
            .expect("Failed to create attempt in cache");
        assert_eq!(attempt.file_name, "MyFile");
        assert_eq!(attempt.extension, DEFAULT_FILE_EXTENSION);

        let attempt_dir = attempts_dir.join(attempt.id());
        assert_eq!(
            fs::read_to_string(attempt_dir.join("Base.txt"))
                .expect("Could not read Base.txt file from attempt dir"),
            "hello base"
        );

        let attempt_id = attempt.id();
        let parsed_attempt = cache
            .parse_attempt_id(&attempt_id)
            .expect("Could not parse attempt id");

        assert_eq!(attempt, parsed_attempt);
    }

    #[test]
    fn prune() {
        let tmpdir = tempfile::tempdir().expect("Could not create a temporary directory");

        let cache = AttemptsCache::new(Some(tmpdir.path()), Some(2))
            .expect("Could not create attempts cache");

        let attempts_dir = tmpdir.path();

        // create a few stale attempts
        let attempts: [Attempt; 4] = array::from_fn(|_| {
            // compensate for file system timestamp (im)precision
            thread::sleep(Duration::from_millis(100));
            cache
                .new_attempt(
                    Path::new("foo/bar/MyFile"),
                    "hello base",
                    "hello left",
                    "bye right",
                )
                .expect("Failed to create attempt in cache")
        });

        let remaining_files = fs::read_dir(attempts_dir)
            .expect("could not read the attempts directory")
            .flatten()
            .count();
        assert_eq!(remaining_files, 2);

        // the two old attempts were pruned
        assert!(!attempts[0].dir.exists());
        assert!(!attempts[1].dir.exists());
        // the two new ones were not
        assert!(attempts[2].dir.exists());
        assert!(attempts[3].dir.exists());
    }
}
