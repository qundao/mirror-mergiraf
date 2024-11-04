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
pub(crate) mod line_based;
pub(crate) mod matching;
pub(crate) mod merge_3dm;
pub(crate) mod merge_postprocessor;
pub(crate) mod merged_text;
pub(crate) mod merged_tree;
#[allow(clippy::mutable_key_type)]
pub(crate) mod multimap;
pub(crate) mod parsed_merge;
pub(crate) mod pcs;
pub(crate) mod priority_list;
pub mod settings;
pub(crate) mod signature;
pub mod supported_langs;
#[cfg(test)]
pub(crate) mod test_utils;
pub mod tree;
pub(crate) mod tree_builder;
pub(crate) mod tree_matcher;
#[cfg(feature = "dotty")]
pub(crate) mod visualizer;

use std::{fs, path::PathBuf, time::Instant};

use attempts::AttemptsCache;
#[cfg(feature = "dotty")]
use graphviz_rust::printer::{DotPrinter, PrinterContext};

use lang_profile::LangProfile;
use line_based::{
    line_based_merge, with_final_newline, MergeResult, FULLY_STRUCTURED_METHOD, LINE_BASED_METHOD,
    STRUCTURED_RESOLUTION_METHOD,
};
use log::{debug, warn};
use merge_3dm::three_way_merge;

use parsed_merge::ParsedMerge;
use pcs::Revision;
use settings::DisplaySettings;
use tree::{Ast, AstNode};
use tree_matcher::{DetailedMatching, TreeMatcher};
use tree_sitter::Parser as TSParser;
use typed_arena::Arena;

#[cfg(feature = "dotty")]
use crate::visualizer::matching_to_graph;

/// Helper to parse a source text with a given tree-sitter parser.
pub(crate) fn parse<'a>(
    parser: &mut TSParser,
    contents: &'a str,
    lang_profile: &LangProfile,
    arena: &'a Arena<AstNode<'a>>,
) -> Result<Ast<'a>, String> {
    let tree = parser
        .parse(contents, None)
        .expect("Parsing example source code failed");
    Ast::new(tree, contents, lang_profile, arena)
}

/// Performs a fully structured merge, parsing the contents of all three revisions,
/// creating tree matchings between all pairs, and merging them.
///
/// The language to use is detected from the extension of fname_base.
/// If a debug dir is provided, various intermediate stages of the matching will be
/// written as files in that directory.
/// Fails if the language cannot be detected or loaded.
pub fn structured_merge(
    contents_base: &str,
    contents_left: &str,
    contents_right: &str,
    parsed_merge: Option<&ParsedMerge>,
    settings: &DisplaySettings,
    lang_profile: &LangProfile,
    debug_dir: &Option<String>,
) -> Result<MergeResult, String> {
    let arena = Arena::new();

    let start = Instant::now();
    let mut parser = TSParser::new();
    parser
        .set_language(&lang_profile.language)
        .unwrap_or_else(|_| panic!("Error loading {} grammar", lang_profile.name));
    debug!("initializing the parser took {:?}", start.elapsed());

    let primary_matcher = TreeMatcher {
        min_height: 1,
        sim_threshold: 0.4,
        max_recovery_size: 100,
        use_rted: true,
        lang_profile: lang_profile.clone(),
    };
    let auxiliary_matcher = TreeMatcher {
        min_height: 2,
        sim_threshold: 0.6,
        max_recovery_size: 100,
        use_rted: false,
        lang_profile: lang_profile.clone(),
    };

    let start = Instant::now();
    let tree_base = parse(&mut parser, &contents_base, lang_profile, &arena)?;
    let tree_left = parse(&mut parser, &contents_left, lang_profile, &arena)?;
    let tree_right = parse(&mut parser, &contents_right, lang_profile, &arena)?;
    debug!("parsing all three files took {:?}", start.elapsed());

    let initial_matchings = parsed_merge.map(|parsed_merge| {
        (
            parsed_merge
                .generate_matching(Revision::Base, Revision::Left, &tree_base, &tree_left)
                .add_submatches(),
            parsed_merge
                .generate_matching(Revision::Base, Revision::Right, &tree_base, &tree_right)
                .add_submatches(),
        )
    });

    let (result_tree, class_mapping) = three_way_merge(
        &tree_base,
        &tree_left,
        &tree_right,
        &initial_matchings,
        &primary_matcher,
        &auxiliary_matcher,
        &debug_dir,
    );
    debug!("{}", result_tree);

    Ok(MergeResult {
        contents: with_final_newline(&result_tree.pretty_print(&class_mapping, settings)),
        conflict_count: result_tree.count_conflicts(),
        conflict_mass: result_tree.conflict_mass(),
        method: if parsed_merge.is_none() {
            FULLY_STRUCTURED_METHOD
        } else {
            STRUCTURED_RESOLUTION_METHOD
        },
        has_additional_issues: false,
    })
}

