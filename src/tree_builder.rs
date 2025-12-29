use std::collections::{HashMap, HashSet};
use std::fmt::Display;
use thiserror::Error;

use either::Either;
use itertools::Itertools;
use log::{debug, trace};
use rustc_hash::FxHashSet;

use crate::merged_tree::Conflict;
use crate::utils::InternalError;
use crate::{
    ast::AstNode,
    changeset::ChangeSet,
    class_mapping::{ClassMapping, Leader, RevNode, RevisionNESet, RevisionSet},
    lang_profile::CommutativeParent,
    merged_tree::MergedTree,
    multimap::MultiMap,
    pcs::{PCSNode, Revision},
    settings::DisplaySettings,
};

/// An internal structure to map a parent and a predecessor to a possible successor in each revision
struct SuccessorMap<'a> {
    multimap: HashMap<PCSNode<'a>, MultiMap<PCSNode<'a>, (Revision, PCSNode<'a>)>>,
    empty: MultiMap<PCSNode<'a>, (Revision, PCSNode<'a>)>,
}

impl<'a> SuccessorMap<'a> {
    fn new(changeset: &ChangeSet<'a>) -> Self {
        let mut parent_to_children: HashMap<
            PCSNode<'a>,
            MultiMap<PCSNode<'a>, (Revision, PCSNode<'a>)>,
        > = HashMap::new();
        for pcs in changeset.iter() {
            let parent_map = parent_to_children.entry(pcs.parent).or_default();
            parent_map.insert(pcs.predecessor, (pcs.revision, pcs.successor));
        }
        SuccessorMap {
            multimap: parent_to_children,
            empty: MultiMap::new(),
        }
    }

    fn get(&self, parent: &PCSNode<'a>) -> &MultiMap<PCSNode<'a>, (Revision, PCSNode<'a>)> {
        self.multimap.get(parent).unwrap_or(&self.empty)
    }
}

/// Algorithm to build back a tree from a changeset, holding the associated static state.
pub struct TreeBuilder<'a, 'b> {
    // index the set of PCS triples by parent
    merged_successors: SuccessorMap<'a>,
    base_successors: SuccessorMap<'a>,
    class_mapping: &'b ClassMapping<'a>,
    settings: &'b DisplaySettings<'a>,
}

/// Variable state, keeping track of visited nodes to avoid looping
#[derive(Debug, Clone)]
struct VisitingState<'a> {
    deleted_and_modified: HashSet<Leader<'a>>,
    visited_nodes: HashSet<Leader<'a>>,
}

impl VisitingState<'_> {
    fn indentation(&self) -> String {
        " ".repeat(self.visited_nodes.len())
    }
}

#[derive(Error, Debug, PartialEq, Eq)]
pub enum TreeBuildingError<'a> {
    // Errors that are expected to happen in certain cases.
    #[error("node `{node}` encountered twice, which generates an infinite tree")]
    NodeEncounteredTwice { node: Leader<'a> },
    #[error("children not allowed to commute per their types")]
    UncommutableChildren,

    // Internal errors, which are a sign of a programming
    // error and should never be allowed to happen, regardless of the input data.
    // To avoid panicking in production, we still return an error for those.
    #[error("more than two conflicting sides after node `{node}`")]
    MoreThanTwoConflictingSides { node: PCSNode<'a> },
    #[error("the virtual root needs to have a child, none found")]
    NoVirtualRootChildFound,
    #[error("impossible to do a line-based fallback merge for a virtual node")]
    LineBasedFallbackOnVirtualNode,
    #[error("impossible to build a subtree for a virtual left/right marker")]
    BuildSubtreeForVirtualMarker,
}

type SuccessorsCursor<'a> = FxHashSet<(Revision, PCSNode<'a>)>;

impl<'a, 'b> TreeBuilder<'a, 'b> {
    /// Create a tree builder from PCS triples, the class mapping and language-specific settings
    pub fn new(
        merged_changeset: &ChangeSet<'a>,
        base_changeset: &ChangeSet<'a>,
        class_mapping: &'b ClassMapping<'a>,
        settings: &'b DisplaySettings<'a>,
    ) -> Self {
        TreeBuilder {
            merged_successors: SuccessorMap::new(merged_changeset),
            base_successors: SuccessorMap::new(base_changeset),
            class_mapping,
            settings,
        }
    }

