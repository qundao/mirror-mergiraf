use itertools::Itertools;
use log::debug;
use std::{
    cmp::{min, Ordering},
    collections::HashSet,
    fmt::Display,
    time::Instant,
};
use tree_edit_distance::{diff, Edit};
use typed_arena::Arena;

use crate::{
    lang_profile::LangProfile,
    matching::Matching,
    multimap::MultiMap,
    priority_list::PriorityList,
    signature::Signature,
    tree::{Ast, AstNode},
};

#[derive(Debug)]
pub struct TreeMatcher {
    /// The minimum height of subtrees to match in the top-down phase
    pub min_height: i32,
    /// The minimum dice similarity to match subtrees in the bottom-up phase
    pub sim_threshold: f32,
    /// Whether to use the tree edit distance algorithm to infer additional matches in the "last chance" pass
    pub use_rted: bool,
    /// The maximum size of trees to match with tree edit distance
    pub max_recovery_size: i32,
    /// The language-specific information, which could be used to inform the matching algorithm
    pub lang_profile: LangProfile,
}

/// A matching which keeps track of how each link was inferred, for visualization purposes
pub struct DetailedMatching<'src> {
    /// The full set of relations between nodes
    pub full: Matching<'src>,
    /// The relations between the roots of isomorphic subtrees
    pub exact: Matching<'src>,
    /// The so-called container matchings inferred in the bottom-up pass
    pub container: Matching<'src>,
    /// The so-called recovery matchings inferred from the container matchings
    pub recovery: Matching<'src>,
}

