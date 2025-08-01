use std::iter::zip;

use itertools::Itertools;
use log::debug;
use regex::Regex;

use crate::{
    ast::AstNode,
    class_mapping::{ClassMapping, Leader, RevNode},
    lang_profile::CommutativeParent,
    merged_tree::MergedTree,
    pcs::Revision,
    signature::isomorphic_merged_trees,
};

impl<'a> MergedTree<'a> {
    /// Transforms a merged tree by checking that there are no signature conflicts.
    /// If there are any, group the elements with identical signatures in the same location
    /// and potentially add a conflict there.
    pub(crate) fn post_process_for_duplicate_signatures(
        self,
        class_mapping: &ClassMapping<'a>,
    ) -> Self {
        match self {
            Self::MixedTree { node, children, .. } => {
                let recursively_processed = children
                    .into_iter()
                    .map(|element| element.post_process_for_duplicate_signatures(class_mapping))
                    .collect();
                let commutative_parent = node.commutative_parent_definition();
                if let Some(commutative_parent) = commutative_parent {
                    let highlighted = highlight_duplicate_signatures(
                        &node,
                        recursively_processed,
                        class_mapping,
                        commutative_parent,
                    );
                    Self::new_mixed(node, highlighted)
                } else {
                    Self::new_mixed(node, recursively_processed)
                }
            }
            Self::ExactTree { .. }
            | Self::Conflict { .. }
            | Self::LineBasedMerge { .. }
            | Self::CommutativeChildSeparator { .. } => self,
        }
    }
}

