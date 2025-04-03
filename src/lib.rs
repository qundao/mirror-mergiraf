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

pub mod attempts;
pub mod bug_reporter;
pub(crate) mod changeset;
pub(crate) mod class_mapping;
pub(crate) mod git;
pub mod lang_profile;
pub mod line_based;
pub(crate) mod matching;
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
pub(crate) mod structured;
pub mod supported_langs;
#[cfg(test)]
pub(crate) mod test_utils;
pub mod tree;
pub(crate) mod tree_builder;
pub(crate) mod tree_matcher;
pub(crate) mod visualizer;

use core::{cmp::Ordering, fmt::Write};
use std::{
    fs,
    path::Path,
    thread,
    time::{Duration, Instant},
};

use attempts::AttemptsCache;
use git::extract_revision_from_git;

use itertools::Itertools;
use lang_profile::LangProfile;
use line_based::{
    LINE_BASED_METHOD, line_based_merge, line_based_merge_with_duplicate_signature_detection,
};
use log::{debug, info, warn};

use merge_result::MergeResult;
use parsed_merge::{PARSED_MERGE_DIFF2_DETECTED, ParsedMerge};
use pcs::Revision;
use settings::DisplaySettings;
use structured::structured_merge;
use supported_langs::SUPPORTED_LANGUAGES;
use tree::{Ast, AstNode};
use tree_sitter::Parser as TSParser;
use typed_arena::Arena;

pub use path_buf_ext::PathBufExt;

/// Current way to disable Mergiraf
/// ## Usage
/// ```console
/// mergiraf=0 mergiraf merge foo bar baz
/// ```
pub const DISABLING_ENV_VAR: &str = "mergiraf";

pub(crate) const FROM_PARSED_ORIGINAL: &str = "from_parsed_original";

/// Helper to parse a source text with a given tree-sitter parser.
pub fn parse<'a>(
    parser: &mut TSParser,
    contents: &'a str,
    lang_profile: &LangProfile,
    arena: &'a Arena<AstNode<'a>>,
    ref_arena: &'a Arena<&'a AstNode<'a>>,
) -> Result<Ast<'a>, String> {
    let tree = parser
        .parse(contents, None)
        .expect("Parsing example source code failed");
    Ast::new(&tree, contents, lang_profile, arena, ref_arena)
}

/// Merge the files textually and then attempt to merge any conflicts
/// in a structured way (see [`structured_merge`]).
/// If there are still conflicts and a full merge is requested, a fully structured
/// merge (independently of the textual merge) is attempted
#[allow(clippy::too_many_arguments)]
pub fn line_merge_and_structured_resolution(
    contents_base: &'static str,
    contents_left: &'static str,
    contents_right: &'static str,
    fname_base: &'static Path,
    settings: DisplaySettings<'static>,
    full_merge: bool,
    attempts_cache: Option<&AttemptsCache>,
    debug_dir: Option<&'static Path>,
    timeout: Duration,
) -> MergeResult {
    let Some(lang_profile) = LangProfile::detect_from_filename(fname_base) else {
        // can't do anything fancier anyway
        debug!(
            "Could not find a supported language for {}. Falling back to a line-based merge.",
            fname_base.display()
        );
        return line_based_merge(contents_base, contents_left, contents_right, &settings);
    };

    let merges = cascading_merge(
        contents_base,
        contents_left,
        contents_right,
        lang_profile,
        settings,
        full_merge,
        debug_dir,
        timeout,
    );

    match select_best_merge(merges) {
        LineBasedAndBestAre::TheSame(merge) => merge,
        LineBasedAndBestAre::NotTheSame { line_based, best } => {
            if best.conflict_count == 0 {
                // for successful merges that aren't line-based,
                // give the opportunity to the user to review Mergiraf's work
                if let Some(cache) = attempts_cache {
                    match cache.new_attempt(
                        fname_base,
                        contents_base,
                        contents_left,
                        contents_right,
                    ) {
                        Ok(attempt) => {
                            best.store_in_attempt(&attempt);
                            line_based.store_in_attempt(&attempt);
                            best.mark_as_best_merge_in_attempt(&attempt, line_based.conflict_count);
                        }
                        Err(err) => {
                            warn!("Could not store merging attempt for later review: {err}");
                        }
                    }
                }
            }
            best
        }
    }
}

enum LineBasedAndBestAre {
    TheSame(MergeResult),
    NotTheSame {
        line_based: MergeResult,
        best: MergeResult,
    },
}

