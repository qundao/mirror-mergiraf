use std::{collections::HashSet, ffi::OsStr, path::Path};

use itertools::Itertools;
use tree_sitter::Language;

use crate::{
    ast::AstNode,
    class_mapping::ClassMapping,
    merged_tree::MergedTree,
    signature::{Signature, SignatureDefinition},
    supported_langs::SUPPORTED_LANGUAGES,
};

/// Language-dependent settings to influence how merging is done.
/// All those settings are declarative (except for the tree-sitter parser, which is
/// imported from the corresponding crate).
#[derive(Debug, Clone)]
pub struct LangProfile {
    /// a name that identifies the language
    pub name: &'static str,
    /// alternate names for the language
    pub alternate_names: &'static [&'static str],
    /// the file extensions of files in this language
    pub extensions: Vec<&'static str>,
    /// `tree_sitter` parser
    pub language: Language,
    /// list of node types which should be treated as leaves (atomic parts of the syntax tree)
    pub atomic_nodes: Vec<&'static str>,
    /// list of node types whose child order does not matter
    pub commutative_parents: Vec<CommutativeParent>,
    // how to extract the signatures of nodes, uniquely identifying children of a commutative parent
    pub signatures: Vec<SignatureDefinition>,
}

impl LangProfile {
    /// Load a profile by language name.
    /// Alternate names or extensions are also considered.
    pub fn find_by_name(name: &str) -> Option<&'static Self> {
        SUPPORTED_LANGUAGES.iter().find(|lang_profile| {
            lang_profile.name.eq_ignore_ascii_case(name)
                || (lang_profile.alternate_names.iter())
                    .chain(&lang_profile.extensions)
                    .any(|aname| aname.eq_ignore_ascii_case(name))
        })
    }

    /// Detects the language of a file based on its filename
    pub fn detect_from_filename<P>(filename: &P) -> Option<&'static Self>
    where
        P: AsRef<Path> + ?Sized,
    {
        let filename = filename.as_ref();
        Self::_detect_from_filename(filename)
    }

    /// Loads a language either by name or by detecting it from a filename
    pub fn find_by_filename_or_name<P>(
        filename: &P,
        language_name: Option<&str>,
    ) -> Result<&'static Self, String>
    where
        P: AsRef<Path> + ?Sized,
    {
        if let Some(lang_name) = language_name {
            Self::find_by_name(lang_name)
                .ok_or_else(|| format!("Specified language '{lang_name}' could not be found"))
        } else {
            Self::detect_from_filename(filename).ok_or_else(|| {
                format!(
                    "Could not find a supported language for {}",
                    filename.as_ref().display()
                )
            })
        }
    }

    fn _detect_from_filename(filename: &Path) -> Option<&'static Self> {
        // TODO make something more advanced like in difftastic
        // https://github.com/Wilfred/difftastic/blob/master/src/parse/tree_sitter_parser.rs
        let extension = filename.extension()?;
        SUPPORTED_LANGUAGES.iter().find(|lang_profile| {
            lang_profile
                .extensions
                .iter()
                .copied()
                // NOTE: the comparison should be case-insensitive, see
                // https://rust-lang.github.io/rust-clippy/master/index.html#case_sensitive_file_extension_comparisons
                .any(|ext| extension.eq_ignore_ascii_case(OsStr::new(ext)))
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
        let conflict_in_children = || {
            node.children
                .iter()
                .any(|child| self.has_signature_conflicts(child))
        };

        let conflict_in_self = || {
            node.children.len() >= 2
                && self.get_commutative_parent(node.grammar_name).is_some()
                && !node
                    .children
                    .iter()
                    .filter_map(|child| self.extract_signature_from_original_node(child))
                    .all_unique()
        };

        conflict_in_self() || conflict_in_children()
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
    // any restrictions on which types of children are allowed to commute together. If empty, all children can commute together.
    pub children_groups: Vec<ChildrenGroup>,
}

impl CommutativeParent {
    /// Short-hand function to declare a commutative parent without any delimiters.
    pub(crate) fn without_delimiters(root_type: &'static str, separator: &'static str) -> Self {
        Self {
            parent_type: root_type,
            separator,
            left_delim: None,
            right_delim: None,
            children_groups: Vec::new(),
        }
    }

    /// Short-hand function to create a commutative parent with delimiters and separator
    pub(crate) fn new(
        parent_type: &'static str,
        left_delim: &'static str,
        separator: &'static str,
        right_delim: &'static str,
    ) -> Self {
        Self {
            parent_type,
            separator,
            left_delim: Some(left_delim),
            right_delim: Some(right_delim),
            children_groups: Vec::new(),
        }
    }

    /// Short-hand function to create a commutative parent with a left delimiter and separator
    pub(crate) fn with_left_delimiter(
        parent_type: &'static str,
        left_delim: &'static str,
        separator: &'static str,
    ) -> Self {
        Self {
            parent_type,
            separator,
            left_delim: Some(left_delim),
            right_delim: None,
            children_groups: Vec::new(),
        }
    }

    /// Short-hand to restrict a commutative parent to some children groups
    pub(crate) fn restricted_to_groups(self, groups: &[&[&'static str]]) -> Self {
        Self {
            children_groups: groups.iter().copied().map(ChildrenGroup::new).collect(),
            ..self
        }
    }

    /// Can children with the supplied types commute together?
    pub(crate) fn children_can_commute(&self, node_types: &HashSet<&str>) -> bool {
        self.children_groups.is_empty()
            || self
                .children_groups
                .iter()
                .any(|group| group.node_types.is_superset(node_types))
    }
}

/// A group of children of a commutative node which are allowed to commute together
#[derive(Debug, Clone)]
pub struct ChildrenGroup {
    /// The types of nodes, as grammar names
    pub node_types: HashSet<&'static str>,
}

impl ChildrenGroup {
    pub(crate) fn new(types: &[&'static str]) -> Self {
        Self {
            node_types: types.iter().copied().collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::test_utils::ctx;

    #[test]
    fn has_signature_conflicts() {
        let ctx = ctx();

        let lang_profile = LangProfile::json();

        let with_conflicts = ctx.parse_json("[{\"a\":1, \"b\":2, \"a\":3}]").root();
        let without_conflicts = ctx.parse_json("{\"a\": [4], \"b\": [4]}").root();

        assert!(lang_profile.has_signature_conflicts(with_conflicts));
        assert!(!lang_profile.has_signature_conflicts(without_conflicts));
    }

    #[test]
    fn find_by_name() {
        assert_eq!(LangProfile::find_by_name("JSON").unwrap().name, "JSON");
        assert_eq!(LangProfile::find_by_name("Json").unwrap().name, "JSON");
        assert_eq!(LangProfile::find_by_name("python").unwrap().name, "Python");
        assert_eq!(LangProfile::find_by_name("py").unwrap().name, "Python");
        assert_eq!(
            LangProfile::find_by_name("Java properties").unwrap().name,
            "Java properties"
        );
        assert!(
            LangProfile::find_by_name("unknown language").is_none(),
            "Language shouldn't be found"
        );
    }

    #[test]
    fn find_by_filename_or_name() {
        assert_eq!(
            LangProfile::find_by_filename_or_name("file.json", None)
                .unwrap()
                .name,
            "JSON"
        );
        assert_eq!(
            LangProfile::find_by_filename_or_name("file.java", Some("JSON"))
                .unwrap()
                .name,
            "JSON"
        );
        assert!(
            LangProfile::find_by_filename_or_name("file.json", Some("non-existent language"),)
                .is_err(),
            "If a language name is provided, the file name should be ignored"
        );
        assert!(
            LangProfile::find_by_filename_or_name("file.unknown_extension", None).is_err(),
            "Looking up language by unknown extension should fail"
        );
    }
}