/// Checks for duplicate signatures among the children of the given commutative parent.
fn highlight_duplicate_signatures<'a>(
    parent: &Leader<'a>,
    elements: Vec<MergedTree<'a>>,
    class_mapping: &ClassMapping<'a>,
    commutative_parent: &CommutativeParent,
) -> Vec<MergedTree<'a>> {
    // compute signatures and index them
    let sigs: Vec<_> = elements
        .iter()
        .map(|element| element.signature(class_mapping))
        .collect();
    let sig_to_indices = sigs
        .iter()
        .enumerate()
        // filter out `None`s, but keep indices of `Some`s
        .filter_map(|(idx, sig)| sig.as_ref().map(|signature| (signature, idx)))
        .into_group_map();

    let mut conflict_found = false;
    sig_to_indices
        .iter()
        .filter_map(|(signature, indices)| (indices.len() > 1).then_some(signature))
        .for_each(|signature| {
            conflict_found = true;
            debug!(
                "signature conflict found in {}: {}",
                commutative_parent.parent_type(),
                signature
            );
        });
    if !conflict_found {
        return elements;
    }

    // find an example of a separator among the elements to merge
    let trimmed_separator = commutative_parent.trimmed_separator();
    let separator_example = find_separator(parent, trimmed_separator, class_mapping);

    // determine whether the separator should be added at the end of a line
    // TODO this could probably be simplified now that we have line-based conflict printing
    let end_regex = Regex::new("\n[ \t]*$").unwrap();
    let add_separator = {
        if let Some(node) = separator_example {
            let full_source = node.node.source_with_surrounding_whitespace();
            if end_regex.is_match(full_source) {
                AddSeparator::AtEnd
            } else {
                AddSeparator::OnlyInside
            }
        } else {
            AddSeparator::OnlyInside
        }
    };

    // do a first pass to remove the elements which will move to other
    // locations to be grouped with other elements with the same signature
    let mut filtered_elements = Vec::new();
    let mut skip_next_separator = true;
    // NOTE: can't use `itertools::zip_eq` here because it doesn't implement `DoubleEndedIterator`
    // which is needed for `.rev()`. See https://github.com/rust-itertools/itertools/pull/531
    debug_assert_eq!(
        elements.len(),
        sigs.len(),
        "Inconsistent length of signature arrays and elements array"
    );
    for (idx, (element, sig)) in zip(&elements, &sigs).enumerate().rev() {
        match sig {
            None => {
                let is_separator = is_separator(element, trimmed_separator);
                if !(is_separator && skip_next_separator) {
                    filtered_elements.push((idx, is_separator, element));
                }
                skip_next_separator = false;
            }
            Some(signature) => {
                let cluster = sig_to_indices
                    .get(signature)
                    .expect("Signature not indexed in sig_to_indices map");
                skip_next_separator = Some(&idx) != cluster.iter().min();
                if !skip_next_separator {
                    filtered_elements.push((idx, false, element));
                }
            }
        }
    }

    // finally build the merged output
    let mut result = Vec::new();
    skip_next_separator = true;
    for (filtered_idx, (idx, is_separator, element)) in
        filtered_elements.iter().copied().enumerate().rev()
    {
        let sig = sigs
            .get(idx)
            .expect("Inconsistent of length of signature arrays and elements array");
        match sig {
            None => {
                // avoid pushing duplicate separators
                // (created by clustering elements with the same signature together)
                if !(is_separator && skip_next_separator) {
                    result.push(element.clone());
                }
                skip_next_separator = false;
            }
            Some(signature) => {
                let cluster = sig_to_indices
                    .get(signature)
                    .expect("Signature not indexed in sig_to_indices map");
                skip_next_separator = false;
                if cluster.len() == 1 {
                    result.push(element.clone());
                } else {
                    // only add the conflict around the first element of the cluster
                    if Some(&idx) == cluster.iter().min() {
                        let conflict_add_separator = match add_separator {
                            AddSeparator::OnlyInside => AddSeparator::OnlyInside,
                            AddSeparator::AtEnd => {
                                if let Some((_, true, _)) = filtered_elements.get(filtered_idx - 1)
                                {
                                    AddSeparator::AtEnd
                                } else {
                                    AddSeparator::OnlyInside
                                }
                            } /* TODO set to OnlyInside if we are the last content node */
                        };
                        let (mut merged, happy_path) = merge_same_sigs(
                            &cluster
                                .iter()
                                .map(|idx| {
                                    elements
                                        .get(*idx)
                                        .expect("Invalid element index in sig_to_indices")
                                })
                                .collect::<Vec<_>>(),
                            class_mapping,
                            separator_example,
                            conflict_add_separator,
                        );

                        if !happy_path {
                            match add_separator {
                                AddSeparator::OnlyInside => {}
                                AddSeparator::AtEnd => {
                                    if let Some((_, true, _)) =
                                        filtered_elements.get(filtered_idx - 1)
                                    {
                                        skip_next_separator = true;
                                    }
                                }
                            };
                        }
                        result.append(&mut merged);
                    } else {
                        skip_next_separator = true;
                    }
                }
            }
        }
    }
    result
}

/// Check if a merged element is a separator of its commutative parent
fn is_separator(element: &MergedTree, trimmed_separator: &'static str) -> bool {
    match element {
        MergedTree::ExactTree { node, .. } => {
            node.as_representative().node.source.trim() == trimmed_separator
        }
        MergedTree::MixedTree { .. } | MergedTree::Conflict { .. } => false,
        MergedTree::LineBasedMerge { parsed, .. } => {
            // "SAFETY": a separator is like a comma or something,
            // there is no way it can have a conflict
            parsed
                .render_conflictless()
                .is_some_and(|r| r.trim() == trimmed_separator)
        }
        MergedTree::CommutativeChildSeparator { .. } => true,
    }
}

/// Whether to include a separator at the beginning or end of a list,
/// or only between each element
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum AddSeparator {
    /// A,B,C
    OnlyInside,
    /// A,B,C,
    AtEnd,
}