/// Takes a non-empty vector of merge results
/// Returns both the line-based and the best one
/// These may happen to coincide, so returns either one or two merges
fn select_best_merge(mut merges: Vec<MergeResult>) -> LineBasedAndBestAre {
    merges.sort_by_key(|merge| merge.conflict_mass);
    debug!("~~~ Merge statistics ~~~");
    for merge in &merges {
        debug!(
            "{}: {} conflict(s), {} mass, has_additional_issues: {}",
            merge.method, merge.conflict_count, merge.conflict_mass, merge.has_additional_issues
        );
    }

    let best_pos = merges
        .iter()
        .position(|merge| !merge.has_additional_issues)
        .unwrap_or_default();
    let line_based_pos = merges
        .iter()
        .position(|merge| merge.method == LINE_BASED_METHOD)
        .expect("No line-based merge available");

    match best_pos.cmp(&line_based_pos) {
        Ordering::Equal => {
            let best = merges.swap_remove(best_pos);
            LineBasedAndBestAre::TheSame(best)
        }
        // in the following 2 cases, we remove the merge that comes later in the list first
        // in order to avoid messing up the other one's index
        Ordering::Less => {
            let line_based = merges.swap_remove(line_based_pos);
            let best = merges.swap_remove(best_pos);
            LineBasedAndBestAre::NotTheSame { line_based, best }
        }
        Ordering::Greater => {
            let best = merges.swap_remove(best_pos);
            let line_based = merges.swap_remove(line_based_pos);
            LineBasedAndBestAre::NotTheSame { line_based, best }
        }
    }
}

/// Attempts various merging methods in turn, and stops early when
/// any of them finds a conflict-free merge without any additional issues.
#[allow(clippy::too_many_arguments)]
pub fn cascading_merge(
    contents_base: &'static str,
    contents_left: &'static str,
    contents_right: &'static str,
    lang_profile: &'static LangProfile,
    settings: DisplaySettings<'static>,
    full_merge: bool,
    debug_dir: Option<&'static Path>,
    timeout: Duration,
) -> Vec<MergeResult> {
    // first attempt: try to merge as line-based
    let start = Instant::now();
    let line_based_merge = line_based_merge_with_duplicate_signature_detection(
        contents_base,
        contents_left,
        contents_right,
        &settings,
        lang_profile,
    );
    debug!("line-based merge took {:?}", start.elapsed());
    if line_based_merge.conflict_count == 0 && !line_based_merge.has_additional_issues {
        return vec![line_based_merge];
    }

    let (tx, rx) = oneshot::channel();

    let line_based_merge_clone = line_based_merge.clone();

    thread::spawn(move || {
        let mut merges = Vec::new();

        // second attempt: to solve the conflicts from the line-based merge
        if !line_based_merge_clone.has_additional_issues {
            let parsed_conflicts = ParsedMerge::parse(&line_based_merge_clone.contents, &settings)
                .expect("the diffy-imara rust library produced inconsistent conflict markers");

            let solved_merge = resolve_merge(&parsed_conflicts, &settings, lang_profile, debug_dir);

            match solved_merge {
                Ok(recovered_merge) => {
                    if recovered_merge.conflict_count == 0 && !recovered_merge.has_additional_issues
                    {
                        let _ = tx.send(vec![recovered_merge]);
                        return;
                    }
                    merges.push(recovered_merge);
                }
                Err(err) => {
                    debug!("error while attempting conflict resolution of line-based merge: {err}");
                }
            }
        }

        if full_merge || line_based_merge_clone.has_additional_issues {
            // third attempt: full-blown structured merge
            let structured_merge = structured_merge(
                contents_base,
                contents_left,
                contents_right,
                None,
                &settings,
                lang_profile,
                debug_dir,
            );
            match structured_merge {
                Ok(successful_merge) => merges.push(successful_merge),
                Err(parse_error) => {
                    debug!("full structured merge encountered an error: {parse_error}");
                }
            };
        }
        let _ = tx.send(merges);
    });

    let mut merges = if timeout.is_zero() {
        rx.recv().unwrap()
    } else {
        match rx.recv_timeout(timeout) {
            Ok(merges) => merges,
            Err(oneshot::RecvTimeoutError::Timeout) => {
                warn!("structured merge took too long, falling back to Git");
                vec![]
            }
            Err(oneshot::RecvTimeoutError::Disconnected) => unreachable!(),
        }
    };

    merges.push(line_based_merge);
    merges
}

/// Takes a vector of merge results produced by [`resolve_merge_cascading`] and picks the best one
fn select_best_solve(mut solves: Vec<MergeResult>) -> Result<MergeResult, String> {
    if solves.is_empty() {
        return Err("Could not generate any solution".to_string());
    }

    solves.sort_by_key(|solve| solve.conflict_mass);
    debug!("~~~ Solve statistics ~~~");
    for solve in &solves {
        debug!(
            "{}: {} conflict(s), {} mass, has_additional_issues: {}",
            solve.method, solve.conflict_count, solve.conflict_mass, solve.has_additional_issues
        );
    }

    let best_solve = solves
        .into_iter()
        .find_or_first(|solve| !solve.has_additional_issues)
        .expect("checked for non-emptiness above");

    if best_solve.method == FROM_PARSED_ORIGINAL {
        // the best solve we've got is the line-based one
        Err("Could not generate any solution".to_string())
    } else {
        Ok(best_solve)
    }
}

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

