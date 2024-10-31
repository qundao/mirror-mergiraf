use itertools::Itertools;
use tree_sitter::Language;

use crate::{
    class_mapping::ClassMapping,
    merged_tree::MergedTree,
    signature::{Signature, SignatureDefinition},
    supported_langs::supported_languages,
    tree::AstNode,
};

/// Language-dependent settings to influence how merging is done.
/// All those settings are declarative (except for the tree-sitter parser, which is
/// imported from the corresponding crate).
#[derive(Debug, Clone)]
pub struct LangProfile {
    /// a name that identifies the language
    pub name: &'static str,
    /// the file extensions of files in this language
    pub extensions: Vec<&'static str>,
    /// tree_sitter parser
    pub language: Language,
    /// list of node types which should be treated as leaves (atomic parts of the syntax tree)
    pub atomic_nodes: Vec<&'static str>,
    /// list of node types whose child order does not matter
    pub commutative_parents: Vec<CommutativeParent>,
    // how to extract the signatures of nodes, uniquely identifying children of a commutative parent
    pub signatures: Vec<SignatureDefinition>,
}

impl LangProfile {
    /// Detects the language of a file based on its filename
    pub fn detect_from_filename(filename: &str) -> Option<LangProfile> {
        // TODO make something more advanced like in difftastic
        // https://github.com/Wilfred/difftastic/blob/master/src/parse/tree_sitter_parser.rs
        let supported = supported_languages();
        supported.into_iter().find(|lang_profile| {
            lang_profile
                .extensions
                .iter()
                .any(|extension| filename.ends_with(extension))
        })
    }

    /// Do all the children of this parent commute?
    pub fn get_commutative_parent(&self, grammar_type: &str) -> Option<&CommutativeParent> {
        self.commutative_parents
            .iter()
            .find(|cr| cr.parent_type == grammar_type)
    }

    /// Extracts a signature for the given node if we have a signature definition
    /// for this type of nodes.
    pub(crate) fn extract_signature_from_original_node<'a>(
        &self,
        node: &'a AstNode<'a>,
    ) -> Option<Signature<'a, 'a>> {
        let definition = self.find_signature_definition_by_grammar_name(node.grammar_name)?;
        Some(definition.extract_signature_from_original_node(node))
    }

    /// Extracts a signature for the given node if we have a signature definition
    /// for this type of nodes.
    pub(crate) fn extract_signature_from_merged_node<'b, 'a: 'b>(
        &self,
        node: &'b MergedTree<'a>,
        class_mapping: &ClassMapping<'a>,
    ) -> Option<Signature<'b, 'a>> {
        let grammar_name = match node {
            MergedTree::ExactTree { node, .. }
            | MergedTree::MixedTree { node, .. }
            | MergedTree::LineBasedMerge { node, .. } => Some(node.grammar_name()),
            MergedTree::Conflict { .. } | MergedTree::CommutativeChildSeparator { .. } => None,
        }?;
        let definition = self.find_signature_definition_by_grammar_name(grammar_name)?;
        let signature = definition.extract_signature_from_merged_node(node, class_mapping);
        Some(signature)
    }

    fn find_signature_definition_by_grammar_name(
        &self,
        grammar_name: &str,
    ) -> Option<&SignatureDefinition> {
        self.signatures
            .iter()
            .find(|sig_def| sig_def.node_type == grammar_name)
    }

    /// Checks if a tree has any signature conflicts in it
    pub(crate) fn has_signature_conflicts<'a>(&self, node: &'a AstNode<'a>) -> bool {
        let conflict_in_children = node
            .children
            .iter()
            .any(|child| self.has_signature_conflicts(child));
        conflict_in_children
            || (if node.children.len() < 2 {
                false
            } else if self.get_commutative_parent(&node.grammar_name).is_some() {
                !node
                    .children
                    .iter()
                    .flat_map(|child| self.extract_signature_from_original_node(child))
                    .all_unique()
            } else {
                false
            })
    }

    /// Should this node type be treated as atomic?
    pub(crate) fn is_atomic_node_type(&self, node_type: &str) -> bool {
        self.atomic_nodes.contains(&node_type)
    }
}

/// Specification for a commutative parent in a given language.
#[derive(Debug, Clone)]
pub struct CommutativeParent {
    // the type of the root node
    pub parent_type: &'static str,
    // any separator that needs to be inserted between the children
    pub separator: &'static str,
    // any left delimiter that can come before all children
    pub left_delim: Option<&'static str>,
    // any right delimiter that can come after all children
    pub right_delim: Option<&'static str>,
}

impl CommutativeParent {
    /// Short-hand function to declare a commutative parent without any delimiters.
    pub(crate) fn without_delimiters(root_type: &'static str, separator: &'static str) -> Self {
        CommutativeParent {
            parent_type: root_type,
            separator,
            left_delim: None,
            right_delim: None,
        }
    }

    /// Short-hand function to create a commutative parent with delimiters and separator
    pub(crate) fn new(
        parent_type: &'static str,
        left_delim: &'static str,
        separator: &'static str,
        right_delim: &'static str,
    ) -> Self {
        CommutativeParent {
            parent_type,
            separator,
            left_delim: Some(left_delim),
            right_delim: Some(right_delim),
        }
    }

    /// Short-hand function to create a commutative parent with a left delimiter and separator
    pub(crate) fn with_left_delimiter(
        parent_type: &'static str,
        left_delim: &'static str,
        separator: &'static str,
    ) -> Self {
        CommutativeParent {
            parent_type,
            separator,
            left_delim: Some(left_delim),
            right_delim: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        lang_profile::{CommutativeParent, LangProfile},
        signature::{signature, PathStep::Field},
        test_utils::ctx,
    };

    #[test]
    fn has_signature_conflicts() {
        let ctx = ctx();

        let lang_profile = LangProfile {
            name: "JSON",
            extensions: vec![".json"],
            language: tree_sitter_json::LANGUAGE.into(),
            atomic_nodes: vec![],
            commutative_parents: vec![
                // the order of keys is deemed irrelevant
                CommutativeParent::new("object", "{", ", ", "}"),
            ],
            signatures: vec![signature("pair", vec![vec![Field("key")]])],
        };

        let with_conflicts = ctx.parse_json("[{\"a\":1, \"b\":2, \"a\":3}]").root();
        let without_conflicts = ctx.parse_json("{\"a\": [4], \"b\": [4]}").root();

        assert!(lang_profile.has_signature_conflicts(with_conflicts));
        assert!(!lang_profile.has_signature_conflicts(without_conflicts));
    }
}