    /// Build the merged tree
    pub fn build_tree(&self) -> Result<MergedTree<'a>, TreeBuildingError<'a>> {
        let mut visiting_state = VisitingState {
            // keep track of all nodes that have been deleted on one side and modified on the other
            deleted_and_modified: HashSet::new(),
            // keep track of visited nodes in the recursive algorithm to avoid looping
            visited_nodes: HashSet::new(),
        };

        // recursively build the tree by starting from the virtual root
        let merged_tree = self.build_subtree(PCSNode::VirtualRoot, &mut visiting_state)?;

        debug!("{merged_tree}");

        let deleted_and_modified = visiting_state.deleted_and_modified;
        // check if any deleted and modified nodes are absent from the resulting tree
        debug!(
            "deleted and modified: {}",
            deleted_and_modified.iter().format(", ")
        );
        let deleted: HashSet<Leader<'a>> = deleted_and_modified
            .into_iter()
            .filter(|deleted| !merged_tree.contains(deleted, self.class_mapping))
            .collect();
        debug!("really deleted children: {}", deleted.iter().format(", "));

        let parents_to_recompute: HashSet<Leader<'a>> = deleted
            .into_iter()
            .map(|deleted| {
                let RevNode { rev, node } = deleted.as_representative();
                self.class_mapping.map_to_leader(RevNode::new(
                    rev,
                    node.parent().expect(
                        "the root node is marked as deleted and modified, \
                        but all roots should be mapped together",
                    ),
                ))
            })
            .collect();
        debug!(
            "parents to recompute: {}",
            parents_to_recompute.iter().format(", ")
        );

        Ok(merged_tree.force_line_based_fallback_on_specific_nodes(
            &parents_to_recompute,
            self.class_mapping,
            self.settings,
        ))
    }

    /// Recursive function to build the merged subtree rooted in a node,
    /// checking if it has already been visited to avoid looping.
    fn build_subtree(
        &'b self,
        node: PCSNode<'a>,
        visiting_state: &mut VisitingState<'a>,
    ) -> Result<MergedTree<'a>, TreeBuildingError<'a>> {
        if let PCSNode::Node { node, .. } = node {
            let visited = &mut visiting_state.visited_nodes;
            if visited.contains(&node) {
                return Err(TreeBuildingError::NodeEncounteredTwice { node });
            }
            visited.insert(node);
        }
        let result = self.build_subtree_from_changeset(node, visiting_state);
        if let PCSNode::Node { node, .. } = node {
            visiting_state.visited_nodes.remove(&node);
        }
        result
    }

