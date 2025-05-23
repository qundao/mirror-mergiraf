//! Syntax aware merging of diverging files
//!
//! ## Overview
//!
//! Mergiraf is a structured merge tool. It takes three versions of a file (base, left and right)
//! and produces a fourth version where the changes from base to left and from base to right are
//! added. It does so with awareness of the syntax of the files, unlike Git's built-in line-based
//! merge algorithm.
//!
//! It is primarily designed to be used as a CLI which implements Git merge driver.
//! This means that it can replace Git's default merge algorithm when merging or rebasing branches.
//!
//! ## Using as a library to build other programs
//!
//! Mergiraf is not designed to be used as a library so far, the Rust API is therefore not meant
//! to be stable.

pub mod ast;
pub mod attempts;
pub mod bug_reporter;
pub(crate) mod changeset;
pub(crate) mod class_mapping;
pub(crate) mod git;
pub mod lang_profile;
pub mod line_based;
pub(crate) mod matching;
mod merge;
pub(crate) mod merge_3dm;
pub(crate) mod merge_postprocessor;
pub(crate) mod merge_result;
pub(crate) mod merged_text;
pub(crate) mod merged_tree;
pub(crate) mod multimap;
pub mod newline;
pub(crate) mod parsed_merge;
mod path_buf_ext;
pub(crate) mod pcs;
pub(crate) mod priority_list;
pub mod settings;
pub(crate) mod signature;
mod solve;
pub(crate) mod structured;
pub mod supported_langs;
#[cfg(test)]
pub(crate) mod test_utils;
pub(crate) mod tree_builder;
pub(crate) mod tree_matcher;
pub(crate) mod visualizer;

use core::fmt::Write;
use std::{path::Path, time::Instant};

use itertools::Itertools;
use lang_profile::LangProfile;
use log::debug;

use merge_result::MergeResult;
use parsed_merge::{PARSED_MERGE_DIFF2_DETECTED, ParsedMerge};
use pcs::Revision;
use settings::DisplaySettings;
use structured::structured_merge;
use supported_langs::SUPPORTED_LANGUAGES;

pub use path_buf_ext::PathBufExt;

/// Current way to disable Mergiraf
/// ## Usage
/// ```console
/// mergiraf=0 mergiraf merge foo bar baz
/// ```
pub const DISABLING_ENV_VAR: &str = "mergiraf";

pub use merge::line_merge_and_structured_resolution;
pub use solve::resolve_merge_cascading;

/// Takes the result of an earlier merge process (likely line-based)
/// and attempts to resolve the remaining conflicts using structured merge
/// on the enclosing AST nodes.
///
/// Returns either a merge (potentially with conflicts) or an error.
fn resolve_merge<'a>(
    parsed_merge: &ParsedMerge<'a>,
    settings: &DisplaySettings<'a>,
    lang_profile: &LangProfile,
    debug_dir: Option<&Path>,
) -> Result<MergeResult, String> {
    let start = Instant::now();

    let base_rev = parsed_merge.reconstruct_revision(Revision::Base);
    let left_rev = parsed_merge.reconstruct_revision(Revision::Left);
    let right_rev = parsed_merge.reconstruct_revision(Revision::Right);

    debug!(
        "re-constructing revisions from parsed merge took {:?}",
        start.elapsed()
    );

    structured_merge(
        &base_rev,
        &left_rev,
        &right_rev,
        Some(parsed_merge),
        settings,
        lang_profile,
        debug_dir,
    )
}

fn fxhasher() -> rustc_hash::FxHasher {
    use std::hash::BuildHasher;
    rustc_hash::FxBuildHasher.build_hasher()
}

/// The implementation of `mergiraf languages`.
///
/// Prints the list of supported languages,
/// either in the format understood by `.gitattributes`,
/// or in a more human-readable format.
pub fn languages(gitattributes: bool) -> String {
    let mut res = String::new();
    for lang_profile in &*SUPPORTED_LANGUAGES {
        if gitattributes {
            for extension in &lang_profile.extensions {
                let _ = writeln!(res, "*.{extension} merge=mergiraf");
            }
        } else {
            let _ = writeln!(
                res,
                "{lang_profile} ({})",
                lang_profile
                    .extensions
                    .iter()
                    .format_with(", ", |ext, f| f(&format_args!("*.{ext}")))
            );
        }
    }
    res
}

#[cfg(test)]
mod test {
    use crate::structured::ZDIFF3_DETECTED;

    use super::*;

    #[test]
    fn zdiff() {
        let contents = "\
<<<<<<< LEFT
    if foo {
        left()
||||||| BASE
=======
    if bar {
        right()
>>>>>>> RIGHT
    }
";
        let settings = DisplaySettings::default();
        let parsed = ParsedMerge::parse(contents, &settings).unwrap();
        let result = resolve_merge(&parsed, &settings, LangProfile::rust(), None);
        assert_eq!(result, Err(ZDIFF3_DETECTED.to_string()));
    }
}