/// Extracts the original revisions of the file from Git and performs a fully structured merge (see
/// [`structured_merge`])
///
/// Returns either a merge or nothing if couldn't extract the revisions.
fn structured_merge_from_git_revisions(
    fname_base: &Path,
    settings: &DisplaySettings,
    debug_dir: Option<&Path>,
    working_dir: &Path,
    lang_profile: &LangProfile,
) -> Result<MergeResult, String> {
    let revision_base = extract_revision(working_dir, fname_base, Revision::Base);
    let revision_left = extract_revision(working_dir, fname_base, Revision::Left);
    let revision_right = extract_revision(working_dir, fname_base, Revision::Right);

    // we only attempt a full structured merge if we could extract revisions from Git
    match (revision_base, revision_left, revision_right) {
        (Ok(contents_base), Ok(contents_left), Ok(contents_right)) => structured_merge(
            &contents_base,
            &contents_left,
            &contents_right,
            None,
            settings,
            lang_profile,
            debug_dir,
        ),
        (rev_base, _, _) => {
            if let Err(b) = rev_base {
                println!("{b}");
            }
            Err("Could not retrieve conflict sides from Git.".to_owned())
        }
    }
}

/// Cascading merge resolution starting from a user-supplied file with merge conflicts
pub fn resolve_merge_cascading<'a>(
    merge_contents: &'a str,
    fname_base: &Path,
    mut settings: DisplaySettings<'a>,
    debug_dir: Option<&Path>,
    working_dir: &Path,
) -> Result<MergeResult, String> {
    let mut solves = Vec::with_capacity(3);

    let lang_profile = LangProfile::detect_from_filename(fname_base).ok_or_else(|| {
        format!(
            "Could not find a supported language for {}",
            fname_base.display()
        )
    })?;

    match ParsedMerge::parse(merge_contents, &settings) {
        Err(err) => {
            if err == PARSED_MERGE_DIFF2_DETECTED {
                // if parsing the original merge failed because it's done in diff2 mode,
                // then we warn the user about it but don't give up yet as we can try a full merge
                warn!(
                    "Cannot solve conflicts in diff2 style. Merging the original conflict sides from scratch instead."
                );
            } else {
                warn!(
                    "Error while parsing conflicts: {err}. Merging the original conflict sides from scratch instead."
                );
            }
        }
        Ok(parsed_merge) => {
            settings.add_revision_names(&parsed_merge);

            match resolve_merge(&parsed_merge, &settings, lang_profile, debug_dir) {
                Ok(solve) if solve.conflict_count == 0 => {
                    info!("Solved all conflicts.");
                    return Ok(solve);
                }
                Ok(solve) => solves.push(solve),
                Err(err) => warn!("Error while resolving conflicts: {err}"),
            }

            let rendered_from_parsed = MergeResult {
                contents: parsed_merge.render(&settings),
                conflict_count: parsed_merge.conflict_count(),
                conflict_mass: parsed_merge.conflict_mass(),
                method: FROM_PARSED_ORIGINAL,
                has_additional_issues: false,
            };
            solves.push(rendered_from_parsed);
        }
    }

    // if we didn't manage to solve all conflicts, try again by extracting the original revisions from Git
    match structured_merge_from_git_revisions(
        fname_base,
        &settings,
        debug_dir,
        working_dir,
        lang_profile,
    ) {
        Ok(structured_merge) => solves.push(structured_merge),
        Err(err) => warn!("Full structured merge failed: {err}"),
    }
    let best_solve = select_best_solve(solves)?;

    match best_solve.conflict_count {
        0 => info!("Solved all conflicts."),
        n => info!("{n} conflict(s) remaining."),
    }
    Ok(best_solve)
}

fn extract_revision(working_dir: &Path, path: &Path, revision: Revision) -> Result<String, String> {
    let temp_file = extract_revision_from_git(working_dir, path, revision)?;
    let contents = fs::read_to_string(temp_file.path()).map_err(|err| err.to_string())?;
    Ok(contents)
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
                "{} ({})",
                lang_profile.name,
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
    use super::*;

    use std::collections::HashSet;

    #[test]
    fn languages_gitattributes() {
        let supported_langs = languages(true);
        // put both into sets to ignore ordering
        let supported_langs: HashSet<_> = supported_langs.lines().collect();
        let expected: HashSet<_> = include_str!("../doc/src/supported_langs.txt")
            .lines()
            .collect();
        assert_eq!(
            supported_langs,
            expected,
            "\
You were probably adding a language to Mergiraf (thanks!), but forgot to update the documentation.
Please update `doc/src/languages.md` and `doc/src/supported_langs.txt`.
The following extensions are missing from the documentation: {:?}",
            supported_langs.difference(&expected)
        );
    }
}