    // Main recursive function to build the merged subtree from a node
    // (without loop checking)
    fn build_subtree_from_changeset(
        &'b self,
        node: PCSNode<'a>,
        visiting_state: &mut VisitingState<'a>,
    ) -> Result<MergedTree<'a>, TreeBuildingError<'a>> {
        // if the node has isomorphic subtrees in all revisions, that's very boring,
        // so we just return a tree that matches that
        if let PCSNode::Node {
            revisions,
            node: leader,
        } = node
            && revisions.is_full()
            && self.class_mapping.is_isomorphic_in_all_revisions(&leader)
        {
            // If one of the sides is doing a reformatting, make sure we pick this side for pretty printing,
            // so that we preserve the new formatting.
            let final_revisions = if self.class_mapping.is_reformatting(&leader, Revision::Left) {
                RevisionNESet::singleton(Revision::Left)
            } else if self.class_mapping.is_reformatting(&leader, Revision::Right) {
                RevisionNESet::singleton(Revision::Right)
            } else {
                revisions
            };

            return Ok(MergedTree::new_exact(
                leader,
                final_revisions,
                self.class_mapping,
            ));
        }

        let children_map = self.merged_successors.get(&node);
        let base_children_map = self.base_successors.get(&node);

        let mut children = Vec::new();
        let mut predecessor = PCSNode::LeftMarker;
        let mut cursor = children_map.get(&predecessor);
        let mut seen_nodes: HashSet<PCSNode<'a>> = HashSet::new(); // to avoid looping, and to make sure every single known predecessor is visited
        seen_nodes.insert(predecessor);

        let pad = visiting_state.indentation();
        trace!("{pad}{node} build_subtree_from_changeset");

        loop {
            match cursor.len() {
                0 => {
                    // This could be a double delete or a delete/modified conflict.
                    // Following the 3DM algorithm, we fall back on line-based merges in this case.
                    // See merge_3dm::tests::{delete_delete, commutative_conflict_delete_delete, commutative_conflict_delete_modified}.
                    return self.commutative_or_line_based_local_fallback(node, visiting_state);
                }
                1 => {
                    // only a single successor, great
                    let (_, current_child) = cursor
                        .iter()
                        .next()
                        .copied()
                        .expect("cursor.len() == 1 but it is actually empty?!");
                    if current_child == PCSNode::RightMarker {
                        break;
                    }
                    if seen_nodes.contains(&current_child) {
                        // there is a loop of children: abort and fall back on line diffing
                        let line_diff =
                            self.commutative_or_line_based_local_fallback(node, visiting_state);
                        return line_diff;
                    }

                    let subtree = self.build_subtree(current_child, visiting_state);
                    let Ok(child_result_tree) = subtree else {
                        // we failed to build the result tree for a child of this node, because of a nasty conflict.
                        // We fall back on line diffing
                        let line_diff =
                            self.commutative_or_line_based_local_fallback(node, visiting_state);
                        return line_diff;
                    };
                    children.push(child_result_tree);
                    predecessor = current_child;
                    seen_nodes.insert(predecessor);
                    cursor = children_map.get(&predecessor);
                }
                2 => {
                    let Ok((next_cursor, conflict)) = self.build_conflict(
                        predecessor,
                        children_map,
                        base_children_map,
                        &mut seen_nodes,
                        visiting_state,
                    ) else {
                        let line_based =
                            self.commutative_or_line_based_local_fallback(node, visiting_state);
                        return line_based;
                    };

                    let Conflict { base, left, right } = conflict;

                    if let PCSNode::Node { node: leader, .. } = node
                        && let Some(commutative_parent) = leader.commutative_parent_definition()
                        && let Ok(solved_conflict) = self.commutatively_merge_lists(
                            &base,
                            &left,
                            &right,
                            commutative_parent,
                            visiting_state,
                        )
                    {
                        children.extend(solved_conflict);
                    } else {
                        children.extend(MergedTree::new_conflict(
                            base,
                            left,
                            right,
                            self.class_mapping,
                        ));
                    }
                    cursor = next_cursor;
                }
                _ => {
                    return Err(TreeBuildingError::MoreThanTwoConflictingSides {
                        node: predecessor,
                    })
                    .debug_panic();
                }
            }
        }

        // check that all non-base nodes were visited
        let non_base_nodes: HashSet<PCSNode<'a>> = children_map
            .keys()
            .copied()
            .filter(|pcsnode| {
                if let PCSNode::Node { revisions, .. } = pcsnode {
                    !revisions.contains(Revision::Base)
                } else {
                    false
                }
            })
            .collect();
        if !seen_nodes.is_superset(&non_base_nodes) {
            // We have a conflict where some node is deleted and we cannot gather where exactly.
            trace!(
                "{pad}{node} Error while gathering successors, some non-base successors were not visited:"
            );
            trace!(
                "{pad}{}",
                non_base_nodes.difference(&seen_nodes).format(", ")
            );
            return self.commutative_or_line_based_local_fallback(node, visiting_state);
        }

        // check that all base nodes that were not visited (deleted on one side) have not been changed on the other side
        for unvisited_base_node in base_children_map
            .keys()
            .copied()
            .filter(|pcsnode| !seen_nodes.contains(pcsnode))
        {
            trace!("{pad}{node} Checking unvisited base node {unvisited_base_node}");
            let PCSNode::Node {
                node: unvisited,
                revisions,
            } = unvisited_base_node
            else {
                continue;
            };
            if visiting_state.visited_nodes.contains(&unvisited) {
                continue;
            }
            let (modified_revision, target_revision) = if revisions.contains(Revision::Left) {
                (Revision::Left, Revision::Right)
            } else if revisions.contains(Revision::Right) {
                (Revision::Right, Revision::Left)
            } else {
                continue; // node was deleted on both sides, we don't care about preserving any changes made to it
            };
            // recursively build the tree representation for the unvisited base node to see if it has any changes
            if let Ok(base_tree) = self.build_subtree(unvisited_base_node, visiting_state)
                && let Some(cover) =
                    self.cover_modified_nodes(&base_tree, target_revision, modified_revision)
            {
                visiting_state.deleted_and_modified.extend(cover.iter());
            } else {
                // as a fallback solution, if we could not compute a cover of the changes in the deleted tree,
                // we request that the root of the subtree is present in the merged output.
                visiting_state.deleted_and_modified.insert(unvisited);
            }
        }

        match node {
            PCSNode::VirtualRoot => children
                .into_iter()
                .next()
                .ok_or(TreeBuildingError::NoVirtualRootChildFound)
                .debug_panic(),
            PCSNode::LeftMarker => {
                Err(TreeBuildingError::BuildSubtreeForVirtualMarker).debug_panic()
            }
            PCSNode::RightMarker => {
                Err(TreeBuildingError::BuildSubtreeForVirtualMarker).debug_panic()
            }
            PCSNode::Node {
                node: revnode,
                revisions,
            } => {
                // Check if all the children are exact trees with at least one common revision
                let mut children_revnodes = Vec::new();
                let mut common_revisions = revisions.set();
                for child in &children {
                    if let MergedTree::ExactTree {
                        node, revisions, ..
                    } = child
                    {
                        common_revisions = common_revisions.intersection(revisions.set());
                        children_revnodes.push(*node);
                    } else {
                        // the child is not a tree that exactly matches a subtree in at least one revision,
                        // so we give up as its parent can also not be one either
                        common_revisions = RevisionSet::new();
                        break;
                    }
                }
                if !common_revisions.is_empty() {
                    let children_revnodes = Some(children_revnodes);
                    for common_rev in common_revisions.iter() {
                        let at_rev = self
                            .class_mapping
                            .children_at_revision(&revnode, common_rev);
                        // Check if the list of children is the same at that revision
                        if at_rev != children_revnodes {
                            common_revisions.remove(common_rev);
                        }
                    }
                }
                if let Some(common_revisions) = common_revisions.as_nonempty() &&
                    // if one of the left/right revisions is doing a reformatting, we make sure it's included in the merged result
                    (!self.class_mapping.is_reformatting(&revnode, Revision::Left) || common_revisions.contains(Revision::Left)) &&
                    (!self.class_mapping.is_reformatting(&revnode, Revision::Right) || common_revisions.contains(Revision::Right))
                {
                    Ok(MergedTree::new_exact(
                        revnode,
                        common_revisions,
                        self.class_mapping,
                    ))
                } else {
                    Ok(MergedTree::new_mixed(revnode, children))
                }
            }
        }
    }

