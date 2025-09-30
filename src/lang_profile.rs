use std::{collections::HashSet, ffi::OsStr, fmt::Display, hash::Hash, path::Path};

use itertools::Itertools;
use tree_sitter::Language;

use crate::{
    ast::AstNode, git, signature::SignatureDefinition, supported_langs::SUPPORTED_LANGUAGES,
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
    /// the full file names that this language should be used for
    pub file_names: Vec<&'static str>,
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
    /// List of node types that should be flattened
    pub flattened_nodes: &'static [&'static str],
    /// List of node types that should be treated like comments,
    /// meaning that they can be bundled into neighbouring nodes to ease commutative merging.
    /// Nodes that are already `extra` in the tree-sitter grammar don't need to be added here.
    pub comment_nodes: &'static [&'static str],
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
                    .chain(&lang_profile.file_names)
                    .any(|aname| aname.eq_ignore_ascii_case(name))
        })
    }

    /// Detects the language of a file based on its filename
    pub fn detect_from_filename<P>(filename: P) -> Option<&'static Self>
    where
        P: AsRef<Path>,
    {
        fn inner(filename: &Path) -> Option<&'static LangProfile> {
            // TODO make something more advanced like in difftastic
            // https://github.com/Wilfred/difftastic/blob/master/src/parse/tree_sitter_parser.rs
            let extension = filename.extension()?;
            let name = filename.file_name()?;
            SUPPORTED_LANGUAGES.iter().find(|lang_profile| {
                lang_profile
                    .extensions
                    .iter()
                    .copied()
                    // NOTE: the comparison should be case-insensitive, see
                    // https://rust-lang.github.io/rust-clippy/master/index.html#case_sensitive_file_extension_comparisons
                    .any(|ext| extension.eq_ignore_ascii_case(OsStr::new(ext)))
                    || lang_profile
                        .file_names
                        .iter()
                        .copied()
                        .any(|ref_name| name == ref_name)
            })
        }
        inner(filename.as_ref())
    }

    /// Detects the language of a file based on VCS attributes
    pub fn detect_language_from_vcs_attr<P>(repo_dir: &Path, filename: P) -> Option<String>
    where
        P: AsRef<Path>,
    {
        git::read_lang_attribute(repo_dir, filename.as_ref())
    }

    /// Loads a language, by:
    /// - first, looking up the language using its name if provided
    /// - failing that, by detecting it via configuration from the gitattributes file
    /// - failing that, by detecting it from a filename
    pub fn find<P>(
        filename: P,
        language_name: Option<&str>,
        repo_dir: Option<&Path>,
    ) -> Result<&'static Self, String>
    where
        P: AsRef<Path>,
    {
        let filename = filename.as_ref();
        if let Some(lang_name) = language_name {
            Self::find_by_name(lang_name)
                .ok_or_else(|| format!("Specified language '{lang_name}' could not be found"))
            // If lookup by name failed, we don't fall back on the other detection methods,
            // because don't want to silently ignore an invalid language name.
        } else if let Some(repo_dir) = repo_dir
            && let Some(lang_name) = Self::detect_language_from_vcs_attr(repo_dir, filename)
        {
            Self::find_by_name(&lang_name).ok_or_else(|| {
                format!("Attribute-specified language '{lang_name}' could not be found")
            })
        } else {
            Self::detect_from_filename(filename).ok_or_else(|| {
                format!(
                    "Could not find a supported language for {}",
                    filename.display()
                )
            })
        }
    }

    fn _detect_from_filename(path: &Path) -> Option<&'static Self> {
        // TODO make something more advanced like in difftastic
        // https://github.com/Wilfred/difftastic/blob/master/src/parse/tree_sitter_parser.rs
        let extension = path.extension()?;
        let name = path.file_name()?;
        SUPPORTED_LANGUAGES.iter().find(|lang_profile| {
            lang_profile
                .extensions
                .iter()
                // NOTE: the comparison should be case-insensitive, see
                // https://rust-lang.github.io/rust-clippy/master/index.html#case_sensitive_file_extension_comparisons
                .any(|ext| extension.eq_ignore_ascii_case(OsStr::new(ext)))
                || lang_profile
                    .file_names
                    .iter()
                    .any(|file_name| name == OsStr::new(file_name))
        })
    }

    /// Do all the children of this parent commute?
    /// This will return any CommutativeParent defined on this node kind.
    /// CommutativeParents defined by queries are ignored.
    pub(crate) fn get_commutative_parent_by_kind(&self, kind: &str) -> Option<&CommutativeParent> {
        self.commutative_parents
            .iter()
            .find(|cr| cr.parent_type == ParentType::ByKind(kind))
    }

    pub(crate) fn find_signature_definition_by_kind(
        &self,
        kind: &str,
    ) -> Option<&SignatureDefinition> {
        self.signatures
            .iter()
            .find(|sig_def| sig_def.node_type == kind)
    }

    /// Should this node type be treated as atomic?
    pub(crate) fn is_atomic_node_type(&self, node_type: &str) -> bool {
        self.atomic_nodes.contains(&node_type)
    }

    /// Check that all node type and field names that are used
    /// in this language profile exist in the tree-sitter language.
    /// This can be used to detect inconsistencies, for instance following
    /// an update of the grammar.
    /// This is a method on `LangProfile` and not just a test with the intention
    /// that in the future, this can become a runtime check (for dynamically loaded
    /// languages).
    #[cfg(test)]
    pub(crate) fn check_kinds(&self) -> Result<(), String> {
        let name_is_valid = |name: &'static str| {
            self.language.id_for_node_kind(name, true) != 0
                || self.language.id_for_node_kind(name, false) != 0
        };
        let field_is_valid = |name: &'static str| self.language.field_id_for_name(name).is_some();

        for atomic_node in &self.atomic_nodes {
            if !name_is_valid(atomic_node) {
                return Err(format!("invalid atomic node type: {atomic_node:?}"));
            }
        }

        for commutative_parent in &self.commutative_parents {
            commutative_parent.check_kinds(&name_is_valid)?;
        }

        for signature in &self.signatures {
            signature.check_kinds(&name_is_valid, &field_is_valid)?;
        }

        for flattened_node in self.flattened_nodes {
            if !name_is_valid(flattened_node) {
                return Err(format!("invalid flattened node type: {flattened_node:?}"));
            }
        }

        Ok(())
    }
}

