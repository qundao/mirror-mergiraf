use std::{borrow::Cow, time::Instant};

use log::debug;
use typed_arena::Arena;

use crate::{
    lang_profile::LangProfile,
    line_based::{with_final_newline, FULLY_STRUCTURED_METHOD, STRUCTURED_RESOLUTION_METHOD},
    merge_3dm::three_way_merge,
    parse,
    parsed_merge::ParsedMerge,
    settings::DisplaySettings,
    tree_matcher::TreeMatcher,
    MergeResult, Revision, TSParser,
};

/// Performs a fully structured merge, parsing the contents of all three revisions,
/// creating tree matchings between all pairs, and merging them.
///
/// The language to use is detected from the extension of `fname_base`.
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
    debug_dir: Option<&str>,
) -> Result<MergeResult, String> {
    let arena = Arena::new();
    let ref_arena = Arena::new();

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
        lang_profile,
    };
    let auxiliary_matcher = TreeMatcher {
        min_height: 2,
        sim_threshold: 0.6,
        max_recovery_size: 100,
        use_rted: false,
        lang_profile,
    };

    let start = Instant::now();
    let tree_base = parse(&mut parser, contents_base, lang_profile, &arena, &ref_arena)?;
    let tree_left = parse(&mut parser, contents_left, lang_profile, &arena, &ref_arena)?;
    let tree_right = parse(
        &mut parser,
        contents_right,
        lang_profile,
        &arena,
        &ref_arena,
    )?;
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
        initial_matchings.as_ref(),
        &primary_matcher,
        &auxiliary_matcher,
        debug_dir,
    );
    debug!("{result_tree}");

    let result = Cow::from(result_tree.pretty_print(&class_mapping, settings));

    Ok(MergeResult {
        contents: with_final_newline(result).into_owned(),
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