/// Given a list of elements having the same signature, create a conflict highlighting this fact,
/// or if they happen to be isomorphic in the left/right revisions, output them as-is.
///
/// Also return a `bool` indicating whether the latter was the case.
fn merge_same_sigs<'a>(
    elements: &[&MergedTree<'a>],
    class_mapping: &ClassMapping<'a>,
    separator: Option<RevNode<'a>>,
    add_separator: AddSeparator,
) -> (Vec<MergedTree<'a>>, bool) {
    if let &[first, second] = elements {
        if isomorphic_merged_trees(first, second, class_mapping) {
            // The two elements don't just have the same signature, they are actually isomorphic!
            // So let's just deduplicate them.
            return (vec![first.clone()], true);
        }
    }
    let base = filter_by_revision(elements, Revision::Base, class_mapping);
    let left = filter_by_revision(elements, Revision::Left, class_mapping);
    let right = filter_by_revision(elements, Revision::Right, class_mapping);

    if left.len() == right.len()
        && zip(&left, &right).all(|(elem_left, elem_right)| elem_left.isomorphic_to(elem_right))
    {
        let left_revnodes = left
            .iter()
            .map(|ast_node| RevNode::new(Revision::Left, ast_node))
            .collect();
        let v = add_separators(left_revnodes, separator, add_separator)
            .into_iter()
            .map(|rev_node| {
                let leader = class_mapping.map_to_leader(rev_node);
                MergedTree::new_exact(leader, class_mapping.revision_set(&leader), class_mapping)
            })
            .collect();
        (v, false)
    } else {
        let separator = separator.map(|revnode| revnode.node);
        // NOTE: here we're adding the separator (coming from one particular revision)
        // to conflict sides in other revisions too, meaning that the nodes in each conflict
        // side aren't necessarily coming from the corresponding revision. Which is bad,
        // but it's not clear how that can be avoided: it can be that the separator doesn't appear
        // at all in a given revision.
        (
            vec![MergedTree::Conflict {
                base: add_separators(base, separator, add_separator),
                left: add_separators(left, separator, add_separator),
                right: add_separators(right, separator, add_separator),
            }],
            false,
        )
    }
}

/// Get the versions of the merged nodes in the original revisions
fn filter_by_revision<'a>(
    elements: &[&MergedTree<'a>],
    revision: Revision,
    class_mapping: &ClassMapping<'a>,
) -> Vec<&'a AstNode<'a>> {
    elements
        .iter()
        .copied()
        .filter_map(|element| match element {
            MergedTree::ExactTree { node, .. }
            | MergedTree::MixedTree { node, .. }
            | MergedTree::LineBasedMerge { node, .. } => class_mapping.node_at_rev(node, revision),
            MergedTree::Conflict { .. } | MergedTree::CommutativeChildSeparator { .. } => None,
        })
        .collect()
}

/// Insert separators between a list of merged elements
fn add_separators<T: Clone + Copy>(
    elements: Vec<T>,
    separator: Option<T>,
    add_separator: AddSeparator,
) -> Vec<T> {
    if elements.is_empty() {
        return vec![];
    }

    let Some(separator) = separator else {
        // no separators to insert -- the result is identical to the input
        return elements;
    };

    let mut result = Vec::with_capacity(elements.len() * 2); // 1 separator per element

    #[allow(unstable_name_collisions)] // The method is stuck in stabilization limbo, see its issue
    result.extend(elements.into_iter().intersperse(separator));
    if add_separator == AddSeparator::AtEnd {
        result.push(separator);
    }
    result
}

/// Find an example of a separator among the list of children of the parent in all three revisions
fn find_separator<'a>(
    parent: &Leader<'a>,
    trimmed_separator: &'static str,
    class_mapping: &ClassMapping<'a>,
) -> Option<RevNode<'a>> {
    let revs = [Revision::Base, Revision::Left, Revision::Right];
    revs.into_iter()
        .filter_map(|rev| {
            class_mapping
                .node_at_rev(parent, rev)
                .map(|node| (rev, node))
        })
        .flat_map(|(rev, node)| {
            node.children
                .iter()
                .map(move |child| RevNode::new(rev, child))
        })
        .find(|revnode| revnode.node.source.trim() == trimmed_separator)
}
