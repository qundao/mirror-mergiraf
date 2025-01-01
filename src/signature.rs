use std::fmt::Display;
use std::hash::{Hash, Hasher};

use itertools::Itertools;

use crate::class_mapping::ClassMapping;
use crate::merged_tree::MergedTree;
use crate::tree::AstNode;

/// A signature discriminates children of a commutative parent together.
/// No two children of the same commutative parent should have the same signature.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Signature<'a, 'b>(Vec<Vec<AstNodeEquiv<'a, 'b>>>);

impl Display for Signature<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "Signature [{}]",
            self.0
                .iter()
                .map(|x| format!(
                    "[{}]",
                    x.iter()
                        .map(|element| match element {
                            AstNodeEquiv::Original(ast_node) => ast_node.source.to_owned(),
                            AstNodeEquiv::Merged(tree) => format!("{tree}"),
                        })
                        .join(", ")
                ))
                .join(", ")
        ))
    }
}

/// A part of a tree, either an original one or a merged one,
/// with equality being defined as "quasi" isomorphism between them.
/// Only "quasi" because this equality doesn't have access to the class mapping
/// so has to resort to hash equality in some sub-cases.
#[derive(Debug, Clone, Copy, Eq)]
enum AstNodeEquiv<'a, 'b: 'a> {
    Original(&'b AstNode<'b>),
    Merged(&'a MergedTree<'b>),
}

impl<'a, 'b> AstNodeEquiv<'a, 'b> {
    /// Unified interface to fetch children by field name on either an original tree or a merged one
    fn children_by_field_name(
        &self,
        field_name: &str,
        class_mapping: &ClassMapping<'b>,
    ) -> Vec<AstNodeEquiv<'a, 'b>> {
        match self {
            AstNodeEquiv::Original(ast_node) => ast_node
                .children_by_field_name(field_name)
                .iter()
                .flat_map(|l| l.iter().map(|c| AstNodeEquiv::Original(c)))
                .collect(),
            AstNodeEquiv::Merged(tree) => match tree {
                MergedTree::ExactTree {
                    node, revisions, ..
                } => {
                    let rev = revisions.any();
                    let representative = class_mapping
                        .node_at_rev(*node, rev)
                        .expect("Inconsistent class_mapping and ExactTree revisions");
                    (AstNodeEquiv::Original(representative))
                        .children_by_field_name(field_name, class_mapping)
                }
                MergedTree::MixedTree { children, .. } => children
                    .iter()
                    .filter(|child| child.field_name(class_mapping) == Some(field_name))
                    .map(AstNodeEquiv::Merged)
                    .collect(),
                MergedTree::Conflict { .. }
                | MergedTree::LineBasedMerge { .. }
                | MergedTree::CommutativeChildSeparator { .. } => Vec::new(),
            },
        }
    }

    /// Unified interface to fetch children by grammar name on either an original tree or a merged one
    fn children_by_grammar_name(
        &self,
        grammar_name: &str,
        class_mapping: &ClassMapping<'b>,
    ) -> Vec<AstNodeEquiv<'a, 'b>> {
        match self {
            AstNodeEquiv::Original(ast_node) => ast_node
                .children
                .iter()
                .filter(|child| child.grammar_name == grammar_name)
                .map(|l| AstNodeEquiv::Original(l))
                .collect(),
            AstNodeEquiv::Merged(tree) => match tree {
                MergedTree::ExactTree {
                    node, revisions, ..
                } => {
                    let rev = revisions.any();
                    let representative = class_mapping
                        .node_at_rev(*node, rev)
                        .expect("Inconsistent class_mapping and ExactTree revisions");
                    (AstNodeEquiv::Original(representative))
                        .children_by_grammar_name(grammar_name, class_mapping)
                }
                MergedTree::MixedTree { children, .. } => children
                    .iter()
                    .filter(|child| child.grammar_name() == Some(grammar_name))
                    .map(AstNodeEquiv::Merged)
                    .collect(),
                MergedTree::Conflict { .. }
                | MergedTree::LineBasedMerge { .. }
                | MergedTree::CommutativeChildSeparator { .. } => Vec::new(),
            },
        }
    }