impl TreeMatcher {
    /// The GumTree classic matching algorithm.
    /// It can be supplied with an initial matching of nodes which are known
    pub fn match_trees<'a>(
        &self,
        left: &'a Ast<'a>,
        right: &'a Ast<'a>,
        initial_matching: Option<&Matching<'a>>,
    ) -> DetailedMatching<'a> {
        let start = Instant::now();

        // First pass, top down, matching pairs of isomorphic subtrees deeper than a certain depth
        let (matching, exact_matching) = self.top_down_pass(left, right, initial_matching);

        debug!("top-down phase yielded {} matches", exact_matching.len());

        let arena = Arena::new();
        let truncated_left = left
            .root()
            .truncate(&|node| exact_matching.get_from_left(node).is_some(), &arena);

        let truncated_right = right.root().truncate(
            &|node| exact_matching.get_from_right(node).is_some(),
            &arena,
        );
        let mut truncated_matching: Matching = matching.translate(truncated_left, truncated_right);

        // Second pass for container mappings
        let (container_matching, recovery_matches) =
            self.bottom_up_pass(truncated_left, truncated_right, &mut truncated_matching);
        debug!("matching took {:?}", start.elapsed());
        let mut full = matching;
        let container = container_matching.translate(left.root(), right.root());
        let recovery = recovery_matches.translate(left.root(), right.root());
        full.add_matching(&container);
        full.add_matching(&recovery);
        DetailedMatching {
            full,
            exact: exact_matching,
            container,
            recovery,
        }
    }

    // First pass of the GumTree classic algorithm, top down, creating the exact matchings between isomorphic subtrees
    fn top_down_pass<'a>(
        &self,
        left: &'a Ast<'a>,
        right: &'a Ast<'a>,
        initial_matching: Option<&Matching<'a>>,
    ) -> (Matching<'a>, Matching<'a>) {
        let mut matching = Matching::new();
        let mut exact_matching = Matching::new();
        let mut auxiliary = Matching::new();

        if let Some(initial_matching) = initial_matching {
            /*
            exact_matching.add_matching(initial_matching);
            for (right_match, left_match) in initial_matching.iter_right_to_left() {
                for (left_descendant, right_descendant) in left_match.dfs().zip(right_match.dfs()) {
                    matching.add(left_descendant, right_descendant);
                }
            } */
            matching.add_matching(initial_matching);
        }

        let mut l1 = PriorityList::new();
        let mut l2 = PriorityList::new();
        l1.push(left.root());
        l2.push(right.root());
        while min(l1.peek_max().unwrap_or(-1), l2.peek_max().unwrap_or(-1)) >= self.min_height {
            let pm_1 = l1.peek_max().unwrap_or(-1);
            let pm_2 = l2.peek_max().unwrap_or(-1);
            match pm_1.cmp(&pm_2) {
                Ordering::Greater => {
                    for t in l1.pop() {
                        l1.open(t);
                    }
                }
                Ordering::Less => {
                    for t in l2.pop() {
                        l2.open(t);
                    }
                }
                Ordering::Equal => {
                    let h1 = l1.pop();
                    let h2 = l2.pop();
                    let dups_left = h1
                        .iter()
                        .map(|n| n.hash)
                        .duplicates()
                        .collect::<HashSet<u64>>();
                    let dups_right = h2
                        .iter()
                        .map(|n| n.hash)
                        .duplicates()
                        .collect::<HashSet<u64>>();
                    let mut matched_1: HashSet<&AstNode<'a>> = HashSet::new();
                    let mut matched_2: HashSet<&AstNode<'a>> = HashSet::new();
                    for t1 in h1.iter() {
                        for t2 in h2.iter() {
                            if exact_matching.are_matched(t1, t2) {
                                matched_1.insert(t1);
                                matched_2.insert(t2);
                            } else if t1.isomorphic_to(t2) {
                                if dups_left.contains(&t1.hash) || dups_right.contains(&t2.hash) {
                                    auxiliary.add(t1, t2);
                                } else if matching.can_be_matched(t1, t2) {
                                    matched_1.insert(t1);
                                    matched_2.insert(t2);
                                    t1.dfs().zip(t2.dfs()).for_each(|(c1, c2)| {
                                        exact_matching.add(c1, c2);
                                        matching.add(c1, c2);
                                    });
                                }
                            }
                        }
                    }
                    // Add back all children of unmatched nodes to the queue
                    h1.iter()
                        .filter(|n| !matched_1.contains(*n))
                        .for_each(|n| l1.open(n));
                    h2.iter()
                        .filter(|n| !matched_2.contains(*n))
                        .for_each(|n| l2.open(n));
                }
            }
        }

        (matching, exact_matching)
    }

    /// Second pass of the GumTree classic algorithm, inferring container and recovery matchings
    fn bottom_up_pass<'a>(
        &self,
        left: &'a AstNode<'a>,
        right: &'a AstNode<'a>,
        matching: &mut Matching<'a>,
    ) -> (Matching<'a>, Matching<'a>) {
        let mut container_matching = Matching::new();
        let mut recovery_matches = Matching::new();
        // traverse the first tree in postfix order, looking at each unmapped node
        for left_node in left.postfix() {
            if left_node.is_root() {
                self.last_chance_match(
                    left_node,
                    right.root(),
                    matching,
                    &mut recovery_matches,
                    &mut container_matching,
                );
            } else if matching.get_from_left(left_node).is_some() || left_node.is_leaf() {
                continue;
            }
            let candidates = self.find_candidates(left_node, matching);
            let mut max_sim = -1.0_f32;
            let mut best_candidate = None;
            for candidate in candidates {
                let sim = matching.dice(left_node, candidate);
                if sim > max_sim && sim > self.sim_threshold {
                    max_sim = sim;
                    best_candidate = Some(candidate);
                } else if sim > max_sim && sim > self.sim_threshold * 0.75 {
                    debug!(
                        "discarding match with similarity {}, close to threshold {}",
                        sim, self.sim_threshold
                    );
                }
            }
            if let Some(winner) = best_candidate {
                // add candidates via tree edit distance matching or cheaper alternative
                self.last_chance_match(
                    left_node,
                    winner,
                    matching,
                    &mut recovery_matches,
                    &mut container_matching,
                );
            }
        }
        (container_matching, recovery_matches)
    }

    /// In the bottom up phase, finds candidates for matching a node,
    /// based on the pre-existing matches of its descendants
    fn find_candidates<'src>(
        &self,
        left_node: &'src AstNode<'src>,
        matching: &Matching<'src>,
    ) -> Vec<&'src AstNode<'src>> {
        let seeds = left_node
            .dfs()
            .flat_map(|descendant| matching.get_from_left(descendant).into_iter());
        let mut seen_ancestors = HashSet::new();
        let mut candidates = Vec::new();
        for seed in seeds {
            for ancestor in seed
                .ancestors()
                .skip(1)
                .filter(|ancestor| ancestor.parent().is_some())
            {
                if seen_ancestors.contains(ancestor) {
                    break;
                }
                seen_ancestors.insert(ancestor);
                if left_node.grammar_name == ancestor.grammar_name
                    && matching.get_from_right(ancestor).is_none()
                {
                    candidates.push(ancestor);
                }
            }
        }
        candidates
    }

    /// Recovers extra matches by computing an edit script via the Tree Edit Distance
    fn last_chance_match<'a>(
        &self,
        left: &'a AstNode<'a>,
        right: &'a AstNode<'a>,
        matching: &mut Matching<'a>,
        recovery_matching: &mut Matching<'a>,
        container_matching: &mut Matching<'a>,
    ) {
        if self.use_rted {
            let max_size = self.max_recovery_size;
            let left_stripped = self.strip_matched_subtrees(left, matching, true);
            let right_stripped = self.strip_matched_subtrees(right, matching, false);
            if left_stripped.size > max_size || right_stripped.size > max_size {
                debug!(
                    "falling back on linear recovery from {} because size is {}, {}",
                    left.grammar_name, left_stripped.size, right_stripped.size,
                );
                self.match_subtrees_linearly(left, right, true, matching, recovery_matching);
            } else {
                // add candidates via tree edit distance matching
                let (edits, _cost) = diff(&left_stripped, &right_stripped);
                let left_nodes = [&left_stripped];
                let right_nodes = [&right_stripped];
                self.convert_tree_edits_to_matches(
                    &left_nodes,
                    &right_nodes,
                    &edits,
                    recovery_matching,
                    matching,
                );
            }
        } else {
            self.match_subtrees_linearly(left, right, false, matching, recovery_matching);
        }
        matching.add(left, right);
        container_matching.add(left, right);
    }

    /// Poor man's approximation of the RTED matching above, which has linear complexity in the size of
    /// both trees matched. It will return less matches however.
    fn match_subtrees_linearly<'a>(
        &self,
        left: &'a AstNode<'a>,
        right: &'a AstNode<'a>,
        recursive: bool,
        matching: &mut Matching<'a>,
        recovery_matching: &mut Matching<'a>,
    ) {
        // index children by type and signature
        let left_children: MultiMap<(&'static str, Option<Signature>), &'a AstNode<'a>> = left
            .children
            .iter()
            .map(|node| {
                (
                    (node.grammar_name, None), // self.lang_profile.extract_signature(node)),
                    *node,
                )
            })
            .collect();
        let right_children: MultiMap<(&'static str, Option<Signature>), &'a AstNode<'a>> = right
            .children
            .iter()
            .map(|node| {
                (
                    (node.grammar_name, None), // self.lang_profile.extract_signature(node)),
                    *node,
                )
            })
            .collect();

        for ((node_type, signature), children_l) in left_children.iter() {
            if children_l.len() == 1 {
                let children_r = right_children.get((node_type, signature.clone())); // TODO avoid this clone and check for length == 1 in a better way
                if children_r.len() == 1 {
                    let child_l = children_l.iter().next().unwrap();
                    let child_r = children_r.iter().next().unwrap();
                    if matching.can_be_matched(child_l, child_r) {
                        if signature.is_some() || recursive {
                            self.match_subtrees_linearly(
                                child_l,
                                child_r,
                                recursive,
                                matching,
                                recovery_matching,
                            );
                        }
                        matching.add(child_l, child_r);
                        recovery_matching.add(child_l, child_r);
                    }
                }
            }
        }
    }

    /// Strips trees of already matched components
    fn strip_matched_subtrees<'a>(
        &self,
        node: &'a AstNode<'a>,
        matching: &Matching<'a>,
        left_side: bool,
    ) -> TEDTree<'a> {
        let mut children = Vec::new();
        let matched_node = if node.is_root() {
            None
        } else if left_side {
            matching.get_from_left(node)
        } else {
            matching.get_from_right(node)
        };
        if matched_node.is_none() {
            for child in node.children.iter() {
                children.push(self.strip_matched_subtrees(child, matching, left_side));
            }
        }
        let size = children.iter().map(|c| c.size).sum::<i32>() + 1;
        TEDTree {
            node,
            matched_to_id: matched_node.map(|n| if left_side { node.id } else { n.id }),
            children,
            weight: matched_node
                .map(|n| if left_side { node.size() } else { n.size() })
                .unwrap_or(1) as u64,
            size,
        }
    }

    /// Recursively extract matches from edit script between two trees
    fn convert_tree_edits_to_matches<'a>(
        &self,
        left_nodes: &[&TEDTree<'a>],
        right_nodes: &[&TEDTree<'a>],
        edits: &[Edit],
        recovery_matching: &mut Matching<'a>,
        matching: &mut Matching<'a>,
    ) {
        let mut left_iterator = left_nodes.iter();
        let mut right_iterator = right_nodes.iter();
        let mut left_cursor = left_iterator.next();
        let mut right_cursor = right_iterator.next();
        for edit in edits.iter() {
            match edit {
                Edit::Replace(child_edits) => {
                    if let (Some(left_node), Some(right_node)) = (left_cursor, right_cursor) {
                        assert_eq!(left_node.node.grammar_name, right_node.node.grammar_name, "Inconsistent grammar names between nodes matched by tree edit distance");
                        if matching.can_be_matched(left_node.node, right_node.node) {
                            matching.add(left_node.node, right_node.node);
                            recovery_matching.add(left_node.node, right_node.node);
                            self.convert_tree_edits_to_matches(
                                left_node.children.iter().collect_vec().as_slice(),
                                right_node
                                    .children
                                    .as_slice()
                                    .iter()
                                    .collect_vec()
                                    .as_slice(),
                                child_edits,
                                recovery_matching,
                                matching,
                            );
                        }
                        left_cursor = left_iterator.next();
                        right_cursor = right_iterator.next();
                    } else {
                        panic!("Trees to match and produced edit script are inconsistent");
                    }
                }
                Edit::Insert => right_cursor = right_iterator.next(),
                Edit::Remove => left_cursor = left_iterator.next(),
            }
        }
    }
}