/// Ways to specify the type of the parent node in a [`CommutativeParent`]
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum ParentType<'a> {
    /// Specified using the grammar node defined in the grammar
    ///
    /// This is used when a node is a commutative parent independent of the context, e.g. for sets
    ByKind(&'a str),
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
            Self::ByKind(name) => f.write_str(name),
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
            parent_type: ParentType::ByKind(root_type),
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
            parent_type: ParentType::ByKind(parent_type),
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
            parent_type: ParentType::ByKind(parent_type),
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

    /// Short-hand function to create a commutative parent with delimiters and separator, with the
    /// parent node specified using a tree-sitter query
    ///
    /// See [`ParentType::ByQuery`] for more information
    pub(crate) fn from_query_without_delimiters(
        query: &'static str,
        separator: &'static str,
    ) -> Self {
        debug_assert!(
            query.contains("@commutative"),
            "A '@commutative' capture is needed to identify which of the captured nodes is commutative, in query '{query:?}'",
        );
        Self {
            parent_type: ParentType::ByQuery(query),
            separator,
            left_delim: None,
            right_delim: None,
            children_groups: Vec::new(),
        }
    }

    /// Restrict a commutative parent to some children groups, possibly with their own separators
    pub(crate) fn restricted_to(self, children_groups: Vec<ChildrenGroup>) -> Self {
        Self {
            children_groups,
            ..self
        }
    }

    /// the type of the root node
    pub(crate) fn parent_type(&self) -> &ParentType<'_> {
        &self.parent_type
    }

    /// The default separator for children of this parent.
    ///
    /// You generally want to use [`Self::child_separator`] instead, since it returns the suitable
    /// separator for a given set of children.
    pub(crate) fn default_separator(&self) -> &'static str {
        self.separator
    }

    /// Can children with the supplied types commute together?
    /// If so, return the separator to use when inserting two nodes
    /// in the same place.
    pub(crate) fn child_separator<'a>(
        &self,
        base_nodes: &[&'a AstNode<'a>],
        left_nodes: &[&'a AstNode<'a>],
        right_nodes: &[&'a AstNode<'a>],
    ) -> Option<&'static str> {
        let trimmed_left_delim = self.left_delim.unwrap_or_default().trim();
        let trimmed_right_delim = self.right_delim.unwrap_or_default().trim();

        if (base_nodes.iter())
            .chain(left_nodes)
            .chain(right_nodes)
            .any(|node| node.is_extra)
        {
            // Extra nodes can't commute
            None
        } else if self.children_groups.is_empty() {
            // If there are no children groups to restrict commutativity to,
            // any children can commute and the default separator is used
            Some(self.separator)
        } else {
            // Otherwise, children belong to a given group if both the grammar kinds of the content nodes
            // and the contents of the separator nodes are accepted by the group.
            self.children_groups.iter().find_map(|group| {
                let group_separator = group.separator.unwrap_or(self.separator);
                (base_nodes.iter())
                    .chain(left_nodes)
                    .chain(right_nodes)
                    .all(|node| {
                        let trimmed = node.source.trim();
                        group.node_types.contains(node.kind)
                            || trimmed == group_separator.trim()
                            || trimmed == trimmed_right_delim
                            || trimmed == trimmed_left_delim
                    })
                    .then_some(group_separator)
            })
        }
    }

    /// The separator for children in this group, trimmed from leading and trailing whitespace.
    /// To obtain the separator to be inserted between two commutatively merged elements,
    /// use [`Self::child_separator`] instead.
    pub(crate) fn trimmed_separator(&self) -> &'static str {
        self.separator.trim()
    }

    /// Check that all node types contained in this object exist in the language.
    /// TODO: support checking the tree-sitter queries too (for parents defined by queries)
    #[cfg(test)]
    pub(crate) fn check_kinds<F>(&self, name_is_valid: &F) -> Result<(), String>
    where
        F: Fn(&'static str) -> bool,
    {
        if let ParentType::ByKind(name) = self.parent_type
            && !name_is_valid(name)
        {
            return Err(format!("invalid commutative node type: {name:?}"));
        }
        for children_group in &self.children_groups {
            children_group.check_kinds(name_is_valid)?;
        }
        Ok(())
    }
}

