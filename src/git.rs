use core::str;
use itertools::Itertools as _;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

/// File produced by a call to a `git` command, which
/// we want to remove when we no longer need it.
/// This wrapper implements [Drop] to ensure that.
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

/// Versions of a file in all three revisions, extracted from git.
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
            "error while retrieving all revisions for '{}':\n{}",
            path.display(),
            error_str
        ));
    }
    let output_str = str::from_utf8(&output.stdout).map_err(|err| err.to_string())?;
    // No entries are returned for files in stage 0 (in index but not conflicted)
    if output_str.is_empty() {
        return Err(format!(
            "'{}' is not in a conflicted state.",
            path.display()
        ));
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

// FIXME: this should've been `#[cfg(test)]`, but for some reason, if I add that,
// `solve_respects_conflict_marker_size_attr` stops compiling
pub fn init(path: impl AsRef<Path>) {
    Command::new("git")
        .arg("init")
        .current_dir(path.as_ref())
        .output()
        .expect("failed to initialize a Git repository");
}

pub mod attr {
    use std::{collections::HashMap, path::Path, process::Command};

    use itertools::Itertools as _;
    use log::warn;

    pub(super) const GIT_CONFLICT_MARKER_SIZE: &str = "conflict-marker-size";
    pub(super) const LINGUIST_LANGUAGE: &str = "linguist-language";
    pub(super) const MERGIRAF_ALLOW_PARSE_ERRORS: &str = "mergiraf.allow-parse-errors";
    pub(super) const MERGIRAF_LANGUAGE: &str = "mergiraf.language";

    /// A value for a Git attribute.
    ///
    /// See <https://git-scm.com/docs/gitattributes#_description> for more information
    #[derive(Debug, PartialEq, Eq)]
    pub(super) enum AttrValue {
        /// A boolean "true"
        ///
        /// ## Example
        /// ```gitattributes
        /// * mergiraf.allow-parse-errors
        /// ```
        /// corresponds to this variant
        Set,
        /// A boolean "false"
        ///
        /// ## Example
        /// ```gitattributes
        /// * -mergiraf.allow-parse-errors
        /// ```
        /// corresponds to this variant
        Unset,
        /// `None`
        ///
        /// ## Example
        /// ```gitattributes
        /// * !conflict-marker-size
        /// ```
        /// corresponds to this variant
        Unspecified,
        /// A non-boolean value.
        ///
        /// ## Example
        /// ```gitattributes
        /// * mergiraf.language=java
        /// ```
        /// corresponds to `Self::Specified("java")`
        Specified(String),
    }

    impl AttrValue {
        /// Extract the value of the attr, if it's specified
        pub(super) fn into_specified(self) -> Option<String> {
            match self {
                Self::Specified(value) => Some(value),
                _ => None,
            }
        }
    }

    impl From<String> for AttrValue {
        fn from(value: String) -> Self {
            match &*value {
                "set" => Self::Set,
                "unset" => Self::Unset,
                "unspecified" => Self::Unspecified,
                _ => Self::Specified(value),
            }
        }
    }

    // ============================================================================================
    // `GitAttrsForMerge` and `GitAttrsForSolve` take care of calling `git check-attr` once,
    // and then parse out all the attributes they need.
    // ============================================================================================

    /// The attributes we (currently) use in `mergiraf merge`
    pub struct GitAttrsForMerge {
        pub language: Option<String>,
        pub allow_parse_errors: Option<bool>,
    }

    impl GitAttrsForMerge {
        pub fn new(repo_dir: &Path, file_name: impl AsRef<Path>) -> Option<Self> {
            let attr_names = &[
                // language
                MERGIRAF_LANGUAGE,
                LINGUIST_LANGUAGE,
                // allow_parse_errors
                MERGIRAF_ALLOW_PARSE_ERRORS,
            ];
            let mut attrs = read_attributes_for_file(repo_dir, file_name, attr_names)?;

            Some(Self {
                language: read_lang_attribute(&mut attrs),
                allow_parse_errors: read_allow_parse_errors_attribute(&mut attrs),
            })
        }
    }

    /// The attributes we (currently) use in `mergiraf solve`
    pub struct GitAttrsForSolve {
        pub conflict_marker_size: Option<usize>,
        pub language: Option<String>,
        pub allow_parse_errors: Option<bool>,
    }

    impl GitAttrsForSolve {
        pub fn new(repo_dir: &Path, file_name: impl AsRef<Path>) -> Option<Self> {
            let attr_names = &[
                // conflict_marker_size
                GIT_CONFLICT_MARKER_SIZE,
                // language
                MERGIRAF_LANGUAGE,
                LINGUIST_LANGUAGE,
                // allow_parse_errors
                MERGIRAF_ALLOW_PARSE_ERRORS,
            ];
            let mut attrs = read_attributes_for_file(repo_dir, file_name, attr_names)?;

            Some(Self {
                conflict_marker_size: read_conflict_marker_size_attribute(&mut attrs),
                language: read_lang_attribute(&mut attrs),
                allow_parse_errors: read_allow_parse_errors_attribute(&mut attrs),
            })
        }
    }

    /// Calls `git check-attr` to read the git attributes defined for a file,
    /// as represented by its path in the repository.
    ///
    /// Returns `None` if attributes could not be retrieved (e.g. due to not
    /// being in a Git repo)
    pub(super) fn read_attributes_for_file(
        repo_dir: &Path,
        file_name: impl AsRef<Path>,
        attrs: &[&'static str],
    ) -> Option<HashMap<&'static str, AttrValue>> {
        // a manually monomorphized inner function to reduce compile times
        fn inner(
            repo_dir: &Path,
            file_name: &Path,
            attrs: &[&'static str],
        ) -> Option<HashMap<&'static str, AttrValue>> {
            // We use null bytes as separators to avoid having to deal
            // with the encoding of spaces in filenames.
            if let Some(output) = Command::new("git")
                .args(["check-attr", "-z"])
                .args(attrs)
                .arg("--")
                .arg(file_name)
                .current_dir(repo_dir)
                .output()
                .ok()
                .filter(|output| output.status.success())
            {
                let mut result_map = HashMap::with_capacity(attrs.len());

                // Parse the output of git-check-attr, which looks like with the `-z` flag:
                // ( <path> NUL <attribute> NUL <info> NUL ) *
                for mut line_parts in &output.stdout.split(|b| *b == b'\0').chunks(3) {
                    // consume the first chunk, which contains the path
                    line_parts.next();
                    if let Some(attribute) = line_parts.next()
                        && let Some(info) = line_parts.next()
                        && let Ok(attribute) = str::from_utf8(attribute)
                        && let Ok(info) = String::from_utf8(info.to_vec())
                        && let Some(attribute) =
                            attrs.iter().find(|orig_attr| **orig_attr == attribute)
                    {
                        result_map.insert(*attribute, AttrValue::from(info));
                    }
                }

                return Some(result_map);
            }
            None
        }
        inner(repo_dir, file_name.as_ref(), attrs)
    }

    // ============================================================================================
    // The following functions take as an input `attrs`, which is a map of attributes that were
    // extracted all from one invocation of `git check-attr`. This is done so that the methods can
    // be composed, as in [GitAttrsForMerge::new] and [GitAttrsForSolve::new].
    // ============================================================================================

    /// Determine the language in which a file should be parsed as specified
    /// by the git attributes defined for that file.
    pub(super) fn read_lang_attribute(
        attrs: &mut HashMap<&'static str, AttrValue>,
    ) -> Option<String> {
        // The following attributes are looked up to determine the language, in this order
        // (if the first attribute is set, it overrides the second one)
        let attr_names = [MERGIRAF_LANGUAGE, LINGUIST_LANGUAGE];

        attr_names.into_iter().find_map(|attr_name| {
            let attr_value = attrs.remove(attr_name);
            debug_assert!(
                attr_value.is_some(),
                "`{attr_name}` wasn't present in `attrs={attrs:?}`"
            );
            attr_value.and_then(AttrValue::into_specified)
        })
    }

    pub(super) fn read_conflict_marker_size_attribute(
        attrs: &mut HashMap<&'static str, AttrValue>,
    ) -> Option<usize> {
        let attr_name = GIT_CONFLICT_MARKER_SIZE;

        let attr_value = attrs.remove(attr_name);
        debug_assert!(
            attr_value.is_some(),
            "`{attr_name}` wasn't present in `attrs={attrs:?}`"
        );
        attr_value
            .and_then(AttrValue::into_specified)
            .and_then(|size| match size.parse() {
                Ok(size) => Some(size),
                Err(err) => {
                    warn!(
                        "The value of the `{GIT_CONFLICT_MARKER_SIZE}` \
                        could not be parsed as a number: {err}"
                    );
                    None
                }
            })
    }

    pub(super) fn read_allow_parse_errors_attribute(
        attrs: &mut HashMap<&'static str, AttrValue>,
    ) -> Option<bool> {
        let attr_name = MERGIRAF_ALLOW_PARSE_ERRORS;

        let attr_value = attrs.remove(attr_name);
        debug_assert!(
            attr_value.is_some(),
            "`{attr_name}` wasn't present in `attrs={attrs:?}`"
        );
        attr_value.and_then(|allow| match allow {
            AttrValue::Unspecified => None,
            AttrValue::Set => Some(true),
            AttrValue::Unset => Some(false),
            AttrValue::Specified(other) => {
                warn!(
                    "invalid value for `{MERGIRAF_ALLOW_PARSE_ERRORS}` attribute: \
                    expected \"{MERGIRAF_ALLOW_PARSE_ERRORS}\" \
                    or \"-{MERGIRAF_ALLOW_PARSE_ERRORS}\", \
                    found \"{MERGIRAF_ALLOW_PARSE_ERRORS}={other}\""
                );
                None
            }
        })
    }
}

#[cfg(test)]
mod test {
    mod attr {
        use std::collections::HashMap;

        use crate::{
            git,
            git::attr::{
                AttrValue, GIT_CONFLICT_MARKER_SIZE, LINGUIST_LANGUAGE,
                MERGIRAF_ALLOW_PARSE_ERRORS, MERGIRAF_LANGUAGE,
            },
            utils::write_string_to_file,
        };

        #[test]
        fn read_attributes_for_file() {
            let repo_dir = tempfile::tempdir().expect("failed to create tempdir");
            let repo_path = repo_dir.path();

            const VALUE_ATTR: &str = "foo-value-attr";
            const SET_ATTR: &str = "foo-set-attr";
            const UNSET_ATTR: &str = "foo-unset-attr";
            const UNSPECIFIED_ATTR: &str = "foo-unspecified-attr";

            let attrs = || {
                git::attr::read_attributes_for_file(
                    repo_path,
                    "foo.txt",
                    &[VALUE_ATTR, SET_ATTR, UNSET_ATTR, UNSPECIFIED_ATTR],
                )
            };

            // repo not yet initialized, so reading attributes should fail
            pretty_assertions::assert_eq!(attrs(), None);

            git::init(repo_path);

            let expected = Some(HashMap::from([
                (VALUE_ATTR, AttrValue::Unspecified),
                (UNSET_ATTR, AttrValue::Unspecified),
                (SET_ATTR, AttrValue::Unspecified),
                (UNSPECIFIED_ATTR, AttrValue::Unspecified),
            ]));
            pretty_assertions::assert_eq!(attrs(), expected);

            write_string_to_file(
                repo_path.join(".gitattributes"),
                "\
                *       foo-value-attr=10
                bar.txt foo-value-attr=11
                foo.txt foo-set-attr
                foo.txt -foo-unset-attr",
            )
            .unwrap();

            let expected = Some(HashMap::from([
                (VALUE_ATTR, value("10")),
                (UNSET_ATTR, AttrValue::Unset),
                (SET_ATTR, AttrValue::Set),
                (UNSPECIFIED_ATTR, AttrValue::Unspecified),
            ]));
            pretty_assertions::assert_eq!(attrs(), expected);
        }

        fn value(s: &str) -> AttrValue {
            AttrValue::from(s.to_string())
        }

        #[test]
        fn read_lang_attribute() {
            let lang = |mut attrs| git::attr::read_lang_attribute(&mut attrs);

            let attrs_empty = HashMap::from([
                (MERGIRAF_LANGUAGE, AttrValue::Unspecified),
                (LINGUIST_LANGUAGE, AttrValue::Unspecified),
            ]);
            assert_eq!(lang(attrs_empty), None);

            let attrs_mergiraf = HashMap::from([
                (MERGIRAF_LANGUAGE, value("rs")),
                (LINGUIST_LANGUAGE, AttrValue::Unspecified),
            ]);
            assert_eq!(lang(attrs_mergiraf).as_deref(), Some("rs"));

            let attrs_linguist = HashMap::from([
                (MERGIRAF_LANGUAGE, AttrValue::Unspecified),
                (LINGUIST_LANGUAGE, value("c++")),
            ]);
            assert_eq!(lang(attrs_linguist).as_deref(), Some("c++"));

            // prefer the one specified by MERGIRAF_LANGUAGE
            let attrs_both = HashMap::from([
                (LINGUIST_LANGUAGE, value("c++")),
                (MERGIRAF_LANGUAGE, value("rust")),
            ]);
            assert_eq!(lang(attrs_both).as_deref(), Some("rust"));
        }

        #[test]
        fn read_conflict_marker_size_attribute() {
            let size = |mut attrs| git::attr::read_conflict_marker_size_attribute(&mut attrs);

            let attrs_empty = HashMap::from([(GIT_CONFLICT_MARKER_SIZE, AttrValue::Unspecified)]);
            assert_eq!(size(attrs_empty), None);

            let attrs_nonempty = HashMap::from([(GIT_CONFLICT_MARKER_SIZE, value("10"))]);
            assert_eq!(size(attrs_nonempty), Some(10));
        }

        #[test]
        fn read_allow_parse_errors_attribute() {
            let allow = |mut attrs| git::attr::read_allow_parse_errors_attribute(&mut attrs);

            let attrs_empty =
                HashMap::from([(MERGIRAF_ALLOW_PARSE_ERRORS, AttrValue::Unspecified)]);
            assert_eq!(allow(attrs_empty), None);

            let attrs_allow = HashMap::from([(MERGIRAF_ALLOW_PARSE_ERRORS, AttrValue::Set)]);
            assert_eq!(allow(attrs_allow), Some(true));

            let attrs_deny = HashMap::from([(MERGIRAF_ALLOW_PARSE_ERRORS, AttrValue::Unset)]);
            assert_eq!(allow(attrs_deny), Some(false));
        }
    }
}