/// Internal tree structure used to interface with the tree edit distance library,
/// as well as strip matched subtrees before computing the tree edit distance.
#[derive(Debug)]
struct TEDTree<'a> {
    node: &'a AstNode<'a>,
    matched_to_id: Option<usize>,
    children: Vec<TEDTree<'a>>,
    weight: u64,
    size: i32,
}

impl<'a> tree_edit_distance::Node for TEDTree<'a> {
    type Kind = (&'static str, Option<&'a str>, Option<usize>);

    fn kind(&self) -> Self::Kind {
        let maybe_source = if self.children.is_empty() {
            Some(self.node.source)
        } else {
            None
        };
        (
            self.node.grammar_name,
            maybe_source, // ensures that if the node is a leaf, it is only matched to a leaf with the same textual content
            self.matched_to_id,
        ) // ensures that if the node is matched, it can only be equated to its match on the other side
    }

    type Weight = u64;
    fn weight(&self) -> Self::Weight {
        self.weight
    }
}

impl<'a> tree_edit_distance::Tree for TEDTree<'a> {
    type Children<'c> = Box<dyn Iterator<Item = &'c TEDTree<'a>> + 'c>
    where
        Self: 'c;

    fn children(&self) -> Self::Children<'_> {
        Box::new(self.children.iter())
    }
}

