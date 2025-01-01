use std::collections::HashMap;

use log::debug;
use regex::Regex;

use crate::{
    class_mapping::{ClassMapping, Leader, RevNode, RevisionNESet},
    lang_profile::{CommutativeParent, LangProfile},
    merged_tree::MergedTree,
    pcs::Revision,
    signature::{isomorphic_merged_trees, Signature},
    tree::AstNode,
};

/// Transforms a merged tree by checking that there are no signature conflicts.
/// If there are any, group the elements with identical signatures in the same location
/// and potentially add a conflict there.
pub(crate) fn post_process_merged_tree_for_duplicate_signatures<'a>(
    tree: MergedTree<'a>,
    lang_profile: &LangProfile,
    class_mapping: &ClassMapping<'a>,
) -> MergedTree<'a> {
    match tree {
        MergedTree::MixedTree { node, children, .. } => {
            let recursively_processed = children
                .into_iter()
                .map(|element| {
                    post_process_merged_tree_for_duplicate_signatures(
                        element,
                        lang_profile,
                        class_mapping,
                    )
                })
                .collect();
            let commutative_parent = lang_profile.get_commutative_parent(node.grammar_name());
            if let Some(commutative_parent) = commutative_parent {
                let highlighted = highlight_duplicate_signatures(
                    node,
                    recursively_processed,
                    lang_profile,
                    class_mapping,
                    commutative_parent,
                );
                MergedTree::new_mixed(node, highlighted)
            } else {
                MergedTree::new_mixed(node, recursively_processed)
            }
        }
        MergedTree::ExactTree { .. }
        | MergedTree::Conflict { .. }
        | MergedTree::LineBasedMerge { .. }
        | MergedTree::CommutativeChildSeparator { .. } => tree,
    }
}

/// Checks for duplicate signatures among the children of the given commutative parent.
fn highlight_duplicate_signatures<'a>(
    parent: Leader<'a>,
    elements: Vec<MergedTree<'a>>,
    lang_profile: &LangProfile,
    class_mapping: &ClassMapping<'a>,
    commutative_parent: &CommutativeParent,
) -> Vec<MergedTree<'a>> {
    // compute signatures and index them
    let mut sig_to_indices: HashMap<&Signature<'_, 'a>, Vec<usize>> = HashMap::new();
    let mut conflict_found = false;
    let sigs: Vec<_> = elements
        .iter()
        .map(|element| lang_profile.extract_signature_from_merged_node(element, class_mapping))
        .collect();
    for (idx, sig) in sigs.iter().enumerate() {
        if let Some(signature) = sig {
            let existing_indices = sig_to_indices.entry(signature).or_default();
            if !existing_indices.is_empty() {
                conflict_found = true;
                debug!(
                    "signature conflict found in {}: {}",
                    commutative_parent.parent_type, signature
                );
            }
            existing_indices.push(idx);
        }
    }
    if !conflict_found {
        return elements;
    }

    // find an example of a separator among the elements to merge
    let trimmed_separator = commutative_parent.separator.trim();
    let separator_example = find_separator(parent, trimmed_separator, class_mapping);
    let separator_node = separator_example.map(|revnode| revnode.node);

    // determine whether the separator should be added at the beginning of a line or rather at the end
    // TODO this could probably be simplified now that we have line-based conflict printing
    let start_regex = Regex::new("^[ \t]*\n").unwrap();
    let end_regex = Regex::new("\n[ \t]*$").unwrap();
    let add_separator = {
        if let Some(node) = separator_example {
            let full_source = node.node.source_with_surrounding_whitespace();
            if start_regex.find(full_source).is_some() {
                AddSeparator::AtBeginning
            } else if end_regex.find(full_source).is_some() {
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
    for (idx, (element, sig)) in std::iter::zip(&elements, &sigs).enumerate().rev() {
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
    let mut latest_element_is_separator = false;
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
                latest_element_is_separator = is_separator;
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
                            AddSeparator::AtBeginning => {
                                if latest_element_is_separator {
                                    result.pop();
                                    AddSeparator::AtBeginning
                                } else {
                                    AddSeparator::OnlyInside
                                }
                            }
                            AddSeparator::AtEnd => {
                                if let Some((_, true, _)) = filtered_elements.get(filtered_idx - 1)
                                {
                                    skip_next_separator = true;
                                    AddSeparator::AtEnd
                                } else {
                                    AddSeparator::OnlyInside
                                }
                            } /* TODO set to OnlyInside if we are the last content node */
                        };
                        let mut merged = merge_same_sigs(
                            &cluster
                                .iter()
                                .map(|idx| {
                                    elements
                                        .get(*idx)
                                        .expect("Invalid element index in sig_to_indices")
                                })
                                .collect::<Vec<_>>(),
                            class_mapping,
                            separator_node,
                            conflict_add_separator,
                        );
                        result.append(&mut merged);
                    } else {
                        skip_next_separator = true;
                    }
                }
                latest_element_is_separator = false;
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
        MergedTree::LineBasedMerge { contents, .. } => contents.trim() == trimmed_separator,
        MergedTree::CommutativeChildSeparator { .. } => true,
    }
}

