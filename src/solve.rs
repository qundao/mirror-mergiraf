//! Implementation of `mergiraf solve`

use std::{fs, path::Path};

use itertools::Itertools;
use log::{debug, info, warn};

use crate::{
    DisplaySettings, LangProfile, MergeResult, PARSED_MERGE_DIFF2_DETECTED, ParsedMerge,
    git::{GitTempFile, GitTempFiles, extract_all_revisions_from_git},
    resolve_merge, structured_merge,
};

const FROM_PARSED_ORIGINAL: &str = "from_parsed_original";

/// Cascading merge resolution starting from a user-supplied file with merge conflicts
pub fn resolve_merge_cascading<'a>(
    merge_contents: &'a str,
    fname_base: &Path,
    mut settings: DisplaySettings<'a>,
    debug_dir: Option<&Path>,
    working_dir: &Path,
    language: Option<&str>,
) -> Result<MergeResult, String> {
    let mut solves = Vec::with_capacity(3);

    let lang_profile = LangProfile::find_by_filename_or_name(fname_base, language)?;

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

            let mut rendered_from_parsed = parsed_merge.into_merge_result(&settings);
            // For now, we assume that the original merge with conflicts is free of syntax errors
            // and duplicate signatures, so that it has priority over any other merge that we'd produce
            // and would be syntactically invalid.
            rendered_from_parsed.has_additional_issues = false;
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
        Err(FallbackMergeError::MergeError(err)) => warn!("Full structured merge failed: {err}"),
        Err(FallbackMergeError::GitError(err)) => {
            debug!("Error while extracting original revisions from Git: {err}");
            warn!(
                "Couldn't retrieve the original revisions from Git. This \
                limits Mergiraf's ability to solve certain types of conflicts."
            );
        }
    }
    let best_solve = select_best_solve(solves)?;

    match best_solve.conflict_count {
        0 => info!("Solved all conflicts."),
        n => info!("{n} conflict(s) remaining."),
    }
    Ok(best_solve)
}

enum FallbackMergeError {
    GitError(String),
    MergeError(String),
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
) -> Result<MergeResult, FallbackMergeError> {
    let GitTempFiles { base, left, right } =
        extract_all_revisions_from_git(working_dir, fname_base)
            .map_err(FallbackMergeError::GitError)?;

    let read_file = |file: GitTempFile| {
        fs::read_to_string(file.path()).map_err(|e| FallbackMergeError::GitError(e.to_string()))
    };

    // If the file is conflicted in the index but one revision is missing,
    // fallback on an empty string
    let contents_base = base.map(read_file).transpose()?.unwrap_or_default();
    let contents_left = left.map(read_file).transpose()?.unwrap_or_default();
    let contents_right = right.map(read_file).transpose()?.unwrap_or_default();

    // we only attempt a full structured merge if we could extract revisions from Git
    structured_merge(
        &contents_base,
        &contents_left,
        &contents_right,
        None,
        settings,
        lang_profile,
        debug_dir,
    )
    .map_err(FallbackMergeError::MergeError)
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