impl<'a> TEDTree<'a> {
    fn display(&self, f: &mut std::fmt::Formatter<'_>, indentation: usize) -> std::fmt::Result {
        let pad = " ".to_string().repeat(indentation);
        write!(
            f,
            "{}TEDTree({}, {}{}",
            pad,
            self.node.grammar_name,
            match self.matched_to_id {
                None => "unmatched".to_string(),
                Some(id) => id.to_string(),
            },
            if self.children.is_empty() {
                ")\n"
            } else {
                ":\n"
            }
        )?;
        for child in self.children.iter() {
            child.display(f, indentation + 2)?;
        }
        if !self.children.is_empty() {
            writeln!(f, "{pad})")?;
        }
        Ok(())
    }
}

impl<'a> Display for TEDTree<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.display(f, 0)
    }
}

#[cfg(test)]
mod tests {

    use crate::test_utils::ctx;

    use super::*;

    #[test]
    fn small_sample() {
        let ctx = ctx();
        let lang_profile = LangProfile::detect_from_filename("test.rs").unwrap();

        let t1 = ctx.parse_rust("fn my_func() -> i32 { 1 + (3 + (5 - 1)) }");
        let t2 = ctx.parse_rust("fn other_func() { (3 + (5 - 1)) * 2 }");

        let matcher = TreeMatcher {
            min_height: 2,
            sim_threshold: 0.5,
            max_recovery_size: 100,
            use_rted: true,
            lang_profile,
        };

        let detailed_matching = matcher.match_trees(&t1, &t2, None);

        assert_eq!(detailed_matching.exact.len(), 13);
        assert_eq!(detailed_matching.container.len(), 4);
        assert_eq!(detailed_matching.recovery.len(), 10);
        assert_eq!(detailed_matching.full.len(), 23);
    }

