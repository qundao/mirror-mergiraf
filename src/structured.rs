use std::{path::Path, time::Instant};

use log::debug;
use typed_arena::Arena;

use crate::{
    MergeResult, Revision, TSParser, lang_profile::LangProfile, merge_3dm::three_way_merge, parse,
    parsed_merge::ParsedMerge, settings::DisplaySettings, tree_matcher::TreeMatcher,
};

pub const STRUCTURED_RESOLUTION_METHOD: &str = "structured_resolution";
pub const FULLY_STRUCTURED_METHOD: &str = "fully_structured";

pub(crate) const ZDIFF3_DETECTED: &str =
    "Mergiraf cannot solve conflicts displayed in the zdiff style";

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
    debug_dir: Option<&Path>,
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
    };
    let auxiliary_matcher = TreeMatcher {
        min_height: 2,
        sim_threshold: 0.6,
        max_recovery_size: 100,
        use_rted: false,
    };

    let start = Instant::now();
    let tree_base = parse(&mut parser, contents_base, lang_profile, &arena, &ref_arena);
    let tree_left = parse(&mut parser, contents_left, lang_profile, &arena, &ref_arena);
    #[rustfmt::skip]
    let tree_right = parse(&mut parser, contents_right, lang_profile, &arena, &ref_arena);
    debug!("parsing all three files took {:?}", start.elapsed());

    // detect a merge in zdiff3 style
    let (tree_base, tree_left, tree_right) = match (tree_base, tree_left, tree_right) {
        // `contents_{base,left,right}` might've been reconstructed from a zdiff3-style merge.
        // zdiff3 pulls the changes common to left and right sides out of the conflict.
        // If that change was a paren/brace, this will've made the reconstructed base revision have
        // unbalanced parens/braces, and thus fail to parse, while the two other sides parse ok.
        //
        // Note: this might have false negatives, but "common changes" are braces most of
        // the time anyway
        (Err(_), Ok(_), Ok(_)) => return Err(ZDIFF3_DETECTED.into()),
        (b, l, r) => (b?, l?, r?),
    };

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
        settings,
        debug_dir,
    );
    debug!("{result_tree}");

    let merged_text = result_tree.to_merged_text(&class_mapping);

    // Check that the rendered merge is faithful to the tree
    let revisions_to_check = if merged_text.count_conflicts() == 0 {
        [Revision::Base].as_slice()
    } else {
        [Revision::Base, Revision::Left, Revision::Right].as_slice()
    };
    for revision in revisions_to_check {
        let merged_revision = merged_text.reconstruct_revision(*revision);
        let arena = Arena::new();
        let ref_arena = Arena::new();
        let tree = parse(
            &mut parser,
            &merged_revision,
            lang_profile,
            &arena,
            &ref_arena,
        )
        .map_err(|err| {
            format!(
                "merge discarded because rendered revision {revision} has a parsing error: {err}"
            )
        })?;
        if !result_tree.isomorphic_to_source(tree.root(), *revision, &class_mapping) {
            debug!(
                "discarding merge because rendered revision {revision} isn't isomorphic to the merged tree"
            );
            return Err("merge discarded after isomorphism check".to_owned());
        }
    }

    let method = if parsed_merge.is_none() {
        FULLY_STRUCTURED_METHOD
    } else {
        STRUCTURED_RESOLUTION_METHOD
    };
    Ok(merged_text.into_merge_result(settings, method))
}