    fn isomorphic(&self, other: &Self, class_mapping: Option<&ClassMapping<'b>>) -> bool {
        match (self, other) {
            (AstNodeEquiv::Original(a), AstNodeEquiv::Original(b)) => a.isomorphic_to(b),
            (AstNodeEquiv::Original(a), AstNodeEquiv::Merged(b))
            | (AstNodeEquiv::Merged(b), AstNodeEquiv::Original(a)) => {
                match b {
                    MergedTree::ExactTree {
                        hash,
                        revisions,
                        node,
                    } => {
                        if let Some(class_mapping) = class_mapping {
                            let representative = class_mapping
                                .node_at_rev(*node, revisions.any())
                                .expect("inconsistent class mapping and ExactTree revisions");
                            representative.isomorphic_to(a)
                        } else {
                            // in the absence of a class_mapping, we just treat the nodes as equivalent if they have the same hash
                            *hash == a.hash
                        }
                    }
                    MergedTree::MixedTree { node, children, .. } => {
                        node.grammar_name() == a.grammar_name
                            && children.len() == a.children.len()
                            && children
                                .iter()
                                .zip(a.children.iter())
                                .all(|(child, ast_node)| {
                                    AstNodeEquiv::Merged(child).isomorphic(
                                        &AstNodeEquiv::Original(ast_node),
                                        class_mapping,
                                    )
                                })
                    }
                    MergedTree::Conflict { .. } => false,
                    MergedTree::LineBasedMerge { node, contents, .. } => {
                        node.grammar_name() == a.grammar_name && contents == a.source
                    }
                    MergedTree::CommutativeChildSeparator { separator } => {
                        separator.trim() == a.source
                    }
                }
            }
            (AstNodeEquiv::Merged(a), AstNodeEquiv::Merged(b)) => match (a, b) {
                (
                    MergedTree::ExactTree {
                        revisions, node, ..
                    },
                    b,
                )
                | (
                    b,
                    MergedTree::ExactTree {
                        revisions, node, ..
                    },
                ) => {
                    if let Some(class_mapping) = class_mapping {
                        let representative = class_mapping
                            .node_at_rev(*node, revisions.any())
                            .expect("inconsistent class mapping and ExactTree::revisions");
                        AstNodeEquiv::Merged(b).isomorphic(
                            &AstNodeEquiv::Original(representative),
                            Some(class_mapping),
                        )
                    } else {
                        // we don't have access to a class mapping so we resort on hash equality
                        let mut hasher = crate::fxhasher();
                        self.hash(&mut hasher);
                        let hash_a = hasher.finish();
                        hasher = crate::fxhasher();
                        other.hash(&mut hasher);
                        let hash_b = hasher.finish();
                        hash_a == hash_b
                    }
                }
                (
                    MergedTree::MixedTree {
                        node: node_a,
                        children: children_a,
                        ..
                    },
                    MergedTree::MixedTree {
                        node: node_b,
                        children: children_b,
                        ..
                    },
                ) => {
                    node_a.grammar_name() == node_b.grammar_name()
                        && children_a.len() == children_b.len()
                        && children_a
                            .iter()
                            .zip(children_b.iter())
                            .all(|(child_a, child_b)| {
                                AstNodeEquiv::Merged(child_a)
                                    .isomorphic(&AstNodeEquiv::Merged(child_b), class_mapping)
                            })
                }
                (MergedTree::MixedTree { .. }, _) | (_, MergedTree::MixedTree { .. }) => false,
                (MergedTree::Conflict { .. }, _) | (_, MergedTree::Conflict { .. }) => a == b,
                (_, _) => a == b,
            },
        }
    }
}

impl PartialEq for AstNodeEquiv<'_, '_> {
    fn eq(&self, other: &Self) -> bool {
        self.isomorphic(other, None)
    }
}

