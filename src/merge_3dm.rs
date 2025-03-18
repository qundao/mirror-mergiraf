use std::{path::Path, thread, time::Instant};

use log::debug;

use crate::{
    changeset::ChangeSet,
    class_mapping::{ClassMapping, RevNode},
    line_based::line_based_merge,
    matching::Matching,
    merged_tree::MergedTree,
    pcs::Revision,
    settings::DisplaySettings,
    tree::Ast,
    tree_builder::TreeBuilder,
    tree_matcher::{DetailedMatching, TreeMatcher},
    visualizer::write_matching_to_dotty_file,
};

/// Backbone of the 3DM merge algorithm.
///
/// This:
/// * generates [`Matching`]s between all three pairs of revisions,
/// * creates a [`ClassMapping`] to cluster nodes together,
/// * converts the trees to [`ChangeSet`]s
/// * cleans up the union of the changesets
/// * converts back the union of changesets to a [`MergedTree`]
/// * finds and removes duplicated signatures
///
/// A good overview of this algorithm can be found in
/// [Spork: Structured Merge for Java with Formatting Preservation](https://arxiv.org/abs/2202.05329)
/// by Simon Larsén, Jean-Rémy Falleri, Benoit Baudry and Martin Monperrus
#[allow(clippy::too_many_arguments)]
pub fn three_way_merge<'a>(
    base: &'a Ast<'a>,
    left: &'a Ast<'a>,
    right: &'a Ast<'a>,
    initial_matchings: Option<&(Matching<'a>, Matching<'a>)>,
    primary_matcher: &TreeMatcher,
    auxiliary_matcher: &TreeMatcher,
    settings: &DisplaySettings<'a>,
    debug_dir: Option<&Path>,
) -> (MergedTree<'a>, ClassMapping<'a>) {
    // match all pairs of revisions
    let (base_left_matching, base_right_matching, left_right_matching) = generate_matchings(
        base,
        left,
        right,
        initial_matchings,
        primary_matcher,
        auxiliary_matcher,
        debug_dir,
    );

    // create a classmapping
    let class_mapping = create_class_mapping(
        &base_left_matching,
        &base_right_matching,
        &left_right_matching,
    );

    // convert all the trees to PCS triples
    let (changeset, base_changeset) =
        generate_pcs_triples(base, left, right, &class_mapping, debug_dir);

    // try to fix all inconsistencies in the merged changeset
    let cleaned_changeset = fix_pcs_inconsistencies(&changeset, debug_dir);

    // construct the merged tree!
    let merged_tree = build_tree(
        base,
        left,
        right,
        primary_matcher,
        &class_mapping,
        &base_changeset,
        &cleaned_changeset,
        settings,
    );

    // post-process to highlight signature conflicts
    let postprocessed_tree = postprocess_tree(merged_tree, primary_matcher, &class_mapping);

    (postprocessed_tree, class_mapping)
}

fn generate_matchings<'a>(
    base: &'a Ast<'a>,
    left: &'a Ast<'a>,
    right: &'a Ast<'a>,
    initial_matchings: Option<&(Matching<'a>, Matching<'a>)>,
    primary_matcher: &TreeMatcher,
    auxiliary_matcher: &TreeMatcher,
    debug_dir: Option<&Path>,
) -> (
    DetailedMatching<'a>,
    DetailedMatching<'a>,
    DetailedMatching<'a>,
) {
    let start = Instant::now();
    let (base_left_matching, base_right_matching) = thread::scope(|scope| {
        let base_left = scope.spawn(|| {
            debug!("matching base to left");
            primary_matcher.match_trees(
                base,
                left,
                initial_matchings.as_ref().map(|(left, _)| left),
            )
        });
        let base_right = scope.spawn(|| {
            debug!("matching base to right");
            primary_matcher.match_trees(
                base,
                right,
                initial_matchings.as_ref().map(|(_, right)| right),
            )
        });
        (
            base_left
                .join()
                .expect("error in thread matching base and left revisions"),
            base_right
                .join()
                .expect("error in thread matching base and right revisions"),
        )
    });
    debug!("matching left to right");
    let composed_matching = Matching::compose_base_left_and_base_right(
        &base_left_matching.full,
        &base_right_matching.full,
    );
    let left_right_matching = auxiliary_matcher.match_trees(left, right, Some(&composed_matching));
    debug!("matching all three pairs took {:?}", start.elapsed());

    // save the matchings for debugging purposes
    if let Some(debug_dir) = debug_dir {
        thread::scope(|s| {
            s.spawn(|| {
                write_matching_to_dotty_file(
                    debug_dir.join("base_left.dot"),
                    base,
                    left,
                    &base_left_matching,
                );
            });
            s.spawn(|| {
                write_matching_to_dotty_file(
                    debug_dir.join("base_right.dot"),
                    base,
                    right,
                    &base_right_matching,
                );
            });
            s.spawn(|| {
                write_matching_to_dotty_file(
                    debug_dir.join("left_right.dot"),
                    left,
                    right,
                    &left_right_matching,
                );
            });
        });
    }

    (base_left_matching, base_right_matching, left_right_matching)
}

