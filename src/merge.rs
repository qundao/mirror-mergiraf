//! Implementation of `mergiraf merge`

use std::{
    cmp::Ordering,
    path::Path,
    thread,
    time::{Duration, Instant},
};

use log::{debug, warn};

use crate::{
    DisplaySettings, LangProfile, MergeResult,
    attempts::AttemptsCache,
    line_based::{
        LINE_BASED_METHOD, line_based_merge, line_based_merge_with_duplicate_signature_detection,
    },
    resolve_merge, structured_merge,
};

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
    language: Option<&str>,
) -> MergeResult {
    let Ok(lang_profile) = LangProfile::find_by_filename_or_name(fname_base, language) else {
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
    let (parsed_conflicts, line_based_merge) = line_based_merge_with_duplicate_signature_detection(
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

    thread::spawn(move || {
        let mut merges = Vec::new();

        // second attempt: to solve the conflicts from the line-based merge
        if !line_based_merge.has_additional_issues {
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

        if full_merge || line_based_merge.has_additional_issues {
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