/// Merge the files textually and then attempt to merge any conflicts
/// in a structured way (see [`structured_merge`]).
/// If there are still conflicts and a full merge is requested, a fully structured
/// merge (independently of the textual merge) is attempted
pub fn line_merge_and_structured_resolution(
    contents_base: &str,
    contents_left: &str,
    contents_right: &str,
    fname_base: &str,
    settings: &DisplaySettings,
    full_merge: bool,
    attempts_cache: Option<&AttemptsCache>,
    debug_dir: &Option<String>,
) -> MergeResult {
    let mut merges = cascading_merge(
        contents_base,
        contents_left,
        contents_right,
        fname_base,
        settings,
        full_merge,
        debug_dir,
    );

    merges.sort_by_key(|merge| merge.conflict_mass);
    debug!("~~~ Merge statistics ~~~");
    for merge in merges.iter() {
        debug!(
            "{}: {} conflict(s), {} mass, has_additional_issues: {}",
            merge.method, merge.conflict_count, merge.conflict_mass, merge.has_additional_issues
        );
    }

    let line_based = merges
        .iter()
        .find(|merge| merge.method == LINE_BASED_METHOD)
        .expect("No line-based merge available")
        .clone(); // TODO avoid this clone

    let mut first_merge = None;
    for best_merge in merges.into_iter() {
        if !best_merge.has_additional_issues {
            if best_merge.conflict_count == 0 && best_merge.method != LINE_BASED_METHOD {
                // for successful merges that aren't line-based,
                // give the opportunity to the user to review Mergiraf's work
                let attempt = attempts_cache.and_then(|cache| {
                    match cache.new_attempt(
                        &PathBuf::from(fname_base),
                        contents_base,
                        contents_left,
                        contents_right,
                    ) {
                        Ok(attempt) => Some(attempt),
                        Err(err) => {
                            warn!("Could not store merging attempt for later review: {err}");
                            None
                        }
                    }
                });
                best_merge.store_in_attempt(&attempt);
                line_based.store_in_attempt(&attempt);
                best_merge.mark_as_best_merge_in_attempt(&attempt, line_based.conflict_count);
            }
            return best_merge;
        } else {
            if first_merge.is_none() {
                first_merge = Some(best_merge)
            }
        }
    }

    return first_merge.expect("No merge could be computed using any method");
}

/// Do a line-based merge. If it is conflict-free, also check if it introduced any duplicate signatures,
/// in which case this is logged as an additional issue on the merge result.
fn line_based_merge_with_duplicate_signature_detection(
    contents_base: &str,
    contents_left: &str,
    contents_right: &str,
    settings: &DisplaySettings,
    lang_profile: Option<&LangProfile>,
) -> MergeResult {
    let mut line_based_merge = line_based_merge(
        &with_final_newline(&contents_base),
        &with_final_newline(&contents_left),
        &with_final_newline(&contents_right),
        &settings,
    );

    if line_based_merge.conflict_count == 0 {
        // If we support this language, check that there aren't any signature conflicts in the line-based merge
        if let Some(lang_profile) = lang_profile {
            let mut parser = TSParser::new();
            parser
                .set_language(&lang_profile.language)
                .unwrap_or_else(|_| panic!("Error loading {} grammar", lang_profile.name));
            let arena = Arena::new();
            let tree_left = parse(
                &mut parser,
                &line_based_merge.contents,
                lang_profile,
                &arena,
            );

            if let Ok(ast) = tree_left {
                if lang_profile.has_signature_conflicts(ast.root()) {
                    line_based_merge.has_additional_issues = true;
                }
            }
        }
    }
    line_based_merge
}