impl Hash for AstNodeEquiv<'_, '_> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            AstNodeEquiv::Original(ast_node) => ast_node.hash.hash(state),
            AstNodeEquiv::Merged(tree) => match tree {
                MergedTree::ExactTree { hash, .. } | MergedTree::MixedTree { hash, .. } => {
                    hash.hash(state);
                }
                MergedTree::Conflict { base, left, right } => {
                    base.hash(state);
                    left.hash(state);
                    right.hash(state);
                }
                MergedTree::LineBasedMerge { node, contents, .. } => {
                    node.hash(state);
                    contents.hash(state);
                }
                MergedTree::CommutativeChildSeparator { separator } => {
                    separator.hash(state);
                }
            },
        }
    }
}

impl Display for AstNodeEquiv<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AstNodeEquiv::Original(ast_node) => write!(f, "Original({ast_node})"),
            AstNodeEquiv::Merged(merged) => write!(f, "Merged({merged})"),
        }
    }
}

/// Checks if two merged trees are isomorphic
pub(crate) fn isomorphic_merged_trees<'a>(
    a: &MergedTree<'a>,
    b: &MergedTree<'a>,
    class_mapping: &ClassMapping<'a>,
) -> bool {
    AstNodeEquiv::Merged(a).isomorphic(&AstNodeEquiv::Merged(b), Some(class_mapping))
}

/// Defines how to compute the signature for a particular type of nodes.
#[derive(Debug, Clone)]
pub struct SignatureDefinition {
    // the type of the node from which this signature can be extracted
    pub node_type: &'static str,
    // The list of paths to take into account when extracting the signature
    pub paths: Vec<AstPath>,
}

/// Helper to ease declaring signatures in `supported_langs.rs`
pub fn signature(node_type: &'static str, paths: Vec<Vec<PathStep>>) -> SignatureDefinition {
    SignatureDefinition {
        node_type,
        paths: paths.into_iter().map(|steps| AstPath { steps }).collect(),
    }
}

impl SignatureDefinition {
    pub fn new(node_type: &'static str, paths: Vec<Vec<PathStep>>) -> Self {
        signature(node_type, paths)
    }

    /// Extracts a signature for the supplied original node
    pub(crate) fn extract_signature_from_original_node<'a, 'b: 'a>(
        &self,
        node: &'b AstNode<'b>,
    ) -> Signature<'a, 'b> {
        self.extract_internal(AstNodeEquiv::Original(node), &ClassMapping::new())
    }

    /// Extracts a signature for the supplied original node
    pub(crate) fn extract_signature_from_merged_node<'a, 'b: 'a>(
        &self,
        node: &'a MergedTree<'b>,
        class_mapping: &ClassMapping<'b>,
    ) -> Signature<'a, 'b> {
        let node_equiv = AstNodeEquiv::Merged(node);
        self.extract_internal(node_equiv, class_mapping)
    }

    /// Extracts a signature for the supplied node
    fn extract_internal<'a, 'b: 'a>(
        &self,
        node: AstNodeEquiv<'a, 'b>,
        class_mapping: &ClassMapping<'b>,
    ) -> Signature<'a, 'b> {
        Signature(
            self.paths
                .iter()
                .map(|path| path.extract(node, class_mapping).into_iter().collect_vec())
                .collect(),
        )
    }
}

/// Describes how to go from a node to a set of descendants, by following
/// a path specified by a list of field names.
#[derive(Debug, Clone)]
pub struct AstPath {
    /// The list of nodes types to follow
    pub steps: Vec<PathStep>,
}

/// A step in an [`AstPath`], consisting in walking either
/// into a particular field by its name, or selecting all
/// children of a given type.
#[derive(Debug, Clone)]
pub enum PathStep {
    /// Fetch all children in the field
    Field(&'static str),
    /// Fetch all children of a given grammar type
    ChildType(&'static str),
}

impl AstPath {
    pub fn new(steps: Vec<&'static str>) -> Self {
        AstPath {
            steps: steps.into_iter().map(PathStep::Field).collect(),
        }
    }

    /// Extracts a list of descendants which can be reached from the node
    /// by following the path.
    fn extract<'a, 'b: 'a>(
        &self,
        node: AstNodeEquiv<'a, 'b>,
        class_mapping: &ClassMapping<'b>,
    ) -> Vec<AstNodeEquiv<'a, 'b>> {
        let mut result = Vec::new();
        Self::extract_internal(&self.steps, node, &mut result, class_mapping);
        result
    }

    fn extract_internal<'a, 'b: 'a>(
        path: &[PathStep],
        node: AstNodeEquiv<'a, 'b>,
        result: &mut Vec<AstNodeEquiv<'a, 'b>>,
        class_mapping: &ClassMapping<'b>,
    ) {
        match path {
            [] => result.push(node),
            [step, rest @ ..] => {
                match step {
                    PathStep::Field(field_name) => {
                        // select children of the node which have a matching type
                        node.children_by_field_name(field_name, class_mapping)
                            .into_iter()
                            .for_each(|child| {
                                Self::extract_internal(rest, child, result, class_mapping);
                            });
                    }
                    PathStep::ChildType(grammar_name) => node
                        .children_by_grammar_name(grammar_name, class_mapping)
                        .into_iter()
                        .for_each(|child| {
                            Self::extract_internal(rest, child, result, class_mapping);
                        }),
                }
            }
        }
    }
}

