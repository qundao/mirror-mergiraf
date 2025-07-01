use std::{collections::HashSet, ffi::OsStr, fmt::Display, hash::Hash, path::Path};

use itertools::Itertools;
use tree_sitter::Language;

use crate::{signature::SignatureDefinition, supported_langs::SUPPORTED_LANGUAGES};

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
    /// how to extract the signatures of nodes, uniquely identifying children of a commutative parent
    pub signatures: Vec<SignatureDefinition>,
    /// The injections query to locate nodes that need parsing in other languages.
    /// See https://tree-sitter.github.io/tree-sitter/3-syntax-highlighting.html#language-injection
    pub injections: Option<&'static str>,
}

impl PartialEq for LangProfile {
    /// Language names are currently treated as unique identifiers
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Hash for LangProfile {
    // Hashing only by name for now, as it is treated as unique id
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.name.hash(state);
    }
}

impl Display for LangProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
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
    /// This will return any CommutativeParent defined on this grammar type.
    /// CommutativeParents defined by queries are ignored.
    pub(crate) fn get_commutative_parent_by_grammar_name(
        &self,
        grammar_type: &str,
    ) -> Option<&CommutativeParent> {
        self.commutative_parents
            .iter()
            .find(|cr| cr.parent_type == ParentType::ByGrammarName(grammar_type))
    }

    pub(crate) fn find_signature_definition_by_grammar_name(
        &self,
        grammar_name: &str,
    ) -> Option<&SignatureDefinition> {
        self.signatures
            .iter()
            .find(|sig_def| sig_def.node_type == grammar_name)
    }

    /// Should this node type be treated as atomic?
    pub(crate) fn is_atomic_node_type(&self, node_type: &str) -> bool {
        self.atomic_nodes.contains(&node_type)
    }
}

/// Ways to specify the type of the parent node in a [`CommutativeParent`]
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum ParentType<'a> {
    /// Specified using the grammar node defined in the grammar
    ///
    /// This is used when a node is a commutative parent independent of the context, e.g. for sets
    ByGrammarName(&'a str),
    /// Specified using a tree-sitter query:
    ///
    /// ```tree-sitter
    /// (expression_statement (assignment
    ///   left: (identifier) @variable (#eq? @variable "__all__")
    ///   right: (list) @commutative
    /// ))
    /// ```
    ///
    /// This allows designating a node as a commutative parent only in certain contexts.
    ///
    /// For example, Python lists aren't commutative in general (the order matters for iteration,
    /// indexing etc.), but they can be seen as commutative in an [`__all__` declaration][1] -- and
    /// the query above encodes exactly this latter case
    ///
    /// [1]: https://docs.python.org/3/tutorial/modules.html#importing-from-a-package
    ByQuery(&'a str),
}

impl Display for ParentType<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ByGrammarName(name) => write!(f, "specified by grammar name: {name}"),
            // flatten the query to one line, since our logger doesn't handle multiline messages
            // too well
            Self::ByQuery(query) => write!(f, "specified by query: {}", query.lines().format(" ")),
        }
    }
}

/// Specification for a commutative parent in a given language.
#[derive(Debug, Clone)]
pub struct CommutativeParent {
    /// the type of the root node
    parent_type: ParentType<'static>,
    /// any separator that needs to be inserted between the children.
    /// It can be overridden by specifying separators in each children group.
    separator: &'static str,
    /// any left delimiter that can come before all children
    pub left_delim: Option<&'static str>,
    /// any right delimiter that can come after all children
    pub right_delim: Option<&'static str>,
    /// any restrictions on which types of children are allowed to commute together. If empty, all children can commute together.
    pub children_groups: Vec<ChildrenGroup>,
}