fn create_class_mapping<'a>(
    base_left_matching: &DetailedMatching<'a>,
    base_right_matching: &DetailedMatching<'a>,
    left_right_matching: &DetailedMatching<'a>,
) -> ClassMapping<'a> {
    let start = Instant::now();
    let mut class_mapping = ClassMapping::new();
    class_mapping.add_matching(
        &base_left_matching.exact,
        Revision::Base,
        Revision::Left,
        true,
    );
    class_mapping.add_matching(
        &base_right_matching.exact,
        Revision::Base,
        Revision::Right,
        true,
    );
    class_mapping.add_matching(
        &left_right_matching.exact,
        Revision::Left,
        Revision::Right,
        true,
    );

    class_mapping.add_matching(
        &base_left_matching.full,
        Revision::Base,
        Revision::Left,
        false,
    );
    class_mapping.add_matching(
        &base_right_matching.full,
        Revision::Base,
        Revision::Right,
        false,
    );
    class_mapping.add_matching(
        &left_right_matching.full,
        Revision::Left,
        Revision::Right,
        false,
    );
    debug!("constructing the classmapping took {:?}", start.elapsed());
    class_mapping
}

fn generate_pcs_triples<'a>(
    base: &'a Ast<'a>,
    left: &'a Ast<'a>,
    right: &'a Ast<'a>,
    class_mapping: &ClassMapping<'a>,
    debug_dir: Option<&Path>,
) -> (ChangeSet<'a>, ChangeSet<'a>) {
    let start: Instant = Instant::now();
    debug!("generating PCS triples");
    let mut changeset = ChangeSet::new();
    changeset.add_tree(base, Revision::Base, class_mapping);
    changeset.add_tree(left, Revision::Left, class_mapping);
    changeset.add_tree(right, Revision::Right, class_mapping);

    if let Some(debug_dir) = debug_dir {
        changeset.save(debug_dir.join("changeset.txt"));
    }

    // also generate a base changeset
    let mut base_changeset = ChangeSet::new();
    base_changeset.add_tree(base, Revision::Base, class_mapping);

    if let Some(debug_dir) = debug_dir {
        base_changeset.save(debug_dir.join("base_changeset.txt"));
    }
    debug!("generating PCS triples took {:?}", start.elapsed());

    (changeset, base_changeset)
}

fn fix_pcs_inconsistencies<'a>(
    changeset: &ChangeSet<'a>,
    debug_dir: Option<&Path>,
) -> ChangeSet<'a> {
    let start: Instant = Instant::now();
    let mut cleaned_changeset = ChangeSet::new();
    debug!("number of triples: {}", changeset.len());
    for pcs in changeset.iter() {
        let mut conflict_found = false;
        if pcs.revision == Revision::Base {
            let mut conflicting_triples = changeset.inconsistent_triples(pcs);
            let count = changeset.inconsistent_triples(pcs).count();
            if count > 0 {
                debug!("number of conflicting triples: {count}");
            }
            if let Some(triple) =
                conflicting_triples.find(|triple| triple.revision != Revision::Base)
            {
                debug!("eliminating {pcs} by {triple}");
                conflict_found = true;
            }
        }
        if !conflict_found {
            cleaned_changeset.add(*pcs);
        }
    }
    debug!("cleaning up PCS triples took {:?}", start.elapsed());

    if let Some(debug_dir) = debug_dir {
        cleaned_changeset.save(debug_dir.join("cleaned.txt"));
    }

    cleaned_changeset
}

