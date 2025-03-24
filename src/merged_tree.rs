use std::{
    borrow::Cow,
    collections::HashSet,
    fmt::Display,
    hash::{Hash, Hasher},
};

use itertools::Itertools;

use crate::{
    class_mapping::{ClassMapping, Leader, RevNode, RevisionNESet},
    line_based::line_based_merge,
    merged_text::MergedText,
    pcs::Revision,
    settings::DisplaySettings,
    tree::AstNode,
};

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
    Conflict {
        /// The list of nodes in the base revision
        base: Vec<&'a AstNode<'a>>,
        /// The list of nodes in the left revision
        left: Vec<&'a AstNode<'a>>,
        /// The list of nodes in the right revision
        right: Vec<&'a AstNode<'a>>,
    },
    /// A part of the merged result which was obtained by running line-based
    /// merging on a part of the file. This happens in many different situations when
    /// structured merging encounters an error of some sort.
    /// The result may or may not contain conflicts.
    LineBasedMerge {
        /// The syntactic node which corresponds to this part of the file
        node: Leader<'a>,
        /// The result of the line-based merging
        contents: String,
        /// The size of the conflicts included in this merge output
        conflict_mass: usize,
    },
    /// A synthetic part of the merged output, not taken from any revision, added
    /// to separate merged children of a commutative parent.
    CommutativeChildSeparator { separator: &'a str },
}

