use std::{
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

/// A merged tree, which can contain a mixture of elements from the original trees,s
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
    CommutativeChildSeparator { separator: String },
}

#[derive(Debug, Clone)]
enum PreviousSibling<'a> {
    RealNode(Leader<'a>),
    CommutativeSeparator(String),
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
        MergedTree::ExactTree {
            node,
            revisions,
            hash: representative.hash,
        }
    }

    /// Creates a new mixed tree, taking care of the pre-computation of the hash
    pub(crate) fn new_mixed(node: Leader<'a>, children: Vec<MergedTree<'a>>) -> Self {
        // TODO we could refuse to create a new mixed tree with no children
        let mut hasher = crate::fxhasher();
        node.grammar_name().hash(&mut hasher);
        children
            .iter()
            .map(|child| match child {
                MergedTree::ExactTree { hash, .. } | MergedTree::MixedTree { hash, .. } => *hash,
                MergedTree::Conflict {
                    base: _,
                    left: _,
                    right: _,
                } => 1,
                MergedTree::LineBasedMerge {
                    node: _,
                    contents: _,
                    conflict_mass: _,
                } => 2,
                MergedTree::CommutativeChildSeparator { separator: _ } => 3,
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
            MergedTree::ExactTree { node, .. }
            | MergedTree::LineBasedMerge { node, .. }
            | MergedTree::MixedTree { node, .. } => class_mapping.field_name(*node),
            MergedTree::Conflict { .. } | MergedTree::CommutativeChildSeparator { .. } => None,
        }
    }

    /// The `grammar_name` of the underlying AST node, if any.
    pub(crate) fn grammar_name(&self) -> Option<&'static str> {
        match self {
            MergedTree::ExactTree { node, .. }
            | MergedTree::LineBasedMerge { node, .. }
            | MergedTree::MixedTree { node, .. } => Some(node.grammar_name()),
            MergedTree::Conflict { .. } | MergedTree::CommutativeChildSeparator { .. } => None,
        }
    }

    /// Generates a line-based merge for a node across multiple revisions.
    pub(crate) fn line_based_local_fallback_for_revnode(
        node: Leader<'a>,
        class_mapping: &ClassMapping<'a>,
    ) -> MergedTree<'a> {
        let base_src = class_mapping.node_at_rev(node, Revision::Base);
        let left_src = class_mapping.node_at_rev(node, Revision::Left);
        let right_src = class_mapping.node_at_rev(node, Revision::Right);
        match (base_src, left_src, right_src) {
            (None, None, None) => {
                panic!("A node that does not belong to any revision, how curious!")
            }
            (_, Some(_), None) => MergedTree::new_exact(
                node,
                RevisionNESet::singleton(Revision::Left),
                class_mapping,
            ),
            (_, None, Some(_)) => MergedTree::new_exact(
                node,
                RevisionNESet::singleton(Revision::Right),
                class_mapping,
            ),
            (Some(_), None, None) => MergedTree::new_exact(
                node,
                RevisionNESet::singleton(Revision::Base),
                class_mapping,
            ),
            (base, Some(left), Some(right)) => {
                let base_src = base.map(|n| n.unindented_source()).unwrap_or("".to_owned());
                let left_src = left.unindented_source();
                let right_src = right.unindented_source();
                let line_based_merge = line_based_merge(
                    &base_src,
                    &left_src,
                    &right_src,
                    &DisplaySettings::default(),
                );
                MergedTree::LineBasedMerge {
                    node,
                    contents: line_based_merge.contents,
                    conflict_mass: line_based_merge.conflict_mass,
                }
            }
        }
    }

    /// 'Degrade' the merge by adding line-based conflicts for all subtrees rooted in the supplied nodes
    pub(crate) fn force_line_based_fallback_on_specific_nodes(
        &self,
        nodes: &HashSet<Leader<'a>>,
        class_mapping: &ClassMapping<'a>,
    ) -> MergedTree<'a> {
        match self {
            MergedTree::ExactTree {
                node, revisions, ..
            } => {
                if nodes.contains(node) {
                    Self::line_based_local_fallback_for_revnode(*node, class_mapping)
                } else {
                    let picked_revision = revisions.any();
                    let children = class_mapping
                        .children_at_revision(*node, picked_revision)
                        .expect("non-existent children for revision in revset of ExactTree");
                    let cloned_children: Vec<MergedTree<'a>> = children
                        .iter()
                        .map(|c| {
                            MergedTree::new_exact(*c, *revisions, class_mapping)
                                .force_line_based_fallback_on_specific_nodes(nodes, class_mapping)
                        })
                        .collect();
                    if cloned_children
                        .iter()
                        .all(|child| matches!(child, MergedTree::ExactTree { .. }))
                    {
                        self.clone()
                    } else {
                        MergedTree::new_mixed(*node, cloned_children)
                    }
                }
            }
            MergedTree::MixedTree { node, children, .. } => {
                if nodes.contains(node) {
                    Self::line_based_local_fallback_for_revnode(*node, class_mapping)
                } else {
                    let cloned_children = children
                        .iter()
                        .map(|c| {
                            c.force_line_based_fallback_on_specific_nodes(nodes, class_mapping)
                        })
                        .collect();
                    MergedTree::new_mixed(*node, cloned_children)
                }
            }
            _ => self.clone(),
        }
    }

    /// Checks if a particular node is contained in the result tree
    pub fn contains(&self, leader: Leader<'a>, class_mapping: &ClassMapping<'a>) -> bool {
        match self {
            MergedTree::ExactTree {
                node, revisions, ..
            } => {
                let picked_revision = revisions.any();
                let ast_node = class_mapping.node_at_rev(*node, picked_revision).expect(
                    "inconsistency between revision set of ExactTree and the class mapping",
                );
                let chosen_revnode = RevNode::new(picked_revision, ast_node);
                chosen_revnode.contains(&leader, class_mapping)
            }
            MergedTree::MixedTree { node, children, .. } => {
                *node == leader || children.iter().any(|c| c.contains(leader, class_mapping))
            }
            // TODO here we could look for all representatives in their corresponding conflict side, that would be more accurate.
            MergedTree::Conflict { base, left, right } => match leader.as_representative().rev {
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
            MergedTree::LineBasedMerge { node, .. } => *node == leader,
            MergedTree::CommutativeChildSeparator { .. } => false,
        }
    }

    /// Pretty-prints the result tree into its final output. Exciting!
    pub fn pretty_print(
        &self,
        class_mapping: &ClassMapping<'a>,
        settings: &DisplaySettings,
    ) -> String {
        let mut output = MergedText::new();
        self.pretty_print_recursively(&mut output, class_mapping, None, "");
        output.render(settings)
    }

    /// Recursively pretty-prints a sub part of the result tree.
    fn pretty_print_recursively<'u>(
        &'u self,
        output: &mut MergedText,
        class_mapping: &ClassMapping<'a>,
        previous_sibling: Option<PreviousSibling<'a>>,
        indentation: &str,
    ) {
        match self {
            MergedTree::ExactTree {
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
                output.push_merged(tree_at_rev.reindented_source(&new_indentation));
            }
            MergedTree::MixedTree {
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
                        previous_sibling,
                        &new_indentation,
                    );
                    previous_sibling = match c {
                        MergedTree::ExactTree { node, .. }
                        | MergedTree::MixedTree { node, .. }
                        | MergedTree::LineBasedMerge { node, .. } => {
                            Some(PreviousSibling::RealNode(*node))
                        }
                        MergedTree::Conflict { .. } => None,
                        MergedTree::CommutativeChildSeparator { separator } => {
                            Some(PreviousSibling::CommutativeSeparator(separator.clone()))
                        }
                    };
                }
            }
            MergedTree::Conflict { base, left, right } => {
                if base.is_empty() && left.is_empty() && right.is_empty() {
                    return;
                }
                let first_leader = [
                    (left.first(), Revision::Left),
                    (right.first(), Revision::Right),
                    (base.first(), Revision::Base),
                ]
                .iter()
                .find_map(|(maybe_node, rev)| {
                    maybe_node.map(|node| class_mapping.map_to_leader(RevNode::new(*rev, node)))
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
                    Self::pretty_print_astnode_list(Revision::Base, base),
                    Self::pretty_print_astnode_list(Revision::Left, left),
                    Self::pretty_print_astnode_list(Revision::Right, right),
                );
            }
            MergedTree::LineBasedMerge { contents, node, .. } => {
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
                output.push_line_based_merge(contents, &full_indentation);
            }
            MergedTree::CommutativeChildSeparator { separator, .. } => {
                output.push_merged(separator.clone());
            }
        }
    }

    /// Adds any preceding whitespace before pretty-printing a node.
    fn add_preceding_whitespace(
        output: &mut MergedText,
        rev_node: Leader<'a>,
        previous_sibling: Option<PreviousSibling<'a>>,
        indentation: &str,
        class_mapping: &ClassMapping<'a>,
    ) -> String {
        let arbitrary_representative = rev_node.as_representative().node;
        let mut representatives = class_mapping.representatives(rev_node);
        representatives.sort_by(|a, b| Ord::cmp(&a.rev, &b.rev));
        match previous_sibling {
            Some(PreviousSibling::RealNode(previous_node)) => {
                let revisions = class_mapping.revision_set(previous_node);
                let common_revisions =
                    revisions.intersection(class_mapping.revision_set(rev_node).set());
                let whitespaces = [Revision::Left, Revision::Right, Revision::Base]
                    .iter()
                    .map(|rev| {
                        if common_revisions.contains(*rev) {
                            Self::whitespace_at_rev(
                                *rev,
                                previous_node,
                                rev_node,
                                indentation,
                                class_mapping,
                            )
                        } else {
                            None
                        }
                    })
                    .collect_vec();
                let (preceding_whitespace, indentation_shift) =
                    if let [Some(ref whitespace_left), Some(ref whitespace_right), Some(ref whitespace_base)] = whitespaces[..] {
                        if whitespace_base == whitespace_left {
                            Some(whitespace_right.clone())
                        } else {
                            Some(whitespace_left.clone())
                        }
                    } else {
                        whitespaces
                            .into_iter()
                            .flatten()
                            .next()
                    }.unwrap_or_else(|| {
                        representatives.iter()
                            .find_map(|repr| {
                                let indentation_shift = repr.node.indentation_shift().unwrap_or("").to_owned();
                                let ancestor_newlines = format!("\n{}", repr.node.ancestor_indentation().unwrap_or(""));
                                let new_newlines = format!("\n{indentation}");
                                if let Some(preceding_whitespace) = repr.node.preceding_whitespace() {
                                    let new_whitespace = preceding_whitespace.replace(&ancestor_newlines, &new_newlines);
                                    Some((new_whitespace, indentation_shift))
                                } else {
                                    None
                                }
                            })
                            .unwrap_or_else(|| ("".to_owned(), "".to_owned()))
                    });

                output.push_merged(preceding_whitespace);
                format!("{indentation}{indentation_shift}")
            }
            Some(PreviousSibling::CommutativeSeparator(separator)) => {
                if separator.ends_with('\n') {
                    let shift = arbitrary_representative
                        .indentation_shift()
                        .unwrap_or("")
                        .to_owned();
                    let new_indentation = format!("{indentation}{shift}");
                    output.push_merged(new_indentation.clone());
                    return new_indentation;
                }
                indentation.to_string()
            }
            None => {
                let whitespace = representatives
                    .iter()
                    .find_map(|repr| repr.node.preceding_whitespace())
                    .unwrap_or("");
                output.push_merged(whitespace.to_owned());
                indentation.to_string()
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
    ) -> Option<(String, String)> {
        let previous_node_at_rev = class_mapping.node_at_rev(previous_node, rev)?;
        let current_node_at_rev = class_mapping.node_at_rev(current_node, rev)?;

        // let's try to reuse the whitespace from the original source at that revision,
        // which we can do if the previous tree was indeed just before this one in the original tree
        let previous_end = previous_node_at_rev.byte_range.end;
        let current_start = current_node_at_rev.byte_range.start;
        if previous_end <= current_start {
            let root = current_node_at_rev.root();
            let root_start = root.byte_range.start;
            let source = &root.source[(previous_end - root_start)..(current_start - root_start)];
            if source.trim().is_empty() {
                if let Some(ancestor_indentation) = current_node_at_rev.ancestor_indentation() {
                    let indentation_shift =
                        Self::extract_indentation_shift(ancestor_indentation, source);
                    return Some((
                        source.replace(
                            &format!("\n{ancestor_indentation}"),
                            &format!("\n{indentation}"),
                        ),
                        indentation_shift,
                    ));
                } else {
                    let indentation = Self::extract_indentation_shift("", source);
                    return Some((source.to_owned(), indentation));
                }
            }
        }
        None
    }

    fn extract_indentation_shift(ancestor_indentation: &str, preceding_whitespace: &str) -> String {
        let line_with_ancestor_indentation = format!("\n{ancestor_indentation}");
        preceding_whitespace
            .rfind(&line_with_ancestor_indentation)
            .map(|s| preceding_whitespace[(s + line_with_ancestor_indentation.len())..].to_owned())
            .unwrap_or("".to_owned())
    }

    /// The number of conflicts in this merge
    pub fn count_conflicts(&self) -> usize {
        match self {
            MergedTree::ExactTree { .. } => 0,
            MergedTree::MixedTree { children, .. } => {
                children.iter().map(|c| c.count_conflicts()).sum()
            }
            MergedTree::Conflict { .. } => 1,
            MergedTree::LineBasedMerge { contents, .. } => contents.matches(">>>>>>>").count(),
            MergedTree::CommutativeChildSeparator { .. } => 0,
        }
    }

    /// The number of conflicting bytes, as an attempt to quantify the effort
    /// required to solve them.
    pub fn conflict_mass(&self) -> usize {
        match self {
            MergedTree::ExactTree { .. } => 0,
            MergedTree::MixedTree { children, .. } => {
                children.iter().map(|c| c.conflict_mass()).sum()
            }
            MergedTree::Conflict { base, left, right } => {
                Self::pretty_print_astnode_list(Revision::Left, left).len()
                    + Self::pretty_print_astnode_list(Revision::Base, base).len()
                    + Self::pretty_print_astnode_list(Revision::Right, right).len()
            }
            MergedTree::LineBasedMerge { conflict_mass, .. } => *conflict_mass,
            MergedTree::CommutativeChildSeparator { .. } => 0,
        }
    }

    fn pretty_print_astnode_list(_revision: Revision, list: &[&'a AstNode<'a>]) -> String {
        let mut output = String::new();
        let mut first = true;
        list.iter().for_each(|n| {
            let whitespace = n.preceding_whitespace().unwrap_or("");
            if first {
                first = false;
            } else {
                output.push_str(whitespace);
            }
            output.push_str(n.source);
        });
        output
    }

    /// Debug print with indentation
    fn debug_print(&self, indentation: usize) -> String {
        let mut result = " ".to_string().repeat(indentation);
        let c = match self {
            MergedTree::ExactTree {
                node, revisions, ..
            } => format!("Exact({node}{revisions})"),
            MergedTree::MixedTree { node, children, .. } => {
                let children_printed = children
                    .iter()
                    .map(|c| c.debug_print(indentation + 2))
                    .join("\n");
                format!("Mixed({node}\n{children_printed}{result})")
            }
            MergedTree::Conflict { .. } => "Conflict()".to_string(),
            MergedTree::LineBasedMerge { .. } => "LineBasedConflict()".to_string(),
            MergedTree::CommutativeChildSeparator { separator } => {
                format!("CommutativeChildSeparator({})", separator.escape_debug())
            }
        };
        result.push_str(&c);
        result
    }
}

impl<'a> Display for MergedTree<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.debug_print(0))
    }
}