#[allow(clippy::too_many_arguments)]
fn build_tree<'a>(
    base: &Ast<'a>,
    left: &Ast<'a>,
    right: &Ast<'a>,
    primary_matcher: &TreeMatcher<'_>,
    class_mapping: &ClassMapping<'a>,
    base_changeset: &ChangeSet<'a>,
    cleaned_changeset: &ChangeSet<'a>,
    settings: &DisplaySettings<'a>,
) -> MergedTree<'a> {
    let start: Instant = Instant::now();
    let tree_builder = TreeBuilder::new(
        cleaned_changeset,
        base_changeset,
        class_mapping,
        primary_matcher.lang_profile,
        settings,
    );
    let merged_tree = tree_builder.build_tree().unwrap_or_else(|_| {
        let line_based = line_based_merge(base.source(), left.source(), right.source(), settings);
        MergedTree::LineBasedMerge {
            node: class_mapping.map_to_leader(RevNode::new(Revision::Base, base.root())),
            contents: line_based.contents,
            conflict_mass: line_based.conflict_mass,
        }
    });
    debug!("constructing the merged tree took {:?}", start.elapsed());

    merged_tree
}

fn postprocess_tree<'a>(
    merged_tree: MergedTree<'a>,
    primary_matcher: &TreeMatcher<'_>,
    class_mapping: &ClassMapping<'a>,
) -> MergedTree<'a> {
    let start: Instant = Instant::now();
    let postprocessed_tree = merged_tree
        .post_process_for_duplicate_signatures(primary_matcher.lang_profile, class_mapping);
    debug!(
        "post-processing the merged tree for signature conflicts took {:?}",
        start.elapsed()
    );

    postprocessed_tree
}

#[cfg(test)]
mod tests {
    use crate::{lang_profile::LangProfile, settings::DisplaySettings, test_utils::ctx};

    use super::*;

    fn json_matchers() -> (TreeMatcher<'static>, TreeMatcher<'static>) {
        let lang_profile = LangProfile::json();
        let primary_matcher = TreeMatcher {
            min_height: 0,
            sim_threshold: 0.5,
            max_recovery_size: 100,
            use_rted: true,
            lang_profile,
        };
        let auxiliary_matcher = TreeMatcher {
            min_height: 1,
            sim_threshold: 0.5,
            max_recovery_size: 100,
            use_rted: false,
            lang_profile,
        };
        (primary_matcher, auxiliary_matcher)
    }

    #[test]
    fn single_tree_has_no_conflicts() {
        let ctx = ctx();

        let base = ctx.parse_json("[1, {\"a\":2}]");
        let left = ctx.parse_json("[0, 1, {\"a\":2}]");
        let right = ctx.parse_json("[1, {\"a\":2}, 3]");

        let (primary_matcher, auxiliary_matcher) = json_matchers();

        let settings = DisplaySettings::default();

        let (merged_tree, classmapping) = three_way_merge(
            &base,
            &left,
            &right,
            None,
            &primary_matcher,
            &auxiliary_matcher,
            &settings,
            None,
        );

        debug!("{merged_tree}");
        let pretty_printed = merged_tree.pretty_print(&classmapping, &settings);
        assert_eq!(pretty_printed, "[0, 1, {\"a\":2}, 3]");
    }

