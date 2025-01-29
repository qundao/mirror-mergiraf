use crate::{parse, MergeResult, TSParser};
use diffy_imara::{Algorithm, ConflictStyle, MergeOptions};
use typed_arena::Arena;

use crate::{lang_profile::LangProfile, parsed_merge::ParsedMerge, settings::DisplaySettings};
pub const LINE_BASED_METHOD: &str = "line_based";
pub const STRUCTURED_RESOLUTION_METHOD: &str = "structured_resolution";
pub const FULLY_STRUCTURED_METHOD: &str = "fully_structured";

/// Perform a textual merge with the diff3 algorithm.
pub fn line_based_merge(
    contents_base: &str,
    contents_left: &str,
    contents_right: &str,
    settings: Option<&DisplaySettings>,
) -> MergeResult {
    let settings = if let Some(settings) = settings {
        settings
    } else {
        &DisplaySettings::default()
    };
    let merge_options = MergeOptions {
        conflict_marker_length: settings.conflict_marker_size_or_default(),
        style: if settings.diff3 {
            ConflictStyle::Diff3
        } else {
            ConflictStyle::Merge
        },
        algorithm: Algorithm::Histogram,
    };
    let merged = merge_options.merge(contents_base, contents_left, contents_right);
    let merged_contents = match merged {
        Ok(contents) | Err(contents) => contents,
    };
    let parsed_merge = ParsedMerge::parse(&merged_contents, settings)
        .expect("diffy-imara returned a merge that we cannot parse the conflicts of");
    MergeResult {
        contents: parsed_merge.render(settings),
        conflict_count: parsed_merge.conflict_count(),
        conflict_mass: parsed_merge.conflict_mass(),
        method: LINE_BASED_METHOD,
        has_additional_issues: false,
    }
}

/// Do a line-based merge. If it is conflict-free, also check if it introduced any duplicate signatures,
/// in which case this is logged as an additional issue on the merge result.
pub(crate) fn line_based_merge_with_duplicate_signature_detection(
    contents_base: &str,
    contents_left: &str,
    contents_right: &str,
    settings: &DisplaySettings,
    lang_profile: &LangProfile,
) -> MergeResult {
    let mut line_based_merge =
        line_based_merge(contents_base, contents_left, contents_right, Some(settings));

    if line_based_merge.conflict_count == 0 {
        let mut parser = TSParser::new();
        parser
            .set_language(&lang_profile.language)
            .unwrap_or_else(|_| panic!("Error loading {} grammar", lang_profile.name));
        let arena = Arena::new();
        let ref_arena = Arena::new();
        let tree_left = parse(
            &mut parser,
            &line_based_merge.contents,
            lang_profile,
            &arena,
            &ref_arena,
        );

        if let Ok(ast) = tree_left {
            if lang_profile.has_signature_conflicts(ast.root()) {
                line_based_merge.has_additional_issues = true;
            }
        }
    }
    line_based_merge
}