    #[test]
    fn example_from_the_paper() {
        let ctx = ctx();
        let lang_profile = LangProfile::detect_from_filename("test.rs").unwrap();

        let t1 = ctx.parse_java(
            "public class Test { public String foo(int i) { if (i == 0) return \"Foo!\"; } }",
        );
        let t2 = ctx.parse_java("public class Test { private String foo(int i) { if (i == 0) return \"Bar\"; else if (i == -1) return \"Foo!\"; } }");

        let matcher = TreeMatcher {
            min_height: 2,
            sim_threshold: 0.5,
            max_recovery_size: 100,
            use_rted: true,
            lang_profile,
        };

        let matching = matcher.match_trees(&t1, &t2, None);

        assert_eq!(matching.exact.len(), 21);
        assert_eq!(matching.container.len(), 6);
        assert_eq!(matching.recovery.len(), 18);
        assert_eq!(matching.full.len(), 39);
    }

    #[test]
    fn without_rted() {
        let ctx = ctx();
        let lang_profile = LangProfile::detect_from_filename("test.rs").unwrap();

        let t1 = ctx.parse_java(
            "public class Test { public String foo(int i) { if (i == 0) return \"Foo!\"; } }",
        );
        let t2 = ctx.parse_java("public class Test { private String foo(int i) { if (i == 0) return \"Bar\"; else if (i == -1) return \"Foo!\"; } }");

        let matcher = TreeMatcher {
            min_height: 2,
            sim_threshold: 0.5,
            max_recovery_size: 100,
            use_rted: false,
            lang_profile,
        };

        let matching = matcher.match_trees(&t1, &t2, None);

        assert_eq!(matching.exact.len(), 21);
        assert_eq!(matching.container.len(), 6);
        assert_eq!(matching.recovery.len(), 9);
        assert_eq!(matching.full.len(), 36);
    }

    #[test]
    fn matching_very_shallow_structures() {
        let ctx = ctx();
        let lang_profile = LangProfile::detect_from_filename("test.rs").unwrap();

        let left = ctx.parse_json("[1, 2]");
        let right = ctx.parse_json("[0, 1, 2]");

        let matcher = TreeMatcher {
            min_height: 0,
            sim_threshold: 0.5,
            max_recovery_size: 100,
            use_rted: true,
            lang_profile,
        };

        let matching = matcher.match_trees(&left, &right, None);

        assert_eq!(matching.exact.len(), 4);
        assert_eq!(matching.container.len(), 2);
        assert_eq!(matching.recovery.len(), 3);
        assert_eq!(matching.full.len(), 7);
    }

    #[test]
    fn matching_rust_types() {
        let ctx = ctx();
        let lang_profile = LangProfile::detect_from_filename("test.rs").unwrap();

        let left = ctx.parse_rust("use std::collections::{HashMap};");
        let right = ctx.parse_rust("use std::collections::{HashMap, HashSet};");

        let matcher = TreeMatcher {
            min_height: 2,
            sim_threshold: 0.5,
            max_recovery_size: 100,
            use_rted: true,
            lang_profile,
        };
        let matching = matcher.match_trees(&left, &right, None);

        assert_eq!(matching.exact.len(), 0);
        assert_eq!(matching.container.len(), 1);
        assert_eq!(matching.recovery.len(), 14);
        assert_eq!(matching.full.len(), 14);
    }
}