impl CommutativeParent {
    /// Short-hand function to declare a commutative parent without any delimiters.
    pub(crate) fn without_delimiters(root_type: &'static str, separator: &'static str) -> Self {
        Self {
            parent_type: ParentType::ByGrammarName(root_type),
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
            parent_type: ParentType::ByGrammarName(parent_type),
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
            parent_type: ParentType::ByGrammarName(parent_type),
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

    /// Short-hand function to create a commutative parent with delimiters and separator, with the
    /// parent node specified using a tree-sitter query
    ///
    /// See [`ParentType::ByQuery`] for more information
    pub(crate) fn from_query(
        query: &'static str,
        left_delim: &'static str,
        separator: &'static str,
        right_delim: &'static str,
    ) -> Self {
        debug_assert!(
            query.contains("@commutative"),
            "A '@commutative' capture is needed to identify which of the captured nodes is commutative, in query '{query:?}'",
        );
        Self {
            parent_type: ParentType::ByQuery(query),
            separator,
            left_delim: Some(left_delim),
            right_delim: Some(right_delim),
            children_groups: Vec::new(),
        }
    }

    /// Restrict a commutative parent to some children groups, possibly with their own separators
    pub(crate) fn restricted_to(self, children_groups: Vec<ChildrenGroup>) -> Self {
        #[cfg(debug_assertions)]
        {
            for children_group in &children_groups {
                if let Some(specific_separator) = children_group.separator {
                    assert_eq!(
                        specific_separator.trim(),
                        self.separator.trim(),
                        "Children group separator '{specific_separator:?}' inconsistent with parent separator '{:?}' in commutative parent '{:?}'",
                        self.separator,
                        self.parent_type
                    );
                }
            }
        }
        Self {
            children_groups,
            ..self
        }
    }

    /// the type of the root node
    pub(crate) fn parent_type(&self) -> &ParentType {
        &self.parent_type
    }

    /// Can children with the supplied types commute together?
    /// If so, return the separator to use when inserting two nodes
    /// in the same place.
    pub(crate) fn child_separator(&self, node_types: &HashSet<&str>) -> Option<&'static str> {
        if self.children_groups.is_empty() {
            // If there are no children groups to restrict commutativity to,
            // any children can commute and the default separator is used
            Some(self.separator)
        } else {
            // Otherwise, children can only commute if their types all belong
            // to the same group, in which case the separator is either that of
            // that specific group, or the default one for the commutative parent
            // as a fall-back.
            self.children_groups.iter().find_map(|group| {
                if group.node_types.is_superset(node_types) {
                    group.separator.or(Some(self.separator))
                } else {
                    None
                }
            })
        }
    }

    /// The separator for children in this group, trimmed from leading and trailing whitespace.
    /// To obtain the separator to be inserted between two commutatively merged elements,
    /// use `child_separator` instead.
    pub(crate) fn trimmed_separator(&self) -> &'static str {
        self.separator.trim()
    }
}

/// A group of children of a commutative node which are allowed to commute together
#[derive(Debug, Clone)]
pub struct ChildrenGroup {
    /// The types of nodes, as grammar names
    pub node_types: HashSet<&'static str>,
    /// An optional separator specific to this children group,
    /// better suited than the one from the commutative parent.
    /// It must only differ from the separator of the parent up to
    /// whitespace (their trimmed versions should be equal).
    pub separator: Option<&'static str>,
}

impl ChildrenGroup {
    pub(crate) fn new(types: &[&'static str]) -> Self {
        Self {
            node_types: types.iter().copied().collect(),
            separator: None,
        }
    }

    pub(crate) fn with_separator(types: &[&'static str], separator: &'static str) -> Self {
        Self {
            node_types: types.iter().copied().collect(),
            separator: Some(separator),
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

        let with_conflicts = ctx.parse_json("[{\"a\":1, \"b\":2, \"a\":3}]");
        let without_conflicts = ctx.parse_json("{\"a\": [4], \"b\": [4]}");

        assert!(with_conflicts.has_signature_conflicts());
        assert!(!without_conflicts.has_signature_conflicts());
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