    /// Construct a conflict by following successors on all three revisions
    /// from the given predecessor.
    fn build_conflict(
        &self,
        predecessor: PCSNode<'a>,
        merged_successors: &'b MultiMap<PCSNode<'a>, (Revision, PCSNode<'a>)>,
        base_successors: &'b MultiMap<PCSNode<'a>, (Revision, PCSNode<'a>)>,
        seen_nodes: &mut HashSet<PCSNode<'a>>,
        visiting_state: &mut VisitingState<'a>,
    ) -> Result<(&'b SuccessorsCursor<'a>, Conflict<'a>), String> {
        let pad = visiting_state.indentation();
        trace!("{pad}{predecessor} build_conflict");
        let (end_left, list_left) = self.extract_conflict_side(
            predecessor,
            Revision::Left,
            merged_successors,
            base_successors,
            seen_nodes,
            visiting_state,
        )?;
        let (end_right, list_right) = self.extract_conflict_side(
            predecessor,
            Revision::Right,
            merged_successors,
            base_successors,
            seen_nodes,
            visiting_state,
        )?;
        let (end_base, list_base) = self.extract_conflict_side(
            predecessor,
            Revision::Base,
            base_successors,
            merged_successors,
            seen_nodes,
            visiting_state,
        )?;

        fn strip_revs<'a, S>(end: &HashSet<(Revision, PCSNode<'a>), S>) -> HashSet<PCSNode<'a>> {
            end.iter().map(|(_, node)| *node).collect()
        }