/// Whether to include a separator at the beginning or end of a list,
/// or only between each element
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum AddSeparator {
    OnlyInside,
    AtBeginning,
    AtEnd,
}

/// Given a list of elements having the same signature, create a conflict highlighting this fact,
/// or if they happen to be isomorphic in the left/right revisions, output them as-is.
fn merge_same_sigs<'a>(
    elements: &[&MergedTree<'a>],
    class_mapping: &ClassMapping<'a>,
    separator: Option<&'a AstNode<'a>>,
    add_separator: AddSeparator,
) -> Vec<MergedTree<'a>> {
    if let &[first, second] = elements {
        if isomorphic_merged_trees(first, second, class_mapping) {
            // The two elements don't just have the same signature, they are actually isomorphic!
            // So let's just deduplicate them.
            return vec![first.clone()];
        }
    }
    let base = filter_by_revision(elements, Revision::Base, class_mapping);
    let left = filter_by_revision(elements, Revision::Left, class_mapping);
    let right = filter_by_revision(elements, Revision::Right, class_mapping);

    if left.len() == right.len()
        && left
            .iter()
            .zip(right.iter())
            .all(|(elem_left, elem_right)| elem_left.isomorphic_to(elem_right))
    {
        add_separators(&left, separator, add_separator)
            .iter()
            .map(|ast_node| {
                MergedTree::new_exact(
                    class_mapping.map_to_leader(RevNode::new(Revision::Left, ast_node)),
                    RevisionNESet::singleton(Revision::Left).with(Revision::Right),
                    class_mapping,
                )
            })
            .collect()
    } else {
        vec![MergedTree::Conflict {
            base: add_separators(&base, separator, add_separator),
            left: add_separators(&left, separator, add_separator),
            right: add_separators(&right, separator, add_separator),
        }]
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
            | MergedTree::LineBasedMerge { node, .. } => class_mapping.node_at_rev(*node, revision),
            MergedTree::Conflict { .. } | MergedTree::CommutativeChildSeparator { .. } => None,
        })
        .collect()
}

/// Insert separators between a list of merged elements
fn add_separators<'a>(
    elements: &[&'a AstNode<'a>],
    separator: Option<&'a AstNode<'a>>,
    add_separator: AddSeparator,
) -> Vec<&'a AstNode<'a>> {
    let mut first = true;
    let mut result = Vec::new();
    if let Some(separator) = separator {
        if !elements.is_empty() && add_separator == AddSeparator::AtBeginning {
            result.push(separator);
        }
    }
    for element in elements {
        if first {
            first = false;
        } else if let Some(separator) = separator {
            result.push(separator);
        }
        result.push(element);
    }
    if let Some(separator) = separator {
        if !elements.is_empty() && add_separator == AddSeparator::AtEnd {
            result.push(separator);
        }
    }
    result
}

/// Find an example of a separator among the list of children of the parent in all three revisions
fn find_separator<'a>(
    parent: Leader<'a>,
    trimmed_separator: &'static str,
    class_mapping: &ClassMapping<'a>,
) -> Option<RevNode<'a>> {
    let revs = [Revision::Base, Revision::Left, Revision::Right];
    revs.iter()
        .filter_map(|rev| {
            class_mapping
                .node_at_rev(parent, *rev)
                .map(|node| (*rev, node))
        })
        .flat_map(|(rev, node)| {
            node.children
                .iter()
                .map(move |child| RevNode::new(rev, child))
        })
        .find(|revnode| revnode.node.source.trim() == trimmed_separator)
}