#[derive(Debug, Clone)]
enum PreviousSibling<'a> {
    RealNode(Leader<'a>),
    CommutativeSeparator(&'a str),
}

impl<'a> MergedTree<'a> {
    /// Creates a new exact tree, taking care of the pre-computation of the hash
    pub(crate) fn new_exact(
        node: Leader<'a>,
        revisions: RevisionNESet,
        class_mapping: &ClassMapping<'a>,
    ) -> Self {
        let representative = class_mapping
            .node_at_rev(node, revisions.any())
            .expect("Revision set for ExactTree inconsistent with class mapping");
        Self::ExactTree {
            node,
            revisions,
            hash: representative.hash,
        }
    }

    /// Creates a new mixed tree, taking care of the pre-computation of the hash
    pub(crate) fn new_mixed(node: Leader<'a>, children: Vec<Self>) -> Self {
        // TODO we could refuse to create a new mixed tree with no children
        let mut hasher = crate::fxhasher();
        node.grammar_name().hash(&mut hasher);
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

    /// Determines with which field of its parent this node is associated
    pub(crate) fn field_name(&self, class_mapping: &ClassMapping<'a>) -> Option<&'static str> {
        match self {
            Self::ExactTree { node, .. }
            | Self::LineBasedMerge { node, .. }
            | Self::MixedTree { node, .. } => class_mapping.field_name(*node),
            Self::Conflict { .. } | Self::CommutativeChildSeparator { .. } => None,
        }
    }

    /// The `grammar_name` of the underlying AST node, if any.
    pub(crate) fn grammar_name(&self) -> Option<&'static str> {
        match self {
            Self::ExactTree { node, .. }
            | Self::LineBasedMerge { node, .. }
            | Self::MixedTree { node, .. } => Some(node.grammar_name()),
            Self::Conflict { .. } | Self::CommutativeChildSeparator { .. } => None,
        }
    }

    /// Generates a line-based merge for a node across multiple revisions.
    pub(crate) fn line_based_local_fallback_for_revnode(
        node: Leader<'a>,
        class_mapping: &ClassMapping<'a>,
        settings: &DisplaySettings,
    ) -> Self {
        let base_src = class_mapping.node_at_rev(node, Revision::Base);
        let left_src = class_mapping.node_at_rev(node, Revision::Left);
        let right_src = class_mapping.node_at_rev(node, Revision::Right);
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
                let line_based_merge = line_based_merge(&base_src, &left_src, &right_src, settings);
                Self::LineBasedMerge {
                    node,
                    contents: line_based_merge.contents,
                    conflict_mass: line_based_merge.conflict_mass,
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
            Self::ExactTree {
                node, revisions, ..
            } => {
                if nodes.contains(&node) {
                    Self::line_based_local_fallback_for_revnode(node, class_mapping, settings)
                } else {
                    let picked_revision = revisions.any();
                    let children = class_mapping
                        .children_at_revision(node, picked_revision)
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
            }
            Self::MixedTree { node, children, .. } => {
                if nodes.contains(&node) {
                    Self::line_based_local_fallback_for_revnode(node, class_mapping, settings)
                } else {
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
            }
            _ => self,
        }
    }

    /// Checks if a particular node is contained in the result tree
    pub fn contains(&self, leader: Leader<'a>, class_mapping: &ClassMapping<'a>) -> bool {
        match self {
            Self::ExactTree {
                node, revisions, ..
            } => {
                let picked_revision = revisions.any();
                let ast_node = class_mapping.node_at_rev(*node, picked_revision).expect(
                    "inconsistency between revision set of ExactTree and the class mapping",
                );
                let chosen_revnode = RevNode::new(picked_revision, ast_node);
                chosen_revnode.contains(&leader, class_mapping)
            }
            Self::MixedTree { node, children, .. } => {
                *node == leader || children.iter().any(|c| c.contains(leader, class_mapping))
            }
            // TODO here we could look for all representatives in their corresponding conflict side, that would be more accurate.
            Self::Conflict { base, left, right } => match leader.as_representative().rev {
                Revision::Base => base
                    .iter()
                    .any(|n| RevNode::new(Revision::Base, n).contains(&leader, class_mapping)),
                Revision::Left => left
                    .iter()
                    .any(|n| RevNode::new(Revision::Left, n).contains(&leader, class_mapping)),
                Revision::Right => right
                    .iter()
                    .any(|n| RevNode::new(Revision::Right, n).contains(&leader, class_mapping)),
            },
            Self::LineBasedMerge { node, .. } => *node == leader,
            Self::CommutativeChildSeparator { .. } => false,
        }
    }

    /// Pretty-prints the result tree into its final output. Exciting!
    pub fn pretty_print<'u: 'a>(
        &'u self,
        class_mapping: &ClassMapping<'a>,
        settings: &DisplaySettings,
    ) -> String {
        let mut output = MergedText::new();
        self.pretty_print_recursively(&mut output, class_mapping, None, "", settings);
        output.render(settings)
    }

    /// Recursively pretty-prints a sub part of the result tree.
    fn pretty_print_recursively<'u: 'a>(
        &'u self,
        output: &mut MergedText<'a>,
        class_mapping: &ClassMapping<'a>,
        previous_sibling: Option<&PreviousSibling<'a>>,
        indentation: &str,
        settings: &DisplaySettings,
    ) {
        match self {
            Self::ExactTree {
                node, revisions, ..
            } => {
                let picked_revision = revisions.any();
                let tree_at_rev = class_mapping
                    .node_at_rev(*node, picked_revision)
                    .expect("specified revision is not available for class leader");
                let new_indentation = Self::add_preceding_whitespace(
                    output,
                    *node,
                    previous_sibling,
                    indentation,
                    class_mapping,
                );
                output.push_merged(tree_at_rev.reindented_source(&new_indentation).into());
            }
            Self::MixedTree {
                node: leader,
                children,
                ..
            } => {
                let new_indentation = Self::add_preceding_whitespace(
                    output,
                    *leader,
                    previous_sibling,
                    indentation,
                    class_mapping,
                );
                let mut previous_sibling = None;
                for c in children {
                    c.pretty_print_recursively(
                        output,
                        class_mapping,
                        previous_sibling.as_ref(),
                        &new_indentation,
                        settings,
                    );
                    previous_sibling = match *c {
                        Self::ExactTree { node, .. }
                        | Self::MixedTree { node, .. }
                        | Self::LineBasedMerge { node, .. } => {
                            Some(PreviousSibling::RealNode(node))
                        }
                        Self::Conflict { .. } => None,
                        Self::CommutativeChildSeparator { separator } => {
                            Some(PreviousSibling::CommutativeSeparator(separator))
                        }
                    };
                }

                if let Some(whitespace) = Self::trailing_whitespace(*leader, class_mapping) {
                    output.push_merged(Cow::from(whitespace));
                }
            }
            Self::Conflict { base, left, right } => {
                if base.is_empty() && left.is_empty() && right.is_empty() {
                    return;
                }
                let first_leader = [
                    (left.first(), Revision::Left),
                    (right.first(), Revision::Right),
                    (base.first(), Revision::Base),
                ]
                .into_iter()
                .find_map(|(maybe_node, rev)| {
                    maybe_node.map(|node| class_mapping.map_to_leader(RevNode::new(rev, node)))
                })
                .expect("The conflict should contain at least one node");
                Self::add_preceding_whitespace(
                    output,
                    first_leader,
                    previous_sibling,
                    indentation,
                    class_mapping,
                );
                // TODO reindent??
                output.push_conflict(
                    Self::pretty_print_astnode_list(Revision::Base, base).into(),
                    Self::pretty_print_astnode_list(Revision::Left, left).into(),
                    Self::pretty_print_astnode_list(Revision::Right, right).into(),
                );
            }
            Self::LineBasedMerge { contents, node, .. } => {
                if contents.is_empty() {
                    return;
                }
                Self::add_preceding_whitespace(
                    output,
                    *node,
                    previous_sibling,
                    indentation,
                    class_mapping,
                );
                let full_indentation = format!(
                    "{}{}",
                    indentation,
                    node.as_representative()
                        .node
                        .indentation_shift()
                        .unwrap_or("")
                );
                output.push_line_based_merge(contents, &full_indentation, settings);
            }
            Self::CommutativeChildSeparator { separator, .. } => {
                output.push_merged(Cow::from(*separator));
            }
        }
    }

    /// Adds any preceding whitespace before pretty-printing a node.
    fn add_preceding_whitespace<'b>(
        output: &mut MergedText<'a>,
        rev_node: Leader<'a>,
        previous_sibling: Option<&PreviousSibling<'a>>,
        indentation: &'b str,
        class_mapping: &ClassMapping<'a>,
    ) -> Cow<'b, str> {
        let arbitrary_representative = rev_node.as_representative().node;
        let representatives = {
            let mut representatives = class_mapping.representatives(rev_node);
            representatives.sort_by_key(|a| a.rev);
            representatives
        };
        match previous_sibling {
            Some(&PreviousSibling::RealNode(previous_node)) => {
                let revisions = class_mapping.revision_set(previous_node);
                let common_revisions =
                    revisions.intersection(class_mapping.revision_set(rev_node).set());
                let whitespaces = [Revision::Left, Revision::Right, Revision::Base].map(|rev| {
                    if common_revisions.contains(rev) {
                        Self::whitespace_at_rev(
                            rev,
                            previous_node,
                            rev_node,
                            indentation,
                            class_mapping,
                        )
                    } else {
                        None
                    }
                });

                let (preceding_whitespace, indentation_shift) = match whitespaces {
                    [
                        Some(whitespace_left),
                        Some(whitespace_right),
                        Some(whitespace_base),
                    ] => {
                        if whitespace_base == whitespace_left {
                            whitespace_right
                        } else {
                            whitespace_left
                        }
                    }
                    [Some(w), _, _] | [_, Some(w), _] | [_, _, Some(w)] => w,
                    _ => representatives
                        .iter()
                        .find_map(|repr| {
                            let indentation_shift = repr.node.indentation_shift().unwrap_or("");
                            let ancestor_newlines =
                                format!("\n{}", repr.node.ancestor_indentation().unwrap_or(""));
                            let new_newlines = format!("\n{indentation}");
                            if let Some(preceding_whitespace) = repr.node.preceding_whitespace() {
                                let new_whitespace =
                                    preceding_whitespace.replace(&ancestor_newlines, &new_newlines);
                                Some((Cow::from(new_whitespace), Cow::from(indentation_shift)))
                            } else {
                                None
                            }
                        })
                        .unwrap_or((Cow::from(""), Cow::from(""))),
                };

                output.push_merged(preceding_whitespace);
                Cow::from(format!("{indentation}{indentation_shift}"))
            }
            Some(PreviousSibling::CommutativeSeparator(separator)) => {
                if separator.ends_with('\n') {
                    let shift = arbitrary_representative.indentation_shift().unwrap_or("");
                    let new_indentation = format!("{indentation}{shift}");
                    output.push_merged(Cow::from(new_indentation.clone()));
                    Cow::from(new_indentation)
                } else {
                    Cow::from(indentation)
                }
            }
            None => {
                let whitespace = representatives
                    .iter()
                    .find_map(|repr| repr.node.preceding_whitespace())
                    .unwrap_or("");
                output.push_merged(Cow::from(whitespace));
                Cow::from(indentation)
            }
        }
    }

    /// Extracts the whitespace between two nodes at a given revision
    fn whitespace_at_rev(
        rev: Revision,
        previous_node: Leader<'a>,
        current_node: Leader<'a>,
        indentation: &str,
        class_mapping: &ClassMapping<'a>,
    ) -> Option<(Cow<'a, str>, Cow<'a, str>)> {
        let previous_node_at_rev = class_mapping.node_at_rev(previous_node, rev)?;
        let current_node_at_rev = class_mapping.node_at_rev(current_node, rev)?;

        // let's try to reuse the whitespace from the original source at that revision,
        // which we can do if the previous tree was indeed just before this one in the original tree
        let previous_end = previous_node_at_rev.byte_range.end;
        let current_start = current_node_at_rev.byte_range.start;
        if previous_end > current_start {
            return None;
        }

        let root = current_node_at_rev.root();
        let root_start = root.byte_range.start;
        let source = &root.source[(previous_end - root_start)..(current_start - root_start)];
        if !source.trim().is_empty() {
            return None;
        }

        if let Some(ancestor_indentation) = current_node_at_rev.ancestor_indentation() {
            let indentation_shift = Self::extract_indentation_shift(ancestor_indentation, source);
            Some((
                Cow::from(source.replace(
                    &format!("\n{ancestor_indentation}"),
                    &format!("\n{indentation}"),
                )),
                Cow::from(indentation_shift),
            ))
        } else {
            let indentation = Self::extract_indentation_shift("", source);
            Some((Cow::from(source), Cow::from(indentation)))
        }
    }

    fn trailing_whitespace(node: Leader<'a>, class_mapping: &ClassMapping<'a>) -> Option<&'a str> {
        let node_base = class_mapping.node_at_rev(node, Revision::Base);
        let node_left = class_mapping.node_at_rev(node, Revision::Left);
        let node_right = class_mapping.node_at_rev(node, Revision::Right);

        match (node_base, node_left, node_right) {
            (Some(base), Some(left), Some(right)) => {
                let base_trailing = base.trailing_whitespace();
                let left_trailing = left.trailing_whitespace();
                let right_trailing = right.trailing_whitespace();
                if base_trailing == left_trailing {
                    right_trailing
                } else {
                    left_trailing
                }
            }
            (_, Some(node), _) | (_, _, Some(node)) | (Some(node), _, _) => {
                node.trailing_whitespace()
            }
            (None, None, None) => None,
        }
    }

    fn extract_indentation_shift<'b>(
        ancestor_indentation: &str,
        preceding_whitespace: &'b str,
    ) -> &'b str {
        let line_with_ancestor_indentation = format!("\n{ancestor_indentation}");
        preceding_whitespace
            .rfind(&line_with_ancestor_indentation)
            .map_or("", |s| {
                &preceding_whitespace[(s + line_with_ancestor_indentation.len())..]
            })
    }

    /// The number of conflicts in this merge
    pub fn count_conflicts(&self, settings: &DisplaySettings) -> usize {
        match self {
            Self::ExactTree { .. } | Self::CommutativeChildSeparator { .. } => 0,
            Self::MixedTree { children, .. } => {
                children.iter().map(|c| c.count_conflicts(settings)).sum()
            }
            Self::Conflict { .. } => 1,
            Self::LineBasedMerge { contents, .. } => {
                let left_marker = ">".repeat(settings.conflict_marker_size_or_default());
                contents.matches(&left_marker).count()
            }
        }
    }

    /// The number of conflicting bytes, as an attempt to quantify the effort
    /// required to solve them.
    pub fn conflict_mass(&self) -> usize {
        match self {
            Self::ExactTree { .. } | Self::CommutativeChildSeparator { .. } => 0,
            Self::MixedTree { children, .. } => children.iter().map(Self::conflict_mass).sum(),
            Self::Conflict { base, left, right } => {
                Self::pretty_print_astnode_list(Revision::Left, left).len()
                    + Self::pretty_print_astnode_list(Revision::Base, base).len()
                    + Self::pretty_print_astnode_list(Revision::Right, right).len()
            }
            Self::LineBasedMerge { conflict_mass, .. } => *conflict_mass,
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
                    .join("\n");
                format!("Mixed({node}\n{children_printed}{result})")
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
}

impl Display for MergedTree<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.debug_print(0))
    }
}