        let base_stripped = strip_revs(end_base);
        let left_stripped = strip_revs(end_left);
        let right_stripped = strip_revs(end_right);
        if base_stripped != left_stripped || base_stripped != right_stripped {
            Err(format!(
                "ends don't match: {}, {}, {}",
                fmt_set(end_base),
                fmt_set(end_left),
                fmt_set(end_right)
            ))
        } else {
            Ok((
                end_base,
                Conflict {
                    base: list_base,
                    left: list_left,
                    right: list_right,
                },
            ))
        }
    }

    /// Extract one side of a conflict by iteratively following `successors` from
    /// the given `starting_node` until we either:
    /// - find a node present in the other conflict side
    /// - reach the last child of `starting_node`'s parent ([`PCSNode::RightMarker`])
    ///
    /// When either of those (end node) is found, return:
    /// - the successors of end node
    /// - the path from `starting_node` to end node
    fn extract_conflict_side(
        &self,
        starting_node: PCSNode<'a>,
        revision: Revision,
        successors: &'b MultiMap<PCSNode<'a>, (Revision, PCSNode<'a>)>,
        other_successors: &'b MultiMap<PCSNode<'a>, (Revision, PCSNode<'a>)>,
        seen_nodes: &mut HashSet<PCSNode<'a>>,
        visiting_state: &mut VisitingState<'a>,
    ) -> Result<(&'b SuccessorsCursor<'a>, Vec<&'a AstNode<'a>>), String> {
        let pad = visiting_state.indentation();
        trace!("{pad}{starting_node} extract_conflict_side");
        let mut result = Vec::new();
        let mut cursor = starting_node;
        loop {
            let all_successors = successors.get(&cursor);
            let candidate = all_successors
                .iter()
                .copied()
                .find_map(|(rev, node)| (rev == revision).then_some(node))
                .ok_or_else(|| {
                    format!("no candidate successor found for {cursor} at {revision}")
                })?;

            if other_successors.contains_key(&candidate) {
                // we found the merging point of the conflict branches
                return Ok((all_successors, result));
            }

            match candidate {
                PCSNode::VirtualRoot | PCSNode::LeftMarker => {
                    unreachable!("those can't be successors")
                }
                PCSNode::RightMarker => {
                    // `starting_node`'s parent has no more children - we can end the search here
                    return Ok((all_successors, result));
                }
                PCSNode::Node { node, .. } => {
                    let representative = self.class_mapping.node_at_rev(&node, revision)
                        .expect("extract_conflict_side: gathering a class leader which doesn't have a representative in the revision");
                    result.push(representative);
                    if !seen_nodes.insert(candidate) {
                        return Err("PCS successor loop detected".to_string());
                    }
                    cursor = candidate;
                }
            }
        }
    }

    /// Attempt to merge the children of the given node commutatively, if the node
    /// is indeed a commutative parent. If that fails, fall back on line-based merging.
    fn commutative_or_line_based_local_fallback(
        &self,
        node: PCSNode<'a>,
        visiting_state: &mut VisitingState<'a>,
    ) -> Result<MergedTree<'a>, TreeBuildingError<'a>> {
        let pad = visiting_state.indentation();
        trace!("{pad}{node} commutative_or_line_based_local_fallback");
        let PCSNode::Node { node, .. } = node else {
            return Err(TreeBuildingError::LineBasedFallbackOnVirtualNode).debug_panic();
        };
        // If the root happens to be commutative, we can merge all children accordingly.
        if let Some(commutative_parent) = node.commutative_parent_definition()
            && let Ok(commutative_merge) =
                self.commutatively_merge_children(&node, commutative_parent, visiting_state)
        {
            Ok(MergedTree::new_mixed(node, commutative_merge))
        } else {
            Ok(MergedTree::line_based_local_fallback_for_revnode(
                node,
                self.class_mapping,
                self.settings,
            ))
        }
    }

    /// From a list of children of a commutative node, filter out separators
    /// and delimiters to return the content nodes only.
    fn keep_content_only<'c>(
        &'c self,
        slice: &'c [&'a AstNode<'a>],
        revision: Revision,
        trimmed_sep: &'c str,
        trimmed_left_delim: &'c str,
        trimmed_right_delim: &'c str,
    ) -> impl Iterator<Item = Leader<'a>> {
        slice
            .iter()
            .filter(move |n| {
                let trimmed = n.source.trim();
                trimmed != trimmed_sep
                    && trimmed != trimmed_left_delim
                    && trimmed != trimmed_right_delim
            })
            .map(move |n| self.class_mapping.map_to_leader(RevNode::new(revision, n)))
    }

    /// Collects examples of separators with the surrounding whitespace
    /// among a list of children of a commutative parent.
    fn find_separators_with_whitespace<'s>(
        slice: &'s [&'a AstNode<'a>],
        trimmed_sep: &'s str,
    ) -> impl Iterator<Item = &'a str> {
        if trimmed_sep.is_empty() {
            Either::Left(
                slice
                    .iter()
                    .skip(1)
                    .filter_map(|node| node.preceding_whitespace())
                    .filter(|s| !s.is_empty()),
            )
        } else {
            Either::Right(
                slice
                    .iter()
                    .filter(move |n| n.source.trim() == trimmed_sep)
                    .map(|n| n.source_with_surrounding_whitespace()),
            )
        }
    }

    /// Merge three lists of nodes, knowing that their order does not matter
    fn commutatively_merge_lists(
        &self,
        base: &[&'a AstNode<'a>],
        left: &[&'a AstNode<'a>],
        right: &[&'a AstNode<'a>],
        commutative_parent: &CommutativeParent,
        visiting_state: &mut VisitingState<'a>,
    ) -> Result<Vec<MergedTree<'a>>, TreeBuildingError<'a>> {
        let pad = visiting_state.indentation();
        trace!("{pad}commutatively_merge_lists");
        // TODO improve handling of comments? comments added by the right side should ideally be placed sensibly

        // check that all the nodes involved are allowed to commute in this context
        let raw_separator = commutative_parent
            .child_separator(base, left, right)
            .ok_or(TreeBuildingError::UncommutableChildren)?;
        let trimmed_sep = raw_separator.trim();
        let trimmed_left_delim = commutative_parent.left_delim.unwrap_or_default().trim();
        let trimmed_right_delim = commutative_parent.right_delim.unwrap_or_default().trim();

        // map each list via class mapping to make each element comparable
        let base_leaders: HashSet<_> = self
            .keep_content_only(
                base,
                Revision::Base,
                trimmed_sep,
                trimmed_left_delim,
                trimmed_right_delim,
            )
            .collect();
        let left_leaders: Vec<_> = self
            .keep_content_only(
                left,
                Revision::Left,
                trimmed_sep,
                trimmed_left_delim,
                trimmed_right_delim,
            )
            .collect();
        let right_leaders: Vec<_> = self
            .keep_content_only(
                right,
                Revision::Right,
                trimmed_sep,
                trimmed_left_delim,
                trimmed_right_delim,
            )
            .collect();

        let left_added: HashSet<_> = left_leaders
            .iter()
            .filter(|x| !base_leaders.contains(x))
            .collect();
        trace!("{pad}left_added: {}", left_added.iter().format(", "));
        let right_added: Vec<_> = right_leaders
            .iter()
            .filter(|x| !base_leaders.contains(x) && !left_added.contains(x))
            .collect();
        trace!("{pad}right_added: {}", right_added.iter().format(", "));

        // then, compute the symmetric difference between the base and right lists
        let right_removed: HashSet<Leader<'_>> = base_leaders
            .into_iter()
            .filter(|x| !right_leaders.contains(x))
            .collect();
        trace!("{pad}right_removed: {}", right_removed.iter().format(", "));
        // check which right removed elements have been modified on the left-hand side,
        // in which case they should be kept
        let mut removed_visiting_state = visiting_state.clone();
        let right_removed_content: Vec<_> = right_removed
            .into_iter()
            .map(|revnode| {
                let subtree = self.build_subtree(
                    PCSNode::Node {
                        revisions: self.class_mapping.revision_set(&revnode),
                        node: revnode,
                    },
                    &mut removed_visiting_state,
                )?;
                Ok((revnode, subtree))
            })
            .collect::<Result<_, TreeBuildingError<'a>>>()?;
        let right_removed_and_not_modified: HashSet<_> = right_removed_content
            .into_iter()
            .filter(|(_, result_tree)| match result_tree {
                MergedTree::ExactTree { revisions, .. } => revisions.contains(Revision::Base),
                _ => false,
            })
            .map(|(revnode, _)| revnode)
            .collect();

        // apply this symmetric difference to the left list
        let merged: Vec<_> = left_leaders
            .iter()
            .filter(|n| !right_removed_and_not_modified.contains(n))
            .chain(right_added)
            .collect();

        // build the result tree for each element of the result
        let merged_content: Vec<MergedTree<'a>> = merged
            .into_iter()
            .map(|revnode| {
                self.build_subtree(
                    PCSNode::Node {
                        revisions: self.class_mapping.revision_set(revnode),
                        node: *revnode,
                    },
                    visiting_state,
                )
            })
            .collect::<Result<_, _>>()?;

        // try to find examples of delimiters and separator in the existing revisions
        let left_delim = [
            (&base, Revision::Base),
            (&left, Revision::Left),
            (&right, Revision::Right),
        ]
        .into_iter()
        .find_map(|(nodes, revision)| {
            nodes.first().and_then(|first| {
                (first.source.trim() == trimmed_left_delim).then_some(
                    self.class_mapping
                        .map_to_leader(RevNode::new(revision, first)),
                )
            })
        });
        let right_delim = [
            (&base, Revision::Base),
            (&left, Revision::Left),
            (&right, Revision::Right),
        ]
        .into_iter()
        .find_map(|(nodes, revision)| {
            nodes.last().and_then(|last| {
                (last.source.trim() == trimmed_right_delim).then_some(
                    self.class_mapping
                        .map_to_leader(RevNode::new(revision, last)),
                )
            })
        });
        let starts_with_separator = [&base, &left, &right].into_iter().any(|rev| {
            rev.iter()
                .map(|n| n.source.trim())
                .find(|s| *s != trimmed_left_delim)
                == Some(trimmed_sep)
        });
        let ends_with_separator = [&base, &left, &right].into_iter().any(|rev| {
            rev.iter()
                .map(|n| n.source.trim())
                .rfind(|s| *s != trimmed_right_delim)
                == Some(trimmed_sep)
        });

        let separator = MergedTree::CommutativeChildSeparator {
            separator: Self::find_separators_with_whitespace(left, trimmed_sep)
                .chain(Self::find_separators_with_whitespace(right, trimmed_sep))
                .chain(Self::find_separators_with_whitespace(base, trimmed_sep))
                // remove the indentation at the end of separators
                // (it will be added back when pretty-printing, possibly at a different level)
                .next()
                .map_or(raw_separator, |separator| {
                    let newline = separator.rfind('\n');
                    match newline {
                        None => separator,
                        Some(index) => &separator[..(index + 1)],
                    }
                }),
        };

        // add delimiters and separators in the merged list
        let mut with_separators = Vec::new();
        if let Some(left_delim) = left_delim {
            with_separators.push(MergedTree::new_exact(
                left_delim,
                self.class_mapping.revision_set(&left_delim),
                self.class_mapping,
            ));
        }
        let mut first = !starts_with_separator;
        for merged in merged_content {
            if first {
                first = false;
            } else {
                with_separators.push(separator.clone());
            }
            with_separators.push(merged);
        }
        if ends_with_separator {
            with_separators.push(separator);
        }
        if let Some(right_delim) = right_delim {
            with_separators.push(MergedTree::new_exact(
                right_delim,
                self.class_mapping.revision_set(&right_delim),
                self.class_mapping,
            ));
        }

        Ok(with_separators)
    }

    /// For a commutative parent, merge its children commutatively.
    /// This extracts the longest prefix and suffix of both sides to avoid re-ordering begin and end markers.
    fn commutatively_merge_children(
        &self,
        leader: &Leader<'a>,
        commutative_parent: &CommutativeParent,
        visiting_state: &mut VisitingState<'a>,
    ) -> Result<Vec<MergedTree<'a>>, TreeBuildingError<'a>> {
        let children_base = self
            .class_mapping
            .children_at_revision(leader, Revision::Base)
            .unwrap_or_default();
        let children_left = self
            .class_mapping
            .children_at_revision(leader, Revision::Left)
            .unwrap_or_default();
        let children_right = self
            .class_mapping
            .children_at_revision(leader, Revision::Right)
            .unwrap_or_default();

        // remove the common prefix of all three
        let common_prefix_length = Self::common_prefix(
            children_base.iter(),
            children_left.iter(),
            children_right.iter(),
        );
        let common_prefix = &children_base[..common_prefix_length];
        let children_base = &children_base[common_prefix_length..];
        let children_left = &children_left[common_prefix_length..];
        let children_right = &children_right[common_prefix_length..];

        // remove the common suffix of all three
        let common_suffix_length = Self::common_prefix(
            children_base.iter().rev(),
            children_left.iter().rev(),
            children_right.iter().rev(),
        );
        let common_suffix = &children_base[children_base.len() - common_suffix_length..];
        let children_base = &children_base[..children_base.len() - common_suffix_length];
        let children_left = &children_left[..children_left.len() - common_suffix_length];
        let children_right = &children_right[..children_right.len() - common_suffix_length];

        // map to ast nodes
        let base = children_base
            .iter()
            .map(|rn| {
                self.class_mapping
                    .node_at_rev(rn, Revision::Base)
                    .expect("inconsistent class mapping for base children of commutative parent")
            })
            .collect_vec();
        let left = children_left
            .iter()
            .map(|rn| {
                self.class_mapping
                    .node_at_rev(rn, Revision::Left)
                    .expect("inconsistent class mapping for left children of commutative parent")
            })
            .collect_vec();
        let right = children_right
            .iter()
            .map(|rn| {
                self.class_mapping
                    .node_at_rev(rn, Revision::Right)
                    .expect("inconsistent class mapping for right children of commutative parent")
            })
            .collect_vec();

        let mut merge_result = self.commutatively_merge_lists(
            &base,
            &left,
            &right,
            commutative_parent,
            visiting_state,
        )?;
        let mut prefix_trees: Vec<_> = common_prefix
            .iter()
            .map(|revnode| {
                self.build_subtree(
                    PCSNode::Node {
                        revisions: self.class_mapping.revision_set(revnode),
                        node: *revnode,
                    },
                    visiting_state,
                )
            })
            .collect::<Result<_, _>>()?;
        let mut suffix_trees: Vec<_> = common_suffix
            .iter()
            .map(|revnode| {
                self.build_subtree(
                    PCSNode::Node {
                        revisions: self.class_mapping.revision_set(revnode),
                        node: *revnode,
                    },
                    visiting_state,
                )
            })
            .collect::<Result<_, _>>()?;

        prefix_trees.append(&mut merge_result);
        prefix_trees.append(&mut suffix_trees);
        Ok(prefix_trees)
    }

    /// Find the common prefix of three lists
    fn common_prefix<T: Eq>(
        first: impl Iterator<Item = T>,
        second: impl Iterator<Item = T>,
        third: impl Iterator<Item = T>,
    ) -> usize {
        first
            .zip(second)
            .zip(third)
            .take_while(|((x, y), z)| x == y && y == z)
            .count()
    }

    /// Given a merged tree, find a set of nodes (descendants of this merged tree)
    /// which exist in the target revision and which completely cover all changes made
    /// to the merged tree in the modifying revision.
    /// This means that all the changes made in the modifying revision must happen
    /// within the subtrees rooted in one of the returned covering nodes.
    /// If there are no changes made in the modifying revision, an empty set is
    /// returned. We attempt to return a set that is as narrow as possible.
    /// If such a covering does not exist, we return None.
    fn cover_modified_nodes(
        &self,
        tree: &MergedTree<'a>,
        target_revision: Revision,
        modifying_revision: Revision,
    ) -> Option<HashSet<Leader<'a>>> {
        match tree {
            MergedTree::ExactTree { revisions, .. } if revisions.contains(Revision::Base) => {
                // the given tree has no changes given that it can be output as the base revision,
                // so the empty set covers the changes
                Some(HashSet::new())
            }
            MergedTree::ExactTree { node, .. } => {
                match self
                    .class_mapping
                    .children_at_revision(node, modifying_revision)
                {
                    Some(children_revnodes) => {
                        let children = children_revnodes
                            .into_iter()
                            .map(|child| {
                                MergedTree::new_exact(
                                    child,
                                    RevisionNESet::singleton(modifying_revision),
                                    self.class_mapping,
                                )
                            })
                            .collect_vec();
                        self.cover_modified_nodes(
                            &MergedTree::new_mixed(*node, children),
                            target_revision,
                            modifying_revision,
                        )
                    }
                    // if the tree doesn't exist at all in the modifying revision,
                    // it does not contain any changes to be covered
                    None => Some(HashSet::new()),
                }
            }
            MergedTree::MixedTree { node, children, .. } => {
                let available_in_revs = self.class_mapping.revision_set(node);
                // compare the list of children on the base and modified revisions,
                // to determine if any change happened at this level.
                // If the children are not available for either revisions (because the node isn't mapped to this revision)
                // then we give up: we cannot find a covering of the modifications in that case.
                let children_base = self
                    .class_mapping
                    .children_at_revision(node, Revision::Base)?;
                let children_modified = self
                    .class_mapping
                    .children_at_revision(node, modifying_revision)?;
                if children_base == children_modified {
                    // the change didn't happen at this level
                    let children_covers: Option<Vec<HashSet<Leader<'a>>>> = children
                        .iter()
                        .map(|child| {
                            self.cover_modified_nodes(child, target_revision, modifying_revision)
                        })
                        .collect();
                    // if all children can be covered then return the union of all children's covers
                    if let Some(children_covers) = children_covers {
                        let union: HashSet<Leader<'a>> =
                            children_covers
                                .into_iter()
                                .fold(HashSet::new(), |mut acc, s| {
                                    acc.extend(s);
                                    acc
                                });
                        return Some(union);
                    }
                    // at least one child could not be covered at all - the root is our only last possibility
                }
                if available_in_revs.contains(target_revision) {
                    Some(HashSet::from([*node]))
                } else {
                    None
                }
            }
            MergedTree::CommutativeChildSeparator { .. } => Some(HashSet::new()), // commutative separators are uninteresting, they don't need covering
            MergedTree::Conflict { .. } | MergedTree::LineBasedMerge { .. } => None,
        }
    }
}