/// Attempts various merging method in turn, and stops early when
/// any of them finds a conflict-free merge without any additional issues.
pub fn cascading_merge(
    contents_base: &str,
    contents_left: &str,
    contents_right: &str,
    fname_base: &str,
    settings: &DisplaySettings,
    full_merge: bool,
    debug_dir: &Option<String>,
) -> Vec<MergeResult> {
    let mut merges = Vec::new();
    let lang_profile = LangProfile::detect_from_filename(&fname_base);

    // first attempt: try to merge as line-based
    let start = Instant::now();
    let line_based_merge = line_based_merge_with_duplicate_signature_detection(
        &contents_base,
        &contents_left,
        &contents_right,
        &settings,
        lang_profile.as_ref(),
    );
    merges.push(line_based_merge.clone());
    debug!("line-based merge took {:?}", start.elapsed());
    if line_based_merge.conflict_count == 0 && !line_based_merge.has_additional_issues {
        return merges;
    }

    if let Some(lang_profile) = lang_profile {
        // second attempt: to solve the conflicts from the line-based merge
        if !line_based_merge.has_additional_issues {
            let start = Instant::now();
            let parsed_conflicts = ParsedMerge::parse(&line_based_merge.contents)
                .expect("the diffy rust library produced inconsistent conflict markers");

            let base_recovered_rev = parsed_conflicts.reconstruct_revision(Revision::Base);
            let left_recovered_rev = parsed_conflicts.reconstruct_revision(Revision::Left);
            let right_recovered_rev = parsed_conflicts.reconstruct_revision(Revision::Right);
            debug!(
                "re-constructing revisions from parsed merge took {:?}",
                start.elapsed()
            );

            let solved_merge = structured_merge(
                &base_recovered_rev,
                &left_recovered_rev,
                &right_recovered_rev,
                Some(&parsed_conflicts),
                settings,
                &lang_profile,
                debug_dir,
            );

            match solved_merge {
                Ok(recovered_merge) => {
                    let conflicts = recovered_merge.conflict_count;
                    let additional_issues = recovered_merge.has_additional_issues;
                    merges.push(recovered_merge);
                    if conflicts == 0 && !additional_issues {
                        return merges;
                    }
                }
                Err(err) => {
                    debug!(
                        "error while attempting conflict resolution of line-based merge: {}",
                        err
                    );
                }
            }
        }

        if full_merge || line_based_merge.has_additional_issues {
            // third attempt: full-blown structured merge
            let structured_merge = structured_merge(
                contents_base,
                contents_left,
                contents_right,
                None,
                settings,
                &lang_profile,
                debug_dir,
            );
            match structured_merge {
                Ok(successful_merge) => merges.push(successful_merge),
                Err(parse_error) => {
                    debug!(
                        "full structured merge encountered an error: {}",
                        parse_error
                    )
                }
            };
        }
    }
    merges
}

/// Takes the result of an earlier merge process (likely line-based)
/// and attempts to resolve the remaining conflicts using structured merge
/// on the enclosing AST nodes.
///
/// Returns either a merge (potentially with conflicts) or an error.
pub fn resolve_merge(
    merge: &str,
    fname_base: &str,
    settings: &DisplaySettings,
    debug_dir: &Option<String>,
) -> Result<(ParsedMerge, MergeResult), String> {
    let parsed_merge = ParsedMerge::parse(merge)?;

    let base_rev = parsed_merge.reconstruct_revision(Revision::Base);
    let left_rev = parsed_merge.reconstruct_revision(Revision::Left);
    let right_rev = parsed_merge.reconstruct_revision(Revision::Right);

    let lang_profile = LangProfile::detect_from_filename(&fname_base).ok_or(format!(
        "Could not find a supported language for {fname_base}"
    ))?;

    let merge = structured_merge(
        &base_rev,
        &left_rev,
        &right_rev,
        Some(&parsed_merge),
        settings,
        &lang_profile,
        debug_dir,
    )?;
    Ok((parsed_merge, merge))
}

#[cfg(feature = "dotty")]
fn save_matching<'a>(
    left: &'a Ast<'a>,
    right: &'a Ast<'a>,
    matching: &DetailedMatching<'a>,
    fname: &str,
) {
    let graph = matching_to_graph(left, right, matching);

    let mut ctx = PrinterContext::default();

    let dotty = graph.print(&mut ctx);
    fs::write(fname, dotty).expect("Unable to write debug graph file")
}
