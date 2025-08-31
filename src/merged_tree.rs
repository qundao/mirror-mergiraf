use std::{
    borrow::Cow,
    collections::HashSet,
    fmt::Display,
    hash::{Hash, Hasher},
    iter,
};

use either::Either;
use itertools::{EitherOrBoth, Itertools};

use crate::{
    ast::AstNode,
    class_mapping::{ClassMapping, Leader, RevNode, RevisionNESet},
    line_based::line_based_merge_parsed,
    parsed_merge::ParsedMerge,
    pcs::Revision,
    settings::DisplaySettings,
    signature::Signature,
};

mod postprocess;
mod print;

/// A merged tree, which can contain a mixture of elements from the original trees,
/// conflict markers, or even new elements inserted by commutative merging to separate them.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MergedTree<'a> {
    /// A tree that exactly matches a part of an original file (possibly present in more than one revision).
    ExactTree {
        /// The subtree from the original revision, represented as a [Leader] of its cluster
        node: Leader<'a>,
        /// The set of revisions from which the source of the tree may be used to generate the merged output.
        /// Note that this is in general smaller than the set of revisions the node is associated with,
        /// because not all such revisions might have isomorphic contents for this node.
        revisions: RevisionNESet,
        /// A precomputed hash value to help with isomorphism detection.
        hash: u64,
    },
    /// A tree that contains a mixture of elements from various revisions.
    MixedTree {
        /// The root node of this tree, which corresponds to a node present in some of the original files
        node: Leader<'a>,
        /// The children of this root, which can be any sorts of merged trees themselves
        children: Vec<MergedTree<'a>>,
        /// A precomputed hash value to help with isomorphism detection.
        hash: u64,
    },
    /// A conflict which needs to be resolved manually by the user
    Conflict(Conflict<'a>),
    /// A part of the merged result which was obtained by running line-based
    /// merging on a part of the file. This happens in many different situations when
    /// structured merging encounters an error of some sort.
    /// The result may or may not contain conflicts.
    LineBasedMerge {
        /// The syntactic node which corresponds to this part of the file
        node: Leader<'a>,
        /// The result of the line-based merging
        parsed: ParsedMerge<'a>,
    },
    /// A synthetic part of the merged output, not taken from any revision, added
    /// to separate merged children of a commutative parent.
    CommutativeChildSeparator { separator: &'a str },
}

/// The inner struct of [`MergedTree::Conflict`]
///
/// For now, used only to allow returning _exactly_ a conflict from
/// [`TreeBuilder::build_conflict`], effectively being a [pattern type]
///
/// [`TreeBuilder::build_conflict`]: crate::tree_builder::TreeBuilder::build_conflict
/// [pattern type]: https://github.com/rust-lang/types-team/issues/126
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct Conflict<'a> {
    /// The list of nodes in the base revision
    pub(crate) base: Vec<&'a AstNode<'a>>,
    /// The list of nodes in the left revision
    pub(crate) left: Vec<&'a AstNode<'a>>,
    /// The list of nodes in the right revision
    pub(crate) right: Vec<&'a AstNode<'a>>,
}

impl<'a> MergedTree<'a> {
    /// Creates a new exact tree, taking care of the pre-computation of the hash
    pub(crate) fn new_exact(
        node: Leader<'a>,
        revisions: RevisionNESet,
        class_mapping: &ClassMapping<'a>,
    ) -> Self {
        let representative = class_mapping
            .node_at_rev(&node, revisions.any())
            .expect("Revision set for ExactTree inconsistent with class mapping");
        Self::ExactTree {
            node,
            revisions,
            hash: representative.hash,
        }
    }