fn fmt_set<S>(s: &HashSet<(Revision, PCSNode<'_>), S>) -> impl Display {
    s.iter()
        .format_with(", ", |(r, n), f| f(&format_args!("({r},{n})")))
}

#[cfg(test)]
mod tests {
    use crate::test_utils::ctx;

    use super::*;

    #[test]
    fn recover_exact_tree() {
        let ctx = ctx();

        let tree = ctx.parse("a.json", "[1, [2, 3]]");

        let class_mapping = ClassMapping::new();
        let mut changeset = ChangeSet::new();
        changeset.add_tree(tree, Revision::Base, &class_mapping);

        let settings = DisplaySettings::default();

        let result_tree = {
            let merged_changeset = &changeset;
            let base_changeset = &changeset;
            let class_mapping = &class_mapping;
            // build the necessary context for the tree-gathering algorithm
            let tree_gatherer =
                TreeBuilder::new(merged_changeset, base_changeset, class_mapping, &settings);
            tree_gatherer.build_tree()
        };

        assert_eq!(
            result_tree,
            Ok(MergedTree::new_exact(
                class_mapping.map_to_leader(RevNode::new(Revision::Base, tree)),
                RevisionNESet::singleton(Revision::Base),
                &class_mapping,
            ))
        );
    }

    #[test]
    fn contains() {
        let ctx = ctx();

        let tree = ctx.parse("a.json", "[1, [2, 3]]");

        let class_mapping = ClassMapping::new();
        let mut changeset = ChangeSet::new();
        changeset.add_tree(tree, Revision::Base, &class_mapping);

        let settings = DisplaySettings::default();

        let result_tree = {
            let merged_changeset = &changeset;
            let base_changeset = &changeset;
            let class_mapping = &class_mapping;
            // build the necessary context for the tree-gathering algorithm
            let tree_gatherer =
                TreeBuilder::new(merged_changeset, base_changeset, class_mapping, &settings);
            tree_gatherer.build_tree()
        }
        .expect("a successful merge was expected");

        assert!(result_tree.contains(
            &class_mapping.map_to_leader(RevNode::new(Revision::Base, tree)),
            &class_mapping
        ));
        assert!(result_tree.contains(
            &class_mapping.map_to_leader(RevNode::new(Revision::Base, tree[0])),
            &class_mapping
        ));
    }
}