/// A group of children of a commutative node which are allowed to commute together
#[derive(Debug, Clone)]
pub struct ChildrenGroup {
    /// The types of nodes, as kinds
    pub node_types: HashSet<&'static str>,
    /// An optional separator specific to this children group,
    /// better suited than the one from the commutative parent.
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

    /// Check that all node types contained in this object exist in the language.
    #[cfg(test)]
    pub(crate) fn check_kinds<F>(&self, name_is_valid: &F) -> Result<(), String>
    where
        F: Fn(&'static str) -> bool,
    {
        for child_type in &self.node_types {
            if !name_is_valid(child_type) {
                return Err(format!("invalid commutative child type: {child_type:?}"));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{env, fs::File, io::Write, process::Command};

    use super::*;

    use crate::{signature::PathStep, test_utils::ctx};

    #[test]
    fn has_signature_conflicts() {
        let ctx = ctx();

        let with_conflicts = ctx.parse("a.json", "[{\"a\":1, \"b\":2, \"a\":3}]");
        let without_conflicts = ctx.parse("a.json", "{\"a\": [4], \"b\": [4]}");

        assert!(with_conflicts.has_signature_conflicts());
        assert!(!without_conflicts.has_signature_conflicts());
    }

    #[test]
    fn find_by_name() {
        fn find(filename: &str) -> Option<&'static LangProfile> {
            LangProfile::find_by_name(filename)
        }
        assert_eq!(find("JSON").unwrap().name, "JSON");
        assert_eq!(find("Json").unwrap().name, "JSON");
        assert_eq!(find("python").unwrap().name, "Python");
        assert_eq!(find("py").unwrap().name, "Python");
        assert_eq!(find("Java properties").unwrap().name, "Java properties");
        assert!(
            find("unknown language").is_none(),
            "Language shouldn't be found"
        );
    }

    #[test]
    fn find_by_filename_or_name() {
        fn find(filename: &str, name: Option<&str>) -> Result<&'static LangProfile, String> {
            LangProfile::find(filename, name, None)
        }
        assert_eq!(find("file.json", None).unwrap().name, "JSON");
        assert_eq!(find("file.java", Some("JSON")).unwrap().name, "JSON");
        assert!(find("java", None).is_err());
        assert_eq!(find("go.mod", None).unwrap().name, "go.mod");
        assert_eq!(find("file", Some("go.mod")).unwrap().name, "go.mod");
        assert!(find("test.go.mod", None).is_err());
        assert!(
            find("file.json", Some("non-existent language")).is_err(),
            "If a language name is provided, the file name should be ignored"
        );
        assert!(
            find("file.unknown_extension", None).is_err(),
            "Looking up language by unknown extension should fail"
        );
    }

    #[test]
    fn check_kinds() {
        let java = LangProfile::find_by_name("Java").expect("missing Java language profile");

        let wrong_atomic_nodes = LangProfile {
            atomic_nodes: vec!["foo_bar"],
            ..java.clone()
        };
        assert_eq!(
            wrong_atomic_nodes.check_kinds(),
            Err("invalid atomic node type: \"foo_bar\"".to_string())
        );

        let wrong_commutative_parent = LangProfile {
            commutative_parents: vec![CommutativeParent::without_delimiters("foo_bar", ", ")],
            ..java.clone()
        };
        assert_eq!(
            wrong_commutative_parent.check_kinds(),
            Err("invalid commutative node type: \"foo_bar\"".to_string())
        );

        let wrong_children_group = LangProfile {
            commutative_parents: vec![
                CommutativeParent::without_delimiters("program", "\n\n")
                    .restricted_to_groups(&[&["foo_bar", "class_declaration"]]),
            ],
            ..java.clone()
        };
        assert_eq!(
            wrong_children_group.check_kinds(),
            Err("invalid commutative child type: \"foo_bar\"".to_string())
        );

        let wrong_signature = LangProfile {
            signatures: vec![SignatureDefinition::new("foo_bar", vec![])],
            ..java.clone()
        };
        assert_eq!(
            wrong_signature.check_kinds(),
            Err("invalid node type for signature: \"foo_bar\"".to_string())
        );

        let wrong_field_in_path = LangProfile {
            signatures: vec![SignatureDefinition::new(
                "program",
                vec![vec![PathStep::Field("foo_bar")]],
            )],
            ..java.clone()
        };
        assert_eq!(
            wrong_field_in_path.check_kinds(),
            Err("invalid field name: \"foo_bar\"".to_string())
        );

        let wrong_type_in_path = LangProfile {
            signatures: vec![SignatureDefinition::new(
                "program",
                vec![vec![PathStep::ChildKind("foo_bar")]],
            )],
            ..java.clone()
        };
        assert_eq!(
            wrong_type_in_path.check_kinds(),
            Err("invalid child type: \"foo_bar\"".to_string())
        );

        let wrong_flattened_nodes = LangProfile {
            flattened_nodes: &["foo_bar"],
            ..java.clone()
        };
        assert_eq!(
            wrong_flattened_nodes.check_kinds(),
            Err("invalid flattened node type: \"foo_bar\"".to_string())
        );
    }

    #[test]
    fn find_by_filename_or_name_vcs() {
        let mut working_dir = env::current_exe().unwrap();
        working_dir.pop();
        let tempdir = tempfile::tempdir_in(working_dir).unwrap();

        Command::new("git")
            .arg("init")
            .current_dir(&tempdir)
            .output()
            .expect("failed to init git repository");
        {
            let attrpath = tempdir.path().join(".gitattributes");
            let mut attrfile = File::create(attrpath).unwrap();
            write!(
                &mut attrfile,
                concat!(
                    "*.bogus.mgf    mergiraf.language=bogus\n",
                    "*.js.mgf       mergiraf.language=javascript\n",
                    "*.myjs.mgf     mergiraf.language=javascript\n",
                    // Test that fallback to `linguist-language` works.
                    "unspecified.bogus.mgf  !mergiraf.language\n",
                    "unset.bogus.mgf        -mergiraf.language\n",
                    "*.bogus        linguist-language=bogus\n",
                    "*.js           linguist-language=javascript\n",
                    "*.myjs         linguist-language=javascript\n",
                    "*.bogus.mgf    linguist-language=python\n",
                ),
            )
            .unwrap();
        }
        Command::new("git")
            .args([
                "-c",
                "user.email=mergiraf@example.com",
                "-c",
                "user.name=Mergiraf Testing",
                "commit",
                "-a",
                "-m",
                "add gitattributes",
            ])
            .current_dir(&tempdir)
            .output()
            .expect("failed to commit attribute file");

        let find = |filename, name| LangProfile::find(filename, name, Some(tempdir.path()));
        assert_eq!(
            find("file.bogus.mgf", None).unwrap_err(),
            "Attribute-specified language 'bogus' could not be found",
        );
        assert_eq!(find("file.js.mgf", None).unwrap().name, "Javascript");
        assert_eq!(find("file.myjs.mgf", None).unwrap().name, "Javascript");
        assert_eq!(find("unset.bogus.mgf", None).unwrap().name, "Python");
        assert_eq!(find("unspecified.bogus.mgf", None).unwrap().name, "Python");
        assert_eq!(
            find("file.bogus", None).unwrap_err(),
            "Attribute-specified language 'bogus' could not be found",
        );
        assert_eq!(
            find("file.noattr", None).unwrap_err(),
            "Could not find a supported language for file.noattr",
        );
        assert_eq!(find("file.js", None).unwrap().name, "Javascript");
        assert_eq!(find("file.myjs", None).unwrap().name, "Javascript");
        assert_eq!(
            find("file.bogus.mgf", Some("python")).unwrap().name,
            "Python"
        );
        assert_eq!(
            find("file.noattr.mgf", Some("python")).unwrap().name,
            "Python"
        );
        assert_eq!(find("file.js.mgf", Some("python")).unwrap().name, "Python");
        assert_eq!(
            find("file.myjs.mgf", Some("python")).unwrap().name,
            "Python"
        );
        assert_eq!(find("file.bogus", Some("python")).unwrap().name, "Python");
        assert_eq!(find("file.noattr", Some("python")).unwrap().name, "Python");
        assert_eq!(find("file.js", Some("python")).unwrap().name, "Python");
        assert_eq!(find("file.myjs", Some("python")).unwrap().name, "Python");
    }
}