    /// Creates a new mixed tree, taking care of the pre-computation of the hash
    pub(crate) fn new_mixed(node: Leader<'a>, children: Vec<Self>) -> Self {
        // NOTE: we allow creating a mixed tree without children, because trying to do otherwise
        // turned out to be very much not worth it: https://codeberg.org/mergiraf/mergiraf/pulls/326
        let mut hasher = crate::fxhasher();
        node.kind().hash(&mut hasher);
        node.lang_profile().hash(&mut hasher);
        children
            .iter()
            .map(|child| match child {
                Self::ExactTree { hash, .. } | Self::MixedTree { hash, .. } => *hash,
                Self::Conflict { .. } => 1,
                Self::LineBasedMerge { .. } => 2,
                Self::CommutativeChildSeparator { .. } => 3,
            })
            .collect_vec()
            .hash(&mut hasher);
        Self::MixedTree {
            node,
            children,
            hash: hasher.finish(),
        }
    }

    /// Creates a new conflict, or a list of exact nodes if the conflict is spurious
    pub(crate) fn new_conflict(
        base: Vec<&'a AstNode<'a>>,
        left: Vec<&'a AstNode<'a>>,
        right: Vec<&'a AstNode<'a>>,
        class_mapping: &ClassMapping<'a>,
    ) -> Either<iter::Once<Self>, impl Iterator<Item = Self>> {
        let isomorphic_sides = |first_side: &[&'a AstNode<'a>], second_side: &[&'a AstNode<'a>]| {
            first_side.len() == second_side.len()
                && iter::zip(first_side.iter(), second_side.iter())
                    .all(|(first, second)| first.isomorphic_to(second))
        };

        fn extract_rev<'a>(
            first_side: Vec<&'a AstNode<'a>>,
            first_rev: Revision,
            second_rev: Revision,
            class_mapping: &ClassMapping<'a>,
        ) -> impl Iterator<Item = MergedTree<'a>> {
            first_side.into_iter().map(move |l| {
                MergedTree::new_exact(
                    class_mapping.map_to_leader(RevNode::new(first_rev, l)),
                    RevisionNESet::singleton(first_rev).with(second_rev),
                    class_mapping,
                )
            })
        }

        if isomorphic_sides(&left, &right) {
            Either::Right(extract_rev(
                left,
                Revision::Left,
                Revision::Right,
                class_mapping,
            ))
        } else if isomorphic_sides(&base, &right) {
            Either::Right(extract_rev(
                left,
                Revision::Left,
                Revision::Left,
                class_mapping,
            ))
        } else if isomorphic_sides(&base, &left) {
            Either::Right(extract_rev(
                right,
                Revision::Right,
                Revision::Right,
                class_mapping,
            ))
        } else {
            Either::Left(iter::once(MergedTree::Conflict(Conflict {
                base,
                left,
                right,
            })))
        }
    }

    /// Determines with which field of its parent this node is associated
    pub(crate) fn field_name(&self, class_mapping: &ClassMapping<'a>) -> Option<&'static str> {
        match self {
            Self::ExactTree { node, .. }
            | Self::LineBasedMerge { node, .. }
            | Self::MixedTree { node, .. } => class_mapping.field_name(node),
            Self::Conflict { .. } | Self::CommutativeChildSeparator { .. } => None,
        }
    }

    /// The `kind` of the underlying AST node, if any.
    pub(crate) fn kind(&self) -> Option<&'static str> {
        match self {
            Self::ExactTree { node, .. }
            | Self::LineBasedMerge { node, .. }
            | Self::MixedTree { node, .. } => Some(node.kind()),
            Self::Conflict { .. } | Self::CommutativeChildSeparator { .. } => None,
        }
    }

    /// Generates a line-based merge for a node across multiple revisions.
    pub(crate) fn line_based_local_fallback_for_revnode(
        node: Leader<'a>,
        class_mapping: &ClassMapping<'a>,
        settings: &DisplaySettings,
    ) -> Self {
        let base_src = class_mapping.node_at_rev(&node, Revision::Base);
        let left_src = class_mapping.node_at_rev(&node, Revision::Left);
        let right_src = class_mapping.node_at_rev(&node, Revision::Right);
        match (base_src, left_src, right_src) {
            (None, None, None) => {
                unreachable!("A node that does not belong to any revision, how curious!")
            }
            (_, Some(_), None) => Self::new_exact(
                node,
                RevisionNESet::singleton(Revision::Left),
                class_mapping,
            ),
            (_, None, Some(_)) => Self::new_exact(
                node,
                RevisionNESet::singleton(Revision::Right),
                class_mapping,
            ),
            (Some(_), None, None) => Self::new_exact(
                node,
                RevisionNESet::singleton(Revision::Base),
                class_mapping,
            ),
            (_, Some(left), Some(right)) if left.isomorphic_to(right) => Self::new_exact(
                node,
                RevisionNESet::singleton(Revision::Left).with(Revision::Right),
                class_mapping,
            ),
            (base, Some(left), Some(right)) => {
                #[allow(clippy::redundant_closure_for_method_calls)] // for symmetry with next lines
                let base_src = base.map_or(Cow::from(""), |base| base.unindented_source());
                let left_src = left.unindented_source();
                let right_src = right.unindented_source();
                let line_based_merge =
                    line_based_merge_parsed(&base_src, &left_src, &right_src, settings);
                Self::LineBasedMerge {
                    node,
                    parsed: line_based_merge,
                }
            }
        }
    }

    /// 'Degrade' the merge by adding line-based conflicts for all subtrees rooted in the supplied nodes
    pub(crate) fn force_line_based_fallback_on_specific_nodes(
        self,
        nodes: &HashSet<Leader<'a>>,
        class_mapping: &ClassMapping<'a>,
        settings: &DisplaySettings,
    ) -> Self {
        if nodes.is_empty() {
            // no nodes to force line-based fallback on
            return self;
        }

        match self {
            Self::ExactTree { node, .. } | Self::MixedTree { node, .. }
                if nodes.contains(&node) =>
            {
                Self::line_based_local_fallback_for_revnode(node, class_mapping, settings)
            }
            Self::ExactTree {
                node, revisions, ..
            } => {
                let picked_revision = revisions.any();
                let children = class_mapping
                    .children_at_revision(&node, picked_revision)
                    .expect("non-existent children for revision in revset of ExactTree");
                let cloned_children: Vec<MergedTree<'a>> = children
                    .into_iter()
                    .map(|c| {
                        Self::new_exact(c, revisions, class_mapping)
                            .force_line_based_fallback_on_specific_nodes(
                                nodes,
                                class_mapping,
                                settings,
                            )
                    })
                    .collect();
                if cloned_children
                    .iter()
                    .all(|child| matches!(child, Self::ExactTree { .. }))
                {
                    self
                } else {
                    Self::new_mixed(node, cloned_children)
                }
            }
            Self::MixedTree { node, children, .. } => {
                let cloned_children = children
                    .into_iter()
                    .map(|c| {
                        c.force_line_based_fallback_on_specific_nodes(
                            nodes,
                            class_mapping,
                            settings,
                        )
                    })
                    .collect();
                Self::new_mixed(node, cloned_children)
            }
            _ => self,
        }
    }

    /// Checks if a particular node is contained in the result tree
    pub fn contains(&self, leader: &Leader<'a>, class_mapping: &ClassMapping<'a>) -> bool {
        match self {
            Self::ExactTree {
                node, revisions, ..
            } => {
                let picked_revision = revisions.any();
                let ast_node = class_mapping.node_at_rev(node, picked_revision).expect(
                    "inconsistency between revision set of ExactTree and the class mapping",
                );
                let chosen_revnode = RevNode::new(picked_revision, ast_node);
                chosen_revnode.contains(leader, class_mapping)
            }
            Self::MixedTree { node, children, .. } => {
                node == leader || children.iter().any(|c| c.contains(leader, class_mapping))
            }
            // TODO here we could look for all representatives in their corresponding conflict side, that would be more accurate.
            Self::Conflict(conflict) => match leader.as_representative().rev {
                Revision::Base => (conflict.base)
                    .iter()
                    .any(|n| RevNode::new(Revision::Base, n).contains(leader, class_mapping)),
                Revision::Left => (conflict.left)
                    .iter()
                    .any(|n| RevNode::new(Revision::Left, n).contains(leader, class_mapping)),
                Revision::Right => (conflict.right)
                    .iter()
                    .any(|n| RevNode::new(Revision::Right, n).contains(leader, class_mapping)),
            },
            Self::LineBasedMerge { node, .. } => node == leader,
            Self::CommutativeChildSeparator { .. } => false,
        }
    }

    /// Checks if the merged tree is isomorphic to a parsed source,
    /// when considered at a particular revision.
    /// This is used as a safety check to make sure that the rendered
    /// version of this merge (which is then re-parsed) is faithful to
    /// the intended merge structure, as a means of detecting invalid
    /// whitespace generation or merges that are syntactically invalid.
    pub fn isomorphic_to_source<'b>(
        &'a self,
        other_node: &'b AstNode<'b>,
        revision: Revision,
        class_mapping: &ClassMapping<'a>,
    ) -> bool {
        match self {
            MergedTree::ExactTree {
                node, revisions, ..
            } => {
                let ast_node = class_mapping.node_at_rev(node, revisions.any()).expect(
                    "inconsistency between revision set of ExactTree and the class mapping",
                );
                ast_node.isomorphic_to(other_node)
            }
            MergedTree::MixedTree { node, children, .. } => {
                if node.kind() != other_node.kind || node.lang_profile() != other_node.lang_profile
                {
                    return false;
                }
                // If one of the children is a line-based merge, we just give up
                // and assume that the nodes are isomorphic. This is because
                // the line-based merge might contain any number of actual children,
                // so we are unable to match the other children together.
                // It would be better to re-parse the textual merge, but that would assume
                // the ability to parse a snippet of text for a particular node type, which
                // is not supported by tree-sitter yet:
                // https://github.com/tree-sitter/tree-sitter/issues/711
                let contains_line_based_merge = children
                    .iter()
                    .any(|c| matches!(c, MergedTree::LineBasedMerge { .. }));
                if contains_line_based_merge {
                    return true;
                }
                let children_at_rev = children
                    .iter()
                    .flat_map(|child| match child {
                        MergedTree::LineBasedMerge { .. } => {
                            unreachable!(
                                "line-based merge should have been caught by the earlier filter"
                            )
                        }
                        MergedTree::Conflict (conflict) => {
                            let nodes = match revision {
                                Revision::Base => &conflict.base,
                                Revision::Left => &conflict.left,
                                Revision::Right => &conflict.right,
                            };
                            nodes.iter().copied().map(MergedChild::Original).collect()
                        }
                        _ => {
                            vec![MergedChild::Merged(child)]
                        }
                    })
                    .filter(|child| {
                        // filter out nodes which wouldn't be present in a parsed tree
                        // so as not to create a mismatch in the number of children
                        match child {
                            MergedChild::Original(child) => {
                                !child.source.is_empty()
                            },
                            MergedChild::Merged(MergedTree::CommutativeChildSeparator {
                                separator,
                            }) => !separator.trim().is_empty(),
                            MergedChild::Merged(MergedTree::MixedTree { children, .. }) => {
                                !children.is_empty()
                            },
                            MergedChild::Merged(MergedTree::ExactTree { node, revisions, .. }) => {
                                let node = class_mapping.node_at_rev(node, revisions.any()).expect(
                                    "inconsistency between revision set of ExactTree and the class mapping",
                                );
                                !node.source.is_empty()
                            }
                            _ => true,
                        }
                    });
                // also filter empty nodes from the newly parsed tree, for consistency with above.
                // See `examples/go.mod/working/duplicate_ignore_directives` for an integration test.
                let filtered_other_children = other_node
                    .children
                    .iter()
                    .filter(|child| !child.source.is_empty());

                children_at_rev
                    .zip_longest(filtered_other_children)
                    .all(|pair| {
                        if let EitherOrBoth::Both(child, other_child) = pair {
                            match child {
                                MergedChild::Merged(merged_tree) => merged_tree
                                    .isomorphic_to_source(other_child, revision, class_mapping),
                                MergedChild::Original(ast_node) => {
                                    ast_node.isomorphic_to(other_child)
                                }
                            }
                        } else {
                            false
                        }
                    })
            }
            MergedTree::LineBasedMerge { .. } => {
                // See above
                true
            }
            MergedTree::Conflict { .. } => {
                // Conflict is only allowed to appear as a child of another node, in which case
                // it will be flattened above
                false
            }
            MergedTree::CommutativeChildSeparator { separator } => {
                separator.trim() == other_node.source.trim()
            }
        }
    }

    fn pretty_print_astnode_list(_revision: Revision, list: &[&'a AstNode<'a>]) -> String {
        let mut output = String::new();
        let mut first = true;
        for n in list {
            let whitespace = n.preceding_whitespace().unwrap_or("");
            if first {
                first = false;
            } else {
                output.push_str(whitespace);
            }
            output.push_str(n.source);
        }
        output
    }

    /// Debug print with indentation
    fn debug_print(&self, indentation: usize) -> String {
        let mut result = " ".repeat(indentation);
        let c = match self {
            Self::ExactTree {
                node, revisions, ..
            } => format!("Exact({node}{revisions})"),
            Self::MixedTree { node, children, .. } => {
                let children_printed = children
                    .iter()
                    .map(|c| c.debug_print(indentation + 2))
                    .format("\n");
                format!("Mixed({node}\n{children_printed}\n{result})")
            }
            Self::Conflict { .. } => "Conflict()".to_string(),
            Self::LineBasedMerge { .. } => "LineBasedConflict()".to_string(),
            Self::CommutativeChildSeparator { separator } => {
                format!("CommutativeChildSeparator({})", separator.escape_debug())
            }
        };
        result.push_str(&c);
        result
    }

    /// Extracts a signature for the given node if there is a signature definition
    /// for this type of nodes in the language profile.
    pub(crate) fn signature<'b>(
        &'b self,
        class_mapping: &ClassMapping<'a>,
    ) -> Option<Signature<'b, 'a>> {
        let definition = match self {
            MergedTree::ExactTree { node, .. }
            | MergedTree::MixedTree { node, .. }
            | MergedTree::LineBasedMerge { node, .. } => node.signature_definition(),
            MergedTree::Conflict { .. } | MergedTree::CommutativeChildSeparator { .. } => None,
        }?;
        let signature = definition.extract_signature_from_merged_node(self, class_mapping);
        Some(signature)
    }
}