    #[test]
    fn merge_conflict() {
        let ctx = ctx();

        let base = ctx.parse_json("[1, 2]");
        let left = ctx.parse_json("[1, 3, 2]");
        let right = ctx.parse_json("[1, 4, 2]");

        let (primary_matcher, auxiliary_matcher) = json_matchers();

        let settings = DisplaySettings::default_compact();

        let (merged_tree, class_mapping) = three_way_merge(
            &base,
            &left,
            &right,
            None,
            &primary_matcher,
            &auxiliary_matcher,
            &settings,
            None,
        );

        let pretty_printed = merged_tree.pretty_print(&class_mapping, &settings);
        assert_eq!(
            pretty_printed,
            "\
[1
<<<<<<< LEFT
, 3
||||||| BASE
=======
, 4
>>>>>>> RIGHT
, 2]"
        );
    }

    #[test]
    fn delete_delete() {
        let ctx = ctx();

        let base = ctx.parse_json("[1, 2]");
        let left = ctx.parse_json("[1]");
        let right = ctx.parse_json("[2]");

        let (primary_matcher, auxiliary_matcher) = json_matchers();

        let settings = DisplaySettings::default_compact();

        let (result_tree, class_mapping) = three_way_merge(
            &base,
            &left,
            &right,
            None,
            &primary_matcher,
            &auxiliary_matcher,
            &settings,
            None,
        );

        let pretty_printed = result_tree.pretty_print(&class_mapping, &settings);
        assert_eq!(
            pretty_printed,
            "\
<<<<<<< LEFT
[1]
||||||| BASE
[1, 2]
=======
[2]
>>>>>>> RIGHT
"
        );
    }

    #[test]
    fn delete_insert() {
        let ctx = ctx();

        let base = ctx.parse_json("[1, 2]");
        let left = ctx.parse_json("[1]");
        let right = ctx.parse_json("[1, 2, 3]");

        let (primary_matcher, auxiliary_matcher) = json_matchers();

        let settings = DisplaySettings::default_compact();

        let (merged_tree, class_mapping) = three_way_merge(
            &base,
            &left,
            &right,
            None,
            &primary_matcher,
            &auxiliary_matcher,
            &settings,
            None,
        );

        let pretty_printed = merged_tree.pretty_print(&class_mapping, &settings);
        assert_eq!(
            pretty_printed,
            "\
<<<<<<< LEFT
[1]
||||||| BASE
[1, 2]
=======
[1, 2, 3]
>>>>>>> RIGHT
"
        );
    }

    #[test]
    fn delete_modify() {
        let ctx = ctx();

        let base = ctx.parse_json("[1, {\"a\": 3}, 2]");
        let left = ctx.parse_json("[1, {\"a\": 4}, 2]");
        let right = ctx.parse_json("[1, 2]");

        let (primary_matcher, auxiliary_matcher) = json_matchers();

        let settings = DisplaySettings::default();

        let (merged_tree, class_mapping) = three_way_merge(
            &base,
            &left,
            &right,
            None,
            &primary_matcher,
            &auxiliary_matcher,
            &settings,
            None,
        );

        let pretty_printed = merged_tree.pretty_print(&class_mapping, &settings);
        assert_eq!(
            pretty_printed,
            "\
<<<<<<< LEFT
[1, {\"a\": 4}, 2]
||||||| BASE
[1, {\"a\": 3}, 2]
=======
[1, 2]
>>>>>>> RIGHT
"
        );
    }

    #[test]
    fn commutative_conflict_end_separator() {
        let ctx = ctx();

        let base = ctx.parse_json("{\"x\": 0}");
        let left = ctx.parse_json("{\"a\": 1, \"x\": 0}");
        let right = ctx.parse_json("{\"b\": 2, \"x\": 0}");

        let (primary_matcher, auxiliary_matcher) = json_matchers();

        let settings = DisplaySettings::default();

        let (merged_tree, class_mapping) = three_way_merge(
            &base,
            &left,
            &right,
            None,
            &primary_matcher,
            &auxiliary_matcher,
            &settings,
            None,
        );

        let pretty_printed = merged_tree.pretty_print(&class_mapping, &settings);
        assert_eq!(pretty_printed, "{\"a\": 1, \"b\": 2, \"x\": 0}");
    }

    #[test]
    fn commutative_conflict_no_end_separator() {
        let ctx = ctx();

        let base = ctx.parse_json("{}");
        let left = ctx.parse_json("{\"a\": 1}");
        let right = ctx.parse_json("{\"b\": 2}");

        let (primary_matcher, auxiliary_matcher) = json_matchers();

        let settings = DisplaySettings::default();

        let (merged_tree, class_mapping) = three_way_merge(
            &base,
            &left,
            &right,
            None,
            &primary_matcher,
            &auxiliary_matcher,
            &settings,
            None,
        );

        let pretty_printed = merged_tree.pretty_print(&class_mapping, &settings);
        assert_eq!(pretty_printed, "{\"a\": 1, \"b\": 2}");
    }

    #[test]
    fn commutative_conflict_double_delete() {
        let ctx = ctx();

        let base = ctx.parse_json("{\"a\": 1, \"b\": 2}");
        let left = ctx.parse_json("{\"a\": 1}");
        let right = ctx.parse_json("{\"b\": 2}");

        let (primary_matcher, auxiliary_matcher) = json_matchers();

        let settings = DisplaySettings::default();

        let (merged_tree, class_mapping) = three_way_merge(
            &base,
            &left,
            &right,
            None,
            &primary_matcher,
            &auxiliary_matcher,
            &settings,
            None,
        );

        let pretty_printed = merged_tree.pretty_print(&class_mapping, &settings);
        assert_eq!(pretty_printed, "{}");
    }

    #[test]
    fn commutative_conflict_delete_modified() {
        let ctx = ctx();

        let base = ctx.parse_json("{\"a\": {\"x\": 1}, \"b\": 2}");
        let left = ctx.parse_json("{\"a\": {\"x\": 2}}");
        let right = ctx.parse_json("{\"b\": 2}");

        let (primary_matcher, auxiliary_matcher) = json_matchers();

        let settings = DisplaySettings::default();

        let (merged_tree, class_mapping) = three_way_merge(
            &base,
            &left,
            &right,
            None,
            &primary_matcher,
            &auxiliary_matcher,
            &settings,
            None,
        );

        let _pretty_printed = merged_tree.pretty_print(&class_mapping, &settings);
        // assert_eq!(pretty_printed, "{}"); // TODO there should be a delete/modify conflict here!
    }

    fn rust_matchers() -> (TreeMatcher<'static>, TreeMatcher<'static>) {
        let lang_profile = LangProfile::rust();
        let primary_matcher = TreeMatcher {
            min_height: 0,
            sim_threshold: 0.5,
            max_recovery_size: 100,
            use_rted: true,
            lang_profile,
        };
        let auxiliary_matcher = TreeMatcher {
            min_height: 1,
            sim_threshold: 0.5,
            max_recovery_size: 100,
            use_rted: false,
            lang_profile,
        };
        (primary_matcher, auxiliary_matcher)
    }

    #[test]
    fn insert_insert_not_really_a_conflict() {
        let ctx = ctx();

        // both `left` and `right` add the `'s` to `&self`, so this would-be-conflict should be
        // resolved during the construction of the tree. NB: The `<'s>` is added just so that
        // `left` and `right` are not completely identical (which would've made the resolution trivial)
        let base = ctx.parse_rust("fn foo(&self) {}");
        let left = ctx.parse_rust("fn foo(&'s self) {}");
        let right = ctx.parse_rust("fn foo<'s>(&'s self) {}");

        let (primary_matcher, auxiliary_matcher) = rust_matchers();

        let settings = DisplaySettings::default();

        let (merged_tree, class_mapping) = three_way_merge(
            &base,
            &left,
            &right,
            None,
            &primary_matcher,
            &auxiliary_matcher,
            &settings,
            None,
        );

        let pretty_printed = merged_tree.pretty_print(&class_mapping, &settings);
        assert_eq!(pretty_printed, "fn foo<'s>(&'s self) {}");
    }

    #[test]
    /// The following (admittedly very bizarre-looking) inputs guarantee a line-based fallback on a
    /// node during merge. We then check whether the resulting line-based merge has the correct
    /// conflict marker size
    fn line_based_local_fallback_for_revnode_respects_conflict_marker_size() {
        let ctx = ctx();

        let base = "\
fn foo() {
    let start = Instant::now();
    let start;
    eprintln!();
}";

        let left = "\
fn foo() {
    let bar;
    let baz = baz();
}
fn baz() {
    let start;
    eprintln!();
}";

        let right = "\
fn foo() {
    let bar;
    let start;
    eprintln!();
}";

        let expected = "\
fn foo() {
    let bar;
    let baz = baz();
}
fn baz() {
<<<<<<<<< LEFT
||||||||| BASE
    let start = Instant::now();
=========
    let bar;
>>>>>>>>> RIGHT
    let start;
    eprintln!();
}";

        let base = ctx.parse_rust(base);
        let left = ctx.parse_rust(left);
        let right = ctx.parse_rust(right);

        let (primary_matcher, auxiliary_matcher) = rust_matchers();

        let settings = DisplaySettings {
            conflict_marker_size: Some(9),
            ..DisplaySettings::default_compact()
        };

        let (merged_tree, class_mapping) = three_way_merge(
            &base,
            &left,
            &right,
            None,
            &primary_matcher,
            &auxiliary_matcher,
            &settings,
            None,
        );

        /// Whether line-based fallback was performed on any node in this tree
        fn contains_line_based_merge(tree: &MergedTree) -> bool {
            match tree {
                MergedTree::LineBasedMerge { .. } => true,
                MergedTree::MixedTree { children, .. } => {
                    children.iter().any(contains_line_based_merge)
                }
                _ => false,
            }
        }

        assert!(contains_line_based_merge(&merged_tree));

        let pretty_printed = merged_tree.pretty_print(&class_mapping, &settings);
        assert_eq!(pretty_printed, expected);
    }
}
