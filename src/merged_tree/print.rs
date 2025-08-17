use core::convert::identity;
use std::borrow::Cow;

use crate::{
    ast::AstNode,
    class_mapping::{ClassMapping, Leader, RevNode},
    merged_text::MergedText,
    merged_tree::{Conflict, MergedTree},
    pcs::Revision,
};

#[cfg(test)]
use crate::settings::DisplaySettings;

#[derive(Debug, Clone)]
enum PreviousSibling<'a> {
    RealNode(Leader<'a>),
    CommutativeSeparator(&'a str),
}

impl<'a> MergedTree<'a> {
    /// Renders the tree to a series of strings, with merged and conflicting sections
    pub fn to_merged_text(&'a self, class_mapping: &ClassMapping<'a>) -> MergedText<'a> {
        let mut merged_text = MergedText::new();
        self.pretty_print_recursively(&mut merged_text, class_mapping, None, "");
        merged_text
    }

    #[cfg(test)]
    /// Pretty-prints the result tree into its final output. Exciting!
    pub fn pretty_print<'u: 'a>(
        &'u self,
        class_mapping: &ClassMapping<'a>,
        settings: &DisplaySettings,
    ) -> String {
        self.to_merged_text(class_mapping).render(settings)
    }

    /// Recursively pretty-prints a sub part of the result tree.
    fn pretty_print_recursively<'u: 'a>(
        &'u self,
        output: &mut MergedText<'a>,
        class_mapping: &ClassMapping<'a>,
        previous_sibling: Option<&PreviousSibling<'a>>,
        indentation: &str,
    ) {
        match self {
            Self::ExactTree {
                node, revisions, ..
            } => {
                let picked_revision = revisions.any();
                let tree_at_rev = class_mapping
                    .node_at_rev(node, picked_revision)
                    .expect("specified revision is not available for class leader");
                let new_indentation = Self::add_preceding_whitespace(
                    output,
                    node,
                    previous_sibling,
                    indentation,
                    class_mapping,
                );
                output.push_merged(tree_at_rev.reindented_source(&new_indentation));
            }
            Self::MixedTree {
                node: leader,
                children,
                ..
            } => {
                let new_indentation = Self::add_preceding_whitespace(
                    output,
                    leader,
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

                if let Some(whitespace) = Self::trailing_whitespace(leader, class_mapping) {
                    output.push_merged(Cow::from(whitespace));
                }
            }
            Self::Conflict(Conflict { base, left, right }) => {
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
                    &first_leader,
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
            Self::LineBasedMerge { parsed, node } => {
                if parsed.is_empty() {
                    return;
                }
                Self::add_preceding_whitespace(
                    output,
                    node,
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
                output.push_line_based_merge(parsed, &full_indentation);
            }
            Self::CommutativeChildSeparator { separator, .. } => {
                output.push_merged(Cow::from(*separator));
            }
        }
    }

    /// Adds any preceding whitespace before pretty-printing a node.
    /// In most cases, whitespace isn't covered by the abstract syntax tree
    /// nodes. Representing a (merged) tree back to a string requires therefore
    /// explicitly adding this whitespace. This method is a heuristic which
    /// picks whitespace from the original trees and attempts to compute a suitable
    /// whitespace to append to the output.
    ///
    /// It also returns the new indentation at which the current node (`rev_node`)
    /// should be pretty-printed (without needing to add any further whitespace on the
    /// first line of the node).
    fn add_preceding_whitespace<'b>(
        output: &mut MergedText<'a>,
        rev_node: &Leader<'a>,
        previous_sibling: Option<&PreviousSibling<'a>>,
        indentation: &'b str,
        class_mapping: &ClassMapping<'a>,
    ) -> Cow<'b, str> {
        // The list of representatives of the node in the Base, Left and Right revisions.
        let representatives = {
            let mut representatives = class_mapping.representatives(rev_node);
            representatives.sort_by_key(|a| a.rev);
            representatives
        };
        match previous_sibling {
            Some(PreviousSibling::RealNode(previous_node)) => {
                let previous_revisions = class_mapping.revision_set(previous_node);
                let revisions = class_mapping.revision_set(rev_node);
                let common_revisions = previous_revisions.intersection(revisions.set());
                let whitespaces = [Revision::Left, Revision::Right, Revision::Base].map(|rev| {
                    if common_revisions.contains(rev) {
                        // The previous node in the output and the current have this revision
                        // in common. So we can likely reuse whitespace from this revision (almost) directly.
                        Self::whitespace_at_rev(
                            rev,
                            previous_node,
                            rev_node,
                            indentation,
                            class_mapping,
                        )
                    } else {
                        // One of the two nodes don't belong to this revision, so we can't use it to infer whitespace between them
                        None
                    }
                });

                // Now we have inferred potentially different whitespaces for each revision.
                // Which one should we pick?
                let (preceding_whitespace, indentation_shift) = if let [
                    Some(whitespace_left),
                    Some(whitespace_right),
                    Some(whitespace_base),
                ] = whitespaces
                {
                    // We have a candidate whitespace for all three revisions.
                    if whitespace_base == whitespace_left {
                        // If whitespace only changed in the right revision, then
                        // the right revision is likely doing some reformatting, so keep
                        // its whitespace, as an attempt to preserve the reformatting.
                        whitespace_right
                    } else {
                        // The left revision could be reformatting. Or both left and right,
                        // in which case we just go for the left revision arbitrarily.
                        whitespace_left
                    }
                } else {
                    // Otherwise, pick any of the computed whitespaces, in the priority order
                    // specified above (left, right, base), to handle reformattings the best we can.
                    (whitespaces.into_iter().find_map(identity))
                        .or_else(|| {
                            // If we couldn't find any computed whitespace,
                            // then fall back on using the whitespace preceding the current node,
                            // in any revision, regardless of whether the previous merged node
                            // is also the previous node in that revision.
                            representatives.iter().find_map(|repr| {
                                let preceding_whitespace = repr.node.preceding_whitespace()?;
                                let indentation_shift = repr.node.indentation_shift().unwrap_or("");
                                let ancestor_newlines =
                                    format!("\n{}", repr.node.ancestor_indentation().unwrap_or(""));
                                let new_newlines = format!("\n{indentation}");
                                // Final whitespace is obtained by re-indenting the preceding whitespace in the
                                // original revision, replacing any newlines in it by newlines with a potentially
                                // different indentation.
                                let new_whitespace =
                                    preceding_whitespace.replace(&ancestor_newlines, &new_newlines);
                                Some((Cow::from(new_whitespace), indentation_shift))
                            })
                        })
                        .unwrap_or_default()
                };

                output.push_merged(preceding_whitespace);
                Cow::from(format!("{indentation}{indentation_shift}"))
            }
            Some(PreviousSibling::CommutativeSeparator(separator)) => {
                // The previous merged node doesn't belong to any revision, as we created this separator
                // during commutative merging of children.
                if separator.ends_with('\n') {
                    // We start a new line, so we need to add indentation accordingly. To determine this
                    // indentation, we pick an arbitrary revision and use the indentation shift from there,
                    // until we figure out a more informed way to do that.
                    let arbitrary_representative = rev_node.as_representative().node;
                    let shift = arbitrary_representative.indentation_shift().unwrap_or("");
                    let new_indentation = format!("{indentation}{shift}");
                    output.push_merged(Cow::from(new_indentation.clone()));
                    Cow::from(new_indentation)
                } else {
                    // The separator is assumed to contain sufficient whitespace on its own,
                    // we don't add any other.
                    Cow::from(indentation)
                }
            }
            None => {
                // Otherwise we're the first child in the list, just fall back on the preceding
                // whitespace in any revision
                let whitespace = representatives
                    .iter()
                    .find_map(|repr| repr.node.preceding_whitespace())
                    .unwrap_or("");
                output.push_merged(Cow::from(whitespace));
                // Also add any leading source (content included in the node's source before the first child)
                let whitespace = representatives
                    .iter()
                    .find_map(|repr| repr.node.leading_source())
                    .unwrap_or("");
                output.push_merged(Cow::from(whitespace));
                Cow::from(indentation)
            }
        }
    }

    /// Extracts the whitespace between two nodes at a given revision.
    /// This returns two strings:
    /// - the whitespace between the nodes
    /// - the indentation shift of the current node (the difference between
    ///   the parent node's indentation and the current node's indentation)
    fn whitespace_at_rev(
        rev: Revision,
        previous_node: &Leader<'a>,
        current_node: &Leader<'a>,
        indentation: &str,
        class_mapping: &ClassMapping<'a>,
    ) -> Option<(Cow<'a, str>, &'a str)> {
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
        // make sure it only consists of whitespace
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
                indentation_shift,
            ))
        } else {
            let indentation = Self::extract_indentation_shift("", source);
            Some((Cow::from(source), indentation))
        }
    }

    /// Computes the best trailing whitespace to keep at the end of a node
    fn trailing_whitespace(node: &Leader<'a>, class_mapping: &ClassMapping<'a>) -> Option<&'a str> {
        let nodes = [Revision::Left, Revision::Right, Revision::Base]
            .map(|rev| class_mapping.node_at_rev(node, rev));

        if let [Some(left), Some(right), Some(base)] = nodes {
            let base_trailing = base.trailing_whitespace();
            let left_trailing = left.trailing_whitespace();
            let right_trailing = right.trailing_whitespace();
            if base_trailing == left_trailing {
                // Only right changes, so perhaps it's a reformatting on the right revision.
                // Let's try to preserve this reformatting
                right_trailing
            } else {
                // Or maybe the left revision reformats. If both reformat, arbitrarily decide to keep the left side.
                left_trailing
            }
        } else {
            // If the node doesn't belong to all revisions, let's just pick a revision (in
            // the priority order defined above) and return the trailing whitespace at that revision.
            nodes
                .into_iter()
                .find_map(identity)
                .and_then(AstNode::trailing_whitespace)
        }
    }

    /// Compute the difference between the ancestor's indentation and the current node's indentation.
    /// When pretty-printing the node at new indentation (given that the node might have moved places),
    /// we'll add this indentation shift to the new indentation, to obtain the indentation of the new contents
    /// of the node.
    fn extract_indentation_shift<'b>(
        ancestor_indentation: &str,
        preceding_whitespace: &'b str,
    ) -> &'b str {
        let line_with_ancestor_indentation = format!("\n{ancestor_indentation}");
        // Subtract the ancestor's indentation from the last indented line.
        // For example, consider:
        // - the following `ancestor_indentation`: ".."       (2 spaces)
        // - the following `preceding_whitespace`: "\n\n...." (4 spaces)
        //
        // We match the former onto the latter like this:
        // "\n\n...."
        //    \n..^^--- the indentation shift
        //    ^^^^-----`ancestor_indentation`
        preceding_whitespace
            .rsplit_once(&line_with_ancestor_indentation)
            .map_or("", |(_, shift)| shift)
    }
}