impl Display for MergedTree<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.debug_print(0))
    }
}

/// Represents a child from a MergedTree::MixedTree
/// where any conflicts have been replaced by their version in
/// one revision.
/// Only used internally in `MergedTree::isomorphic_to_source`.
enum MergedChild<'a, 'b> {
    Merged(&'a MergedTree<'a>),
    Original(&'b AstNode<'b>),
}

#[cfg(test)]
mod test {
    use super::*;

    use crate::{
        merge_3dm::three_way_merge,
        test_utils::{ctx, json_matchers},
    };

    #[test]
    fn debug_print() {
        let ctx = ctx();
        let base = ctx.parse("a.json", "[1, 1]");
        let left = ctx.parse("a.json", "[1, 2]");
        let right = ctx.parse("a.json", "[2, 1]");

        let (primary_matcher, auxiliary_matcher) = json_matchers();
        let (merged_tree, _) = three_way_merge(
            base,
            left,
            right,
            None,
            &primary_matcher,
            &auxiliary_matcher,
            &DisplaySettings::default(),
            None,
        );
        assert_eq!(
            merged_tree.to_string(),
            "\
Mixed(document:0…6@Base
  Mixed(array:0…6@Base
    Exact([:0…1@Base/BLR/)
    Exact(number:1…2@Right/..R/)
    Exact(,:2…3@Base/BLR/)
    Exact(number:4…5@Left/.L./)
    Exact(]:5…6@Base/BLR/)
  )
)"
        );
    }
}
