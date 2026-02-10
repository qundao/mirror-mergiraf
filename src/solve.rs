//! Implementation of `mergiraf solve`

use std::{borrow::Cow, fs, path::Path};

use itertools::Itertools;
use log::{debug, info, warn};

use crate::{
    DisplaySettings, LangProfile, MergeResult, PARSED_MERGE_DIFF2_DETECTED, ParsedMerge,
    git::{
        GitTempFile, GitTempFiles, attr::GitAttrsForSolve, extract_all_revisions_from_git,
        read_content_from_commits,
    },
    newline::{imitate_newline_style, infer_newline_style, normalize_to_lf},
    resolve_merge, structured_merge,
};

const FROM_PARSED_ORIGINAL: &str = "from_parsed_original";

/// Some options can be both:
/// - provided to `mergiraf solve` on the CLI
/// - specified using Git attributes
///
/// This struct stores the former values
#[derive(Default)]
pub struct CliOpts<'a> {
    pub compact: Option<bool>,
    pub conflict_marker_size: Option<usize>,
    pub language: Option<&'a str>,
    pub allow_parse_errors: Option<bool>,
}

/// Cascading merge resolution starting from a user-supplied file with merge conflicts
pub fn solve(
    conflict_path: &Path,
    original_conflict_contents: &str,
    cli_opts: CliOpts,
    working_dir: &Path,
    debug_dir: Option<&Path>,
) -> Result<MergeResult, String> {
    let original_newline_style = infer_newline_style(original_conflict_contents);
    let conflict_contents = normalize_to_lf(original_conflict_contents);

    let (settings, lang_profile) = create_settings(conflict_path, cli_opts, working_dir)?;
    let mut merged = do_solve(
        &conflict_contents,
        conflict_path,
        settings,
        &lang_profile,
        working_dir,
        debug_dir,
    )?;
    merged.contents = imitate_newline_style(&merged.contents, original_newline_style);
    Ok(merged)
}

/// Combine the options provided on the CLI with those extracted from `.gitattributes`
/// to create [DisplaySettings] and [LangProfile] to be used during the solve
fn create_settings(
    conflict_path: &Path,
    cli_opts: CliOpts,
    working_dir: &Path,
) -> Result<(DisplaySettings<'static>, Cow<'static, LangProfile>), String> {
    let (conflict_marker_size_git, allow_parse_errors_git, language_git) =
        if let Some(git_attrs) = GitAttrsForSolve::new(working_dir, conflict_path) {
            (
                git_attrs.conflict_marker_size,
                git_attrs.allow_parse_errors,
                git_attrs.language,
            )
        } else {
            (None, None, None)
        };

    #[rustfmt::skip]
    let conflict_marker_size = cli_opts.conflict_marker_size.or(conflict_marker_size_git);
    let allow_parse_errors = cli_opts.allow_parse_errors.or(allow_parse_errors_git);

    let settings = DisplaySettings::new(
        cli_opts.compact,
        conflict_marker_size,
        // NOTE: the names will be recognized in `do_solve` (if possible)
        None,
        None,
        None,
    );

    let mut lang_profile = Cow::Borrowed(LangProfile::find(
        conflict_path,
        cli_opts.language,
        language_git.as_deref(),
    )?);
    if let Some(allow_parse_errors) = allow_parse_errors {
        lang_profile.to_mut().allow_parse_errors = allow_parse_errors;
    }

    Ok((settings, lang_profile))
}

/// The actual solving algorithm
fn do_solve<'a>(
    merge_contents: &'a str,
    fname_base: &Path,
    mut settings: DisplaySettings<'a>,
    lang_profile: &LangProfile,
    working_dir: &Path,
    debug_dir: Option<&Path>,
) -> Result<MergeResult, String> {
    let mut solves = Vec::with_capacity(4);

    let parsed = match ParsedMerge::parse(merge_contents, &settings) {
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
            None
        }
        Ok(parsed_merge) => {
            settings.add_revision_names(&parsed_merge);

            match resolve_merge(&parsed_merge, &settings, lang_profile, debug_dir) {
                Ok(solve) if solve.conflict_count == 0 => {
                    info!("Solved all conflicts.");
                    debug!("Structured merge from reconstructed revisions.");
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
            Some(parsed_merge)
        }
    };

    // if we didn't manage to solve all conflicts, try again by extracting the original revisions from Git
    match structured_merge_from_git_revisions(
        fname_base,
        &settings,
        debug_dir,
        working_dir,
        lang_profile,
    ) {
        Ok(structured_merge) if structured_merge.conflict_count == 0 => {
            info!("Solved all conflicts.");
            debug!("Structured merge from index conflict information.");
            return Ok(structured_merge);
        }
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

    // if we didn't manage to solve all conflicts, try again by extracting
    // the original revisions from Git (but differently)
    match structured_merge_from_oid(
        fname_base,
        &settings,
        debug_dir,
        working_dir,
        lang_profile,
        parsed.as_ref(),
    ) {
        Some(Ok(merge)) if merge.conflict_count == 0 => {
            info!("Solved all conflicts.");
            debug!("Structured merge from conflict OID information.");
            return Ok(merge);
        }
        Some(Ok(merge)) => solves.push(merge),
        Some(Err(err)) => warn!("OID-based structured merge failed: {err}"),
        None => (),
    }

    select_best_solve(solves)
        .inspect(|best_solve| info!("{} conflict(s) remaining.", best_solve.conflict_count))
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

/// Extracts the original revisions of the file from Git and
/// performs a fully structured merge (see [`structured_merge`])
///
/// Returns:
/// - `None` if the conflict markers do not contain OIDs
/// - `Some(Err(err))` in case of structured merge error
/// - `Some(Ok(merge))` in case of structured merge success
fn structured_merge_from_oid(
    fname_base: &Path,
    settings: &DisplaySettings,
    debug_dir: Option<&Path>,
    working_dir: &Path,
    lang_profile: &LangProfile,
    parsed: Option<&ParsedMerge<'_>>,
) -> Option<Result<MergeResult, String>> {
    let oids = parsed?.extract_conflict_oids()?;
    let contents = read_content_from_commits(working_dir, oids, fname_base)?;
    let merge = structured_merge(
        &contents.0,
        &contents.1,
        &contents.2,
        None,
        settings,
        lang_profile,
        debug_dir,
    );
    Some(merge)
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