impl Display for AstPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.steps.iter().join(", "))
    }
}

impl Display for PathStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PathStep::Field(field_name) => write!(f, "field({field_name})"),
            PathStep::ChildType(child_type) => write!(f, "child_type({child_type})"),
        }
    }
}

#[cfg(test)]
mod tests {

    use crate::{
        class_mapping::{ClassMapping, RevNode, RevisionNESet},
        pcs::Revision,
        test_utils::{ctx, hash},
    };

    use super::*;

    #[test]
    fn equal_signatures() {
        let ctx = ctx();

        let document = ctx.parse_json("{\"a\":\"b\"}").root();
        let other_document = ctx.parse_json("{\"a\":\"c\"}").root();
        let object = document.child(0).unwrap();
        let pair = object.child(1).unwrap();
        let other_pair = other_document.child(0).unwrap().child(1).unwrap();
        let key = pair.child(0).unwrap();

        let signature_def = {
            let paths = vec![vec![PathStep::Field("key")]];
            signature("pair", paths)
        };

        let expected_sig = Signature(vec![vec![AstNodeEquiv::Original(key)]]);
        assert_eq!(
            signature_def.extract_signature_from_original_node(pair),
            expected_sig
        );
        assert_eq!(
            signature_def.extract_signature_from_original_node(other_pair),
            expected_sig
        );
    }

    #[test]
    fn node_equality_and_hashing() {
        let ctx = ctx();

        let object = ctx.parse_json("{\"a\":\"b\"}").root().child(0).unwrap();
        let object_2 = ctx
            .parse_json("[{\"a\": \"b\"}]")
            .root()
            .child(0)
            .unwrap()
            .child(1)
            .unwrap();

        let class_mapping = ClassMapping::new();
        let node_2 = class_mapping.map_to_leader(RevNode {
            rev: Revision::Base,
            node: object_2,
        });
        let exact = MergedTree::new_exact(
            node_2,
            RevisionNESet::singleton(Revision::Base),
            &class_mapping,
        );

        assert!(object.isomorphic_to(object_2));
        assert_eq!(
            AstNodeEquiv::Original(object),
            AstNodeEquiv::Original(object_2)
        );
        assert_eq!(
            hash(&AstNodeEquiv::Original(object)),
            hash(&AstNodeEquiv::Original(object_2))
        );
        assert_eq!(AstNodeEquiv::Original(object), AstNodeEquiv::Merged(&exact));
        assert_eq!(
            hash(&AstNodeEquiv::Original(object)),
            hash(&AstNodeEquiv::Merged(&exact))
        );

        let children = object_2
            .children
            .iter()
            .map(|child| {
                MergedTree::new_exact(
                    class_mapping.map_to_leader(RevNode {
                        rev: Revision::Base,
                        node: child,
                    }),
                    RevisionNESet::singleton(Revision::Base),
                    &class_mapping,
                )
            })
            .collect();
        let mixed_tree = MergedTree::new_mixed(node_2, children);
        assert_eq!(
            AstNodeEquiv::Original(object),
            AstNodeEquiv::Merged(&mixed_tree)
        );
        assert_eq!(
            hash(&AstNodeEquiv::Original(object)),
            hash(&AstNodeEquiv::Merged(&mixed_tree))
        );
    }
}
