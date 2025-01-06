use std::{
    borrow::Cow,
    cell::UnsafeCell,
    cmp::{max, min},
    collections::HashMap,
    fmt::Display,
    hash::{Hash, Hasher},
    ops::Range,
};

use either::Either;
use itertools::Itertools;
use tree_sitter::{Tree, TreeCursor};
use typed_arena::Arena;

use crate::lang_profile::{CommutativeParent, LangProfile};

/// A syntax tree.
///
/// All its nodes are allocated in an arena. Together with the reference to
/// the source code that was parsed, this determines the lifetime parameter.
#[derive(Debug)]
pub struct Ast<'a> {
    source: &'a str,
    root: &'a AstNode<'a>,
}

/// A node in a syntax tree.
///
/// It refers to the part of the source code it was parsed from,
/// and offers a reference to its parent node (if any).
/// It also offers a pre-computed hash value which reflects
/// the entire subtree rooted in this node. Any two isomorphic subtrees
/// will have the same hash value on their root.
#[derive(Debug)]
pub struct AstNode<'a> {
    /// A statically computed hash value, taking into account the children.
    /// It is designed to be the same for any isomorphic tree.
    pub hash: u64,
    /// The children of this node (empty if this is a leaf)
    pub children: Vec<&'a AstNode<'a>>,
    /// The children indexed by field name
    field_to_children: HashMap<&'a str, Vec<&'a AstNode<'a>>>,
    /// The portion of the source code which corresponds to this node
    pub source: &'a str,
    /// The type of node as returned by tree-sitter
    pub grammar_name: &'static str,
    /// The field name via which this node can be accessed from its parent
    pub field_name: Option<&'static str>,
    /// The range of bytes in the original source code that the source of this node spans
    pub byte_range: Range<usize>,
    /// An internal node id returned by tree-sitter, guaranteed to be unique within the tree.
    pub id: usize,
    /// A cached number of descendants
    descendant_count: usize,
    /// The parent of this node, if any.
    parent: UnsafeCell<Option<&'a AstNode<'a>>>,
    /// As the DFS of a child is a subslice of the DFS of its parent, we compute the entire DFS of
    /// the root once and slice all child DFS into this slice.
    /// This is computed right after construction and then never written to again.
    /// On nodes that have been truncated (which is rare) this will be `None`.
    dfs: UnsafeCell<Option<&'a [&'a AstNode<'a>]>>,
}

impl<'a> Ast<'a> {
    /// Create a new tree from a `tree_sitter` tree, the source code it was generated from,
    /// and an arena to allocate the nodes from.
    pub fn new(
        tree: &Tree,
        source: &'a str,
        lang_profile: &LangProfile,
        arena: &'a Arena<AstNode<'a>>,
        ref_arena: &'a Arena<&'a AstNode<'a>>,
    ) -> Result<Ast<'a>, String> {
        let root = AstNode::internal_new(&mut tree.walk(), source, lang_profile, arena)?;
        root.internal_precompute_root_dfs(ref_arena);
        Ok(Ast { source, root })
    }

    /// The height of the tree
    pub fn height(&self) -> i32 {
        self.root().height()
    }

    /// The number of nodes in the tree
    pub fn size(&self) -> usize {
        self.root().size()
    }

    /// The root of the tree
    pub fn root(&self) -> &'a AstNode<'a> {
        self.root
    }

    /// Start a Depth-First Search in prefix order on the tree
    pub fn dfs(&'a self) -> impl Iterator<Item = &'a AstNode<'a>> {
        self.root().dfs()
    }

    /// Start a Depth-First Search in postfix order on the tree
    pub fn postfix(&'a self) -> impl Iterator<Item = &'a AstNode<'a>> {
        self.root().postfix()
    }

    /// The source code this tree was parsed from
    pub fn source(&self) -> &'a str {
        self.source
    }
}

impl<'a> AstNode<'a> {
    fn internal_new<'b>(
        cursor: &mut TreeCursor<'b>,
        global_source: &'a str,
        lang_profile: &LangProfile,
        arena: &'a Arena<AstNode<'a>>,
    ) -> Result<&'a AstNode<'a>, String> {
        let mut children = Vec::new();
        let mut field_to_children: HashMap<&'a str, Vec<&'a AstNode<'a>>> = HashMap::new();
        let field_name = cursor.field_name();
        let atomic = lang_profile.is_atomic_node_type(cursor.node().grammar_name());
        if !atomic && cursor.goto_first_child() {
            let child = Self::internal_new(cursor, global_source, lang_profile, arena)?;
            children.push(child);
            if let Some(field_name) = cursor.field_name() {
                field_to_children.entry(field_name).or_default().push(child);
            }
            while cursor.goto_next_sibling() {
                let child = Self::internal_new(cursor, global_source, lang_profile, arena)?;
                children.push(child);
                if let Some(field_name) = cursor.field_name() {
                    field_to_children.entry(field_name).or_default().push(child);
                }
            }
            cursor.goto_parent();
        }
        let node = cursor.node();
        let range = node.byte_range();
        let local_source = &global_source[range.start..range.end];
        if node.is_error() {
            return Err(format!(
                "parse error at {range:?}, starting with: {}",
                &local_source[..min(32, local_source.len())]
            ));
        }

        // if this is a leaf that spans multiple lines, create one child per line,
        // to ease matching and diffing (typically, for multi-line comments)
        if children.is_empty() && local_source.contains('\n') {
            let lines = local_source.split('\n');
            let mut offset = range.start;
            for line in lines {
                let trimmed = line.trim_start();
                let start_position = offset + line.len() - trimmed.len();
                let mut hasher = crate::fxhasher();
                trimmed.hash(&mut hasher);
                children.push(arena.alloc(AstNode {
                    hash: hasher.finish(),
                    children: Vec::new(),
                    field_to_children: HashMap::new(),
                    source: trimmed,
                    grammar_name: "@virtual_line@",
                    field_name: None,
                    byte_range: Range {
                        start: start_position,
                        end: start_position + trimmed.len(),
                    },
                    id: 2 * start_position + 1, // start_position is known to be unique among virtual lines
                    descendant_count: 1,
                    parent: UnsafeCell::new(None),
                    dfs: UnsafeCell::new(None),
                }));
                offset += line.len() + 1;
            }
        }

        // pre-compute a hash value that is invariant under isomorphism
        let mut hasher = crate::fxhasher();
        if children.is_empty() {
            node.grammar_name().hash(&mut hasher);
            local_source.hash(&mut hasher);
        } else {
            node.grammar_name().hash(&mut hasher);
            children
                .iter()
                .map(|child| child.hash)
                .collect_vec()
                .hash(&mut hasher);
        };

        let descendant_count = 1 + children
            .iter()
            .map(|child| child.descendant_count)
            .sum::<usize>();

        let result = arena.alloc(AstNode {
            hash: hasher.finish(),
            children,
            field_to_children,
            source: local_source,
            grammar_name: node.grammar_name(),
            field_name,
            // parse-specific fields not included in hash/isomorphism
            byte_range: node.byte_range(),
            id: 2 * node.id(), // 2* to make it disjoint from the split lines we introduce above
            descendant_count,
            parent: UnsafeCell::new(None),
            dfs: UnsafeCell::new(None),
        });
        result.internal_set_parent_on_children();
        Ok(result)
    }

    fn internal_set_parent_on_children(&'a self) {
        for child in &self.children {
            unsafe { *child.parent.get() = Some(self) }
        }
    }

    fn internal_precompute_root_dfs(&'a self, ref_arena: &'a Arena<&'a AstNode<'a>>) {
        let mut result = vec![];

        let mut worklist = vec![self];
        while let Some(node) = worklist.pop() {
            worklist.extend(node.children.iter().rev());
            result.push(node);
        }

        let result = ref_arena.alloc_extend(result);

        fn process_node<'a>(node: &'a AstNode<'a>, result: &'a [&'a AstNode<'a>], i: &mut usize) {
            let start = *i;
            *i += 1;
            for child in &node.children {
                process_node(child, result, i);
            }
            let end = *i;
            unsafe { (*node.dfs.get()) = Some(&result[start..end]) };
        }

        let mut i = 0;
        process_node(self, result, &mut i);
    }

    /// The height of the subtree under that node
    pub fn height(&self) -> i32 {
        match self.children.iter().map(|c| c.height()).max() {
            None => 0,
            Some(x) => x + 1,
        }
    }

    /// The number of descendants of the node (including itself).
    /// If the tree was obtained by truncating some nodes, turning them into leaves,
    /// the size of each node is preserved (those leaves have a higher own weight)
    pub fn size(&self) -> usize {
        self.descendant_count
    }

    /// The weight of this node independently of the contents of this subtree.
    /// This is one unless the tree was obtained by truncation from another tree,
    /// in which case the truncated leaves have a weight that is equal to the size
    /// of their former subtree.
    pub fn own_weight(&self) -> usize {
        self.descendant_count
            - self
                .children
                .iter()
                .map(|child| child.descendant_count)
                .sum::<usize>()
    }

    /// Convenience accessor for children
    pub fn child(&self, index: usize) -> Option<&'a AstNode<'a>> {
        self.children.get(index).copied()
    }

    /// Get children by field name (children do not need to be associated to a field name,
    /// those are set in the grammar in particular rules)
    pub fn children_by_field_name(&self, field_name: &str) -> Option<&Vec<&'a AstNode<'a>>> {
        self.field_to_children.get(field_name)
    }

    /// Convenience function
    pub fn is_leaf(&self) -> bool {
        self.children.is_empty()
    }

    /// Convenience function
    pub fn is_root(&'a self) -> bool {
        self.parent().is_none()
    }

    /// Depth-first search iterator
    pub fn dfs(&'a self) -> impl Iterator<Item = &'a AstNode<'a>> {
        // SAFETY: This is not written to after construction.
        if let Some(dfs) = unsafe { &*self.dfs.get() } {
            Either::Left(dfs.iter().copied())
        } else {
            Either::Right(DfsIterator {
                current: vec![&self],
            })
        }
    }

    /// Postfix iterator
    pub fn postfix(&'a self) -> impl Iterator<Item = &'a AstNode<'a>> {
        PostfixIterator {
            queue: vec![(self, 0)],
        }
    }

    /// Ancestors iterator (which includes the node itself)
    pub fn ancestors(&'a self) -> impl Iterator<Item = &'a AstNode<'a>> {
        AncestorsIterator { cursor: Some(self) }
    }

    /// The root of the tree this node is part of
    pub fn root(&'a self) -> &'a AstNode<'a> {
        self.ancestors()
            .last()
            .expect("There must be at least one ancestor of any node: the node itself")
    }

    /// Whether this node is isomorphic to another
    pub fn isomorphic_to(&'a self, t2: &'a AstNode<'a>) -> bool {
        let mut zipped = self.dfs().zip(t2.dfs());
        self.hash == t2.hash
            && zipped.all(|(n1, n2)| {
                n1.grammar_name == n2.grammar_name
                    && n1.children.len() == n2.children.len()
                    && (!n1.children.is_empty() || n1.source == n2.source)
            })
    }

    /// Get the parent of this node, if any
    pub fn parent(&'a self) -> Option<&'a AstNode<'a>> {
        unsafe { *self.parent.get() }
    }

    /// Represent the tree as a sort of S-expression
    pub fn s_expr(&self) -> String {
        let mut output = String::new();
        self.internal_s_expr(&mut output);
        output
    }

    fn internal_s_expr(&self, output: &mut String) {
        if self.is_leaf() {
            output.push_str(self.source);
        } else {
            output.push_str(self.grammar_name);
            output.push('(');
            let mut first = true;
            for child in &self.children {
                if first {
                    first = false;
                } else {
                    output.push(' ');
                }
                child.internal_s_expr(output);
            }
            output.push(')');
        }
    }

    /// The node that comes just before this node in the list of children
    /// of its parent (if any).
    pub fn predecessor(&'a self) -> Option<&'a AstNode<'a>> {
        let parent = self.parent()?;
        let mut previous = None;
        for sibling in &parent.children {
            if sibling.id == self.id {
                return previous;
            }
            previous = Some(sibling);
        }
        None
    }

    /// The node that comes just after this node in the list of children
    /// of its parent (if any).
    pub fn successor(&'a self) -> Option<&'a AstNode<'a>> {
        let parent = self.parent()?;
        parent
            .children
            .iter()
            .skip_while(|sibling| sibling.id != self.id)
            .dropping(1)
            .copied()
            .next()
    }

    /// Truncate a tree so that all nodes selected by the predicate are treated as leaves
    pub fn truncate<'b, F>(
        &'a self,
        predicate: &F,
        arena: &'b Arena<AstNode<'b>>,
    ) -> &'b AstNode<'b>
    where
        F: Fn(&'a AstNode<'a>) -> bool,
        'a: 'b,
    {
        let truncate = predicate(self);
        let children = if truncate {
            Vec::new()
        } else {
            self.children
                .iter()
                .map(|child| child.truncate(predicate, arena))
                .collect()
        };
        let field_to_children = if truncate {
            HashMap::new()
        } else {
            let child_id_map: HashMap<usize, &'b AstNode<'b>> =
                children.iter().map(|child| (child.id, *child)).collect();
            self.field_to_children
                .iter()
                .map(|(k, v)| (*k, v.iter().map(|child| child_id_map[&child.id]).collect()))
                .collect()
        };
        let result = arena.alloc(AstNode {
            children,
            field_to_children,
            byte_range: self.byte_range.clone(),
            parent: UnsafeCell::new(None),
            dfs: UnsafeCell::new(None),
            ..*self
        });
        result.internal_set_parent_on_children();
        result
    }

    /// Any whitespace that precedes this node.
    /// This will be None if the node doesn't have a predecessor,
    /// otherwise it's the whitespace between its predecessor and itself.
    pub fn preceding_whitespace(&'a self) -> Option<&'a str> {
        let parent = self.parent()?;
        let predecessor = self.predecessor()?;
        // in some grammars (such as Go), newlines can be parsed explicitly as nodes, meaning that
        // the whitespace at the end of the predecessor should be included as well in what we return here.
        let predecessor_end_whitespace =
            predecessor.source.len() - predecessor.source.trim_end().len();
        let start =
            predecessor.byte_range.end - predecessor_end_whitespace - parent.byte_range.start;
        let end = self.byte_range.start - parent.byte_range.start;
        Some(&parent.source[start..end])
    }

    /// Raw indentation that precedes this node.
    /// This is None if the preceding whitespace does not contain any newline.
    pub fn preceding_indentation(&'a self) -> Option<&'a str> {
        let whitespace = self.preceding_whitespace()?;
        let last_newline = whitespace.rfind('\n')?;
        Some(&whitespace[(last_newline + 1)..])
    }

    /// Any whitespace between the end of the last child of this node
    /// and the end of this node itself
    pub fn trailing_whitespace(&'a self) -> Option<&'a str> {
        let last_child = self.children.last()?;
        let additional_content =
            &self.source[(last_child.byte_range.end - self.byte_range.start)..];
        if !additional_content.is_empty() && additional_content.trim().is_empty() {
            Some(additional_content)
        } else {
            None
        }
    }

    /// The preceding indentation of the first ancestor that has some.
    pub fn ancestor_indentation(&'a self) -> Option<&'a str> {
        self.ancestors()
            .skip(1)
            .find_map(|ancestor| ancestor.preceding_indentation())
    }

    /// The difference between this node's preceding indentation and
    /// its ancestor indentation.
    pub fn indentation_shift(&'a self) -> Option<&'a str> {
        let own_indentation = self.preceding_indentation()?;
        match self.ancestor_indentation() {
            Some(ancestor_indentation) => {
                own_indentation
                    .strip_prefix(ancestor_indentation)
                    // if the indentation of the ancestor isn't a prefix of the current indentation,
                    // tabs and spaces might be mixed, so we try again with tabs normalized to 4 spaces
                    .or_else(|| {
                        [
                            Self::extract_indentation_suffix_from_mixed_spaces_and_tabs(
                                own_indentation,
                                ancestor_indentation,
                                8,
                            ),
                            Self::extract_indentation_suffix_from_mixed_spaces_and_tabs(
                                own_indentation,
                                ancestor_indentation,
                                4,
                            ),
                        ]
                        .into_iter()
                        .flatten()
                        .min_by_key(|indentation| indentation.len()) // try to find the minimal shift
                    })
            }
            None => Some(own_indentation),
        }
    }

    fn extract_indentation_suffix_from_mixed_spaces_and_tabs<'b>(
        own_indentation: &'b str,
        ancestor_indentation: &'b str,
        tab_width: usize,
    ) -> Option<&'b str> {
        let tab_spaces = " ".repeat(tab_width);
        let own_spaces = own_indentation.replace('\t', &tab_spaces);
        let suffix = own_spaces.strip_prefix(&ancestor_indentation.replace('\t', &tab_spaces))?;
        // convert back the suffix of the normalized strings to a suffix of the original string
        let mut idx = own_indentation.len();
        let mut remaining_spaces = suffix.len();
        for char in own_indentation.chars().rev() {
            if remaining_spaces == 0 {
                break;
            }
            let width = match char {
                '\t' => tab_width,
                _ => 1,
            };
            remaining_spaces = remaining_spaces.saturating_sub(width);
            idx -= char.len_utf8();
        }
        idx = max(idx, 0);
        if idx < own_indentation.len() {
            Some(&own_indentation[idx..])
        } else {
            None
        }
    }

    /// The source of this node, stripped from any indentation inherited by the node or its ancestors
    pub fn unindented_source(&'a self) -> Cow<'a, str> {
        match self.preceding_indentation().or(self.ancestor_indentation()) {
            Some(indentation) => {
                // TODO FIXME this is invalid for multiline string literals!
                Cow::from(self.source.replace(&format!("\n{indentation}"), "\n"))
            }
            None => Cow::from(self.source),
        }
    }

    /// The source of this node, stripped from any indentation inherited by the node or its ancestors
    /// and shifted back to the desired indentation.
    pub fn reindented_source(&'a self, new_indentation: &str) -> String {
        let new_newlines = format!("\n{new_indentation}");
        let indentation = (self.preceding_indentation())
            .or(self.ancestor_indentation())
            .unwrap_or("");
        self.source
            .replace(&format!("\n{indentation}"), &new_newlines) // TODO FIXME this is invalid for multiline string literals!
    }

    /// Source of the node, including any whitespace before and after,
    /// but only within the bounds of the node's parent.
    pub fn source_with_surrounding_whitespace(&'a self) -> &'a str {
        if let Some(parent) = self.parent() {
            let mut start = self.byte_range.start;
            let mut end = self.byte_range.end;
            if let Some(predecessor) = self.predecessor() {
                start = predecessor.byte_range.end;
            }
            if let Some(successor) = self.successor() {
                end = successor.byte_range.start;
            }
            let parent_start = parent.byte_range.start;
            &parent.source[(start - parent_start)..(end - parent_start)]
        } else {
            self.source
        }
    }

    /// Represents the node and its sub-structure in ASCII art
    pub fn ascii_tree(&'a self, lang_profile: &LangProfile) -> String {
        self.internal_ascii_tree("\x1b[0;90m", true, lang_profile, None)
    }

    fn internal_ascii_tree(
        &'a self,
        prefix: &str,
        last_child: bool,
        lang_profile: &LangProfile,
        parent: Option<&CommutativeParent>,
    ) -> String {
        let num_children = self.children.len();
        let next_parent = lang_profile.get_commutative_parent(self.grammar_name);

        let tree_sym = if last_child { "└" } else { "├" };

        let key = (self.field_name)
            .map(|key| format!("{key}: "))
            .unwrap_or_default();

        let grammar_name = if self.source != self.grammar_name {
            self.grammar_name
        } else {
            &format!("\x1b[0;31m{}\x1b[0m", self.grammar_name)
        };

        let source = (num_children == 0 && self.source != self.grammar_name)
            .then(|| format!(" \x1b[0;31m{}\x1b[0m", self.source.replace('\n', "\\n")))
            .unwrap_or_default();

        let commutative = (next_parent.is_some())
            .then_some(" \x1b[0;95mCommutative\x1b[0m")
            .unwrap_or_default();

        let sig = (parent.is_some())
            .then(|| {
                lang_profile
                    .extract_signature_from_original_node(self)
                    .map(|sig| format!(" \x1b[0;96m{sig}\x1b[0m"))
            })
            .flatten()
            .unwrap_or_default();

        std::iter::once(format!(
            "{prefix}{tree_sym}{key}\x1b[0m{grammar_name}{source}{commutative}{sig}\n"
        ))
        .chain(
            self.children
                .iter()
                .enumerate()
                .filter(|(_, child)| child.grammar_name != "@virtual_line@")
                .map(|(index, child)| {
                    let new_prefix = format!("{prefix}{} ", if last_child { " " } else { "│" });
                    child.internal_ascii_tree(
                        &new_prefix,
                        index == num_children - 1,
                        lang_profile,
                        next_parent,
                    )
                }),
        )
        .collect()
    }
}

/// We pre-compute hash values for all nodes,
/// so we make sure those are used instead of recursively walking the tree
/// each time a hash is computed.
impl Hash for AstNode<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.hash.hash(state);
        self.id.hash(state);
        self.grammar_name.hash(state);
        self.byte_range.hash(state);
    }
}

impl PartialEq for AstNode<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash
            && self.id == other.id
            && self.grammar_name == other.grammar_name
            && self.byte_range == other.byte_range
    }
}

impl Eq for AstNode<'_> {}

// AstNode fails to be Sync by default because it contains
// an UnsafeCell. But this cell is only mutated during initialization and only
// ever refers to something that lives as long as the node itself (thanks to the
// use of arenas) so it's fine to share it across threads.
unsafe impl Sync for AstNode<'_> {}
unsafe impl Send for AstNode<'_> {}

impl Display for AstNode<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}…{}",
            self.grammar_name, self.byte_range.start, self.byte_range.end
        )
    }
}

struct DfsIterator<'a> {
    current: Vec<&'a AstNode<'a>>,
}

impl<'a> Iterator for DfsIterator<'a> {
    type Item = &'a AstNode<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let node = self.current.pop()?;
        self.current.extend(node.children.iter().rev());
        Some(node)
    }
}

struct PostfixIterator<'a> {
    queue: Vec<(&'a AstNode<'a>, usize)>,
}

impl<'a> Iterator for PostfixIterator<'a> {
    type Item = &'a AstNode<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let (node, visited_children) = self.queue.pop()?;
        match node.child(visited_children) {
            None => Some(node),
            Some(child) => {
                self.queue.push((node, visited_children + 1));
                self.queue.push((child, 0));
                self.next()
            }
        }
    }
}

struct AncestorsIterator<'src> {
    cursor: Option<&'src AstNode<'src>>,
}

impl<'a> Iterator for AncestorsIterator<'a> {
    type Item = &'a AstNode<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let to_return = self.cursor?;
        self.cursor = to_return.parent();
        Some(to_return)
    }
}

#[cfg(test)]
mod tests {

    use itertools::Itertools;
    use regex::Regex;

    use crate::test_utils::ctx;

    use super::*;

    #[test]
    fn check_heights() {
        let ctx = ctx();

        assert_eq!(ctx.parse_json("null").height(), 1);
        assert_eq!(ctx.parse_json("[1]").height(), 2);
        assert_eq!(ctx.parse_json("{\"foo\": 3}").height(), 4);
    }

    #[test]
    fn check_sizes() {
        let ctx = ctx();

        assert_eq!(ctx.parse_json("null").size(), 2);
        assert_eq!(ctx.parse_json("[1]").size(), 5);
        assert_eq!(ctx.parse_json("{\"foo\": 3}").size(), 11);
    }

    #[test]
    fn check_children_by_field_names() {
        let ctx = ctx();

        let root = ctx.parse_json("{\"foo\": 3}").root();
        let object = root.child(0).unwrap();
        let pair = object.child(1).unwrap();
        assert_eq!(root.children_by_field_name("non_existent"), None);
        assert_eq!(
            pair.children_by_field_name("key")
                .unwrap()
                .first()
                .unwrap()
                .source,
            "\"foo\""
        );
    }

    #[test]
    fn check_children_by_field_names_with_modifiers() {
        let ctx = ctx();

        let root = ctx.parse_java("public class MyCls {}").root();
        let class_declaration = root.child(0).unwrap();
        assert_eq!(
            class_declaration.children_by_field_name("name"),
            Some(&vec![class_declaration.child(2).unwrap()])
        );
    }

    #[test]
    fn check_atomic_nodes() {
        let ctx = ctx();

        let root = ctx.parse_java("import java.io.InputStream;").root();
        let import_statement = root.child(0).unwrap();
        assert_eq!(import_statement.children.len(), 0);
    }

    #[test]
    fn check_s_expr() {
        let ctx = ctx();

        assert_eq!(
            ctx.parse_json("{\"foo\": 3}").root().s_expr(),
            "document(object({ pair(string(\" foo \") : 3) }))"
        );
    }

    #[test]
    fn hashing_does_not_depend_on_whitespace_but_on_content() {
        let ctx = ctx();

        let hash_1 = &ctx.parse_rust("fn x() -> i32 { 7 - 1 }").root().hash;
        let hash_2 = &ctx.parse_rust("fn x() -> i32 {\n 7-1 }").root().hash;
        let hash_3 = &ctx.parse_rust("fn x() -> i32 {\n 9-2 }").root().hash;

        assert_eq!(hash_1, hash_2); // whitespace and indentation differences are insignificant
        assert_ne!(hash_2, hash_3);

        let hash_4 = &ctx.parse_rust("fn x() { \"some string\" }").root().hash;
        let hash_5 = &ctx.parse_rust("fn x() { \" some string\" }").root().hash;
        let hash_6 = &ctx.parse_rust("fn x() {   \"some string\" }").root().hash;
        assert_ne!(hash_4, hash_5); // whitespace inside of a string is significant
        assert_eq!(hash_4, hash_6);
    }

    #[test]
    fn isomorphism_is_not_just_hashing() {
        let ctx = ctx();

        let node_1 = ctx.parse_rust("fn x() -> i32 { 7 - 1 }").root();
        let node_2 = ctx.parse_rust("fn x() -> i32 { 8 - 1 }").root();
        let fake_hash_collision = AstNode {
            hash: node_1.hash,
            parent: UnsafeCell::new(None),
            dfs: UnsafeCell::new(None),
            children: node_2.children.to_owned(),
            field_to_children: HashMap::new(),
            byte_range: node_2.byte_range.to_owned(),
            ..*node_2
        };

        assert_eq!(node_1.hash, fake_hash_collision.hash);
        assert!(!node_1.isomorphic_to(&fake_hash_collision));
    }

    #[test]
    fn parents_are_accessible() {
        let ctx = ctx();
        let tree = ctx.parse_json("{\"foo\": 3}");
        let root = tree.root();
        let first_child = root.child(0).expect("AST node is missing a child");
        let second_child = first_child
            .child(0)
            .expect("First child should also have a child itself");

        assert_eq!(root.parent(), None);
        assert_eq!(first_child.parent(), Some(root));
        assert_eq!(second_child.parent(), Some(first_child));
    }

    #[test]
    fn dfs_traversal() {
        let ctx = ctx();
        let tree = ctx.parse_json("{\"foo\": 3}");

        let node_types = tree.root().dfs().map(|n| n.grammar_name).collect_vec();

        assert_eq!(
            node_types,
            vec![
                "document",
                "object",
                "{",
                "pair",
                "string",
                "\"",
                "string_content",
                "\"",
                ":",
                "number",
                "}"
            ]
        );
    }

    #[test]
    fn postfix_traversal() {
        let ctx = ctx();
        let tree = ctx.parse_json("{\"foo\": 3}");

        let node_types = tree.root().postfix().map(|n| n.grammar_name).collect_vec();

        assert_eq!(
            node_types,
            vec![
                "{",
                "\"",
                "string_content",
                "\"",
                "string",
                ":",
                "number",
                "pair",
                "}",
                "object",
                "document"
            ]
        );
    }

    #[test]
    fn truncate() {
        let ctx = ctx();
        let tree = ctx.parse_json("{\"foo\": 3, \"bar\": 4}");

        let arena = Arena::new();
        let truncated = tree
            .root()
            .truncate(&|node| node.grammar_name == "pair", &arena);

        let node_types = truncated.postfix().map(|n| n.grammar_name).collect_vec();

        let truncated_object = truncated.root().child(0).unwrap();
        let original_object = tree.root().child(0).unwrap();
        let truncated_first_pair = truncated_object.child(1).unwrap();
        let original_first_pair = original_object.child(1).unwrap();

        assert_eq!(truncated_object.id, original_object.id);
        assert_eq!(truncated_first_pair.id, original_first_pair.id);
        assert_eq!(truncated.size(), tree.size());
        assert_eq!(
            node_types,
            vec!["{", "pair", ",", "pair", "}", "object", "document"]
        );
    }

    #[test]
    fn preceding_whitespace() {
        let ctx = ctx();
        let tree = ctx.parse_json("[1, 2,\n 3]");

        let root = tree.root().child(0).unwrap();
        let bracket = root.child(0).unwrap();
        let one = root.child(1).unwrap();
        let comma = root.child(2).unwrap();
        let two = root.child(3).unwrap();
        let three = root.child(5).unwrap();

        assert_eq!(root.preceding_whitespace(), None);
        assert_eq!(bracket.preceding_whitespace(), None);
        assert_eq!(one.preceding_whitespace(), Some(""));
        assert_eq!(comma.preceding_whitespace(), Some(""));
        assert_eq!(two.preceding_whitespace(), Some(" "));
        assert_eq!(three.preceding_whitespace(), Some("\n "));
    }

    #[test]
    fn preceding_whitespace_go() {
        let ctx = ctx();
        let tree = ctx.parse_go("import (\n    \"fmt\"\n    \"core\"\n)\n");
        let root = tree.root().child(0).unwrap();
        let import_list = root.child(1).unwrap();
        let core = import_list.child(2).unwrap();
        assert_eq!(core.source, "\"core\"");
        assert_eq!(core.preceding_whitespace(), Some("\n    "));
        assert_eq!(core.ancestor_indentation(), None);
    }

    #[test]
    fn trailing_whitespace_toml() {
        let ctx = ctx();
        let tree = ctx.parse_toml("[foo]\na = 1\n\n[bar]\nb = 2");
        let first_table = tree.root().child(0).unwrap();
        let second_table = tree.root().child(1).unwrap();
        assert_eq!(first_table.source, "[foo]\na = 1\n\n");
        assert_eq!(first_table.trailing_whitespace(), Some("\n\n"));
        assert_eq!(second_table.source, "[bar]\nb = 2");
        assert_eq!(second_table.trailing_whitespace(), None);
    }

    #[test]
    fn preceding_indentation_shift() {
        let ctx = ctx();
        let tree = ctx.parse_java("\nclass MyCls {\n    int attr;\n}");
        let class_decl = tree.root().child(0).unwrap();
        let class_body = class_decl.child(2).unwrap();
        let attr = class_body.child(1).unwrap();

        assert_eq!(attr.indentation_shift(), Some("    "));
    }

    #[test]
    fn preceding_indentation_shift_tabs() {
        let ctx = ctx();
        let tree = ctx.parse_java("class Outer {\n\tclass MyCls {\n\t\tint attr;\n\t}\n}\n");
        let class_decl = tree
            .root()
            .child(0)
            .unwrap()
            .child(2)
            .unwrap()
            .child(1)
            .unwrap();
        let class_body = class_decl.child(2).unwrap();
        let attr = class_body.child(1).unwrap();

        assert_eq!(attr.indentation_shift(), Some("\t"));
    }

    #[test]
    fn preceding_indentation_shift_mixed_spaces_and_tabs() {
        let ctx = ctx();
        let tree = ctx.parse_java("class Outer {\n\tclass MyCls {\n        int attr;\n\t}\n}\n");
        let class_decl = tree
            .root()
            .child(0)
            .unwrap()
            .child(2)
            .unwrap()
            .child(1)
            .unwrap();
        let class_body = class_decl.child(2).unwrap();
        let attr = class_body.child(1).unwrap();

        assert_eq!(attr.indentation_shift(), Some("    "));
    }

    #[test]
    fn preceding_indentation_shift_mixed_tabs_and_spaces() {
        let ctx = ctx();
        let tree = ctx.parse_java("class Outer {\n    class MyCls {\n\t\tint attr;\n    }\n}\n");
        let class_decl = tree
            .root()
            .child(0)
            .unwrap()
            .child(2)
            .unwrap()
            .child(1)
            .unwrap();
        let class_body = class_decl.child(2).unwrap();
        let attr = class_body.child(1).unwrap();

        assert_eq!(attr.indentation_shift(), Some("\t"));
    }

    #[test]
    fn reindent_yaml() {
        let ctx = ctx();
        let tree = ctx.parse_yaml("hello:\n  foo: 2\nbar: 4\n");
        let block_node = tree.root().child(0).unwrap().child(0).unwrap();
        assert_eq!(block_node.grammar_name, "block_node");
        let value = block_node
            .child(0)
            .unwrap()
            .child(0)
            .unwrap()
            .child(2)
            .unwrap();
        assert_eq!(value.grammar_name, "block_node");

        assert_eq!(block_node.indentation_shift(), None);
        assert_eq!(value.indentation_shift(), Some("  "));
    }

    #[test]
    fn source_with_whitespace() {
        let ctx = ctx();
        let tree = ctx.parse_json(" [ 1 , 2,\n 3]");

        let root = tree.root().child(0).unwrap();
        let bracket = root.child(0).unwrap();
        let one = root.child(1).unwrap();
        let comma = root.child(2).unwrap();
        let two = root.child(3).unwrap();
        let comma_2 = root.child(4).unwrap();

        assert_eq!(bracket.source_with_surrounding_whitespace(), "[ ");
        assert_eq!(one.source_with_surrounding_whitespace(), " 1 ");
        assert_eq!(comma.source_with_surrounding_whitespace(), " , ");
        assert_eq!(two.source_with_surrounding_whitespace(), " 2");
        assert_eq!(comma_2.source_with_surrounding_whitespace(), ",\n ");
    }

    #[test]
    fn removing_indentation() {
        let ctx = ctx();

        let tree = ctx.parse_json(
            r#"
{
    "a": [
        1,
        2,
    ],
    "b": {
        "c": "foo"
    }
}
"#,
        );

        let root = tree.root().child(0).unwrap();
        let entry_a = root.child(1).unwrap();
        let array = entry_a.child(2).unwrap();

        assert_eq!(entry_a.source, "\"a\": [\n        1,\n        2,\n    ]");
        assert_eq!(entry_a.indentation_shift(), Some("    "));
        assert_eq!(entry_a.ancestor_indentation(), None);
        assert_eq!(entry_a.unindented_source(), "\"a\": [\n    1,\n    2,\n]");
        assert_eq!(
            entry_a.reindented_source("  "),
            "\"a\": [\n      1,\n      2,\n  ]"
        );

        assert_eq!(array.source, "[\n        1,\n        2,\n    ]");
        assert_eq!(array.indentation_shift(), None);
        assert_eq!(array.ancestor_indentation(), Some("    "));
        assert_eq!(array.unindented_source(), "[\n    1,\n    2,\n]");
        assert_eq!(array.reindented_source("  "), "[\n      1,\n      2,\n  ]");
    }

    #[test]
    fn multiline_comments_are_isomorphic() {
        let ctx = ctx();

        let comment_1 = ctx
            .parse_java("/**\n * This is a comment\n * spanning on many lines\n*/")
            .root()
            .child(0)
            .unwrap();
        let comment_2 = ctx
            .parse_java("  /**\n   * This is a comment\n   * spanning on many lines\n  */")
            .root()
            .child(0)
            .unwrap();

        assert!(comment_1.isomorphic_to(comment_2));
        assert_eq!(comment_1.children.len(), 4);
        assert_eq!(
            comment_2
                .children
                .iter()
                .map(|child| child.source)
                .collect_vec(),
            vec![
                "/**",
                "* This is a comment",
                "* spanning on many lines",
                "*/"
            ]
        );
        assert_eq!(
            comment_2
                .children
                .iter()
                .map(|child| &child.byte_range)
                .collect_vec(),
            vec![
                &Range { start: 2, end: 5 },
                &Range { start: 9, end: 28 },
                &Range { start: 32, end: 56 },
                &Range { start: 59, end: 61 },
            ]
        );
        assert_eq!(
            comment_2.children.get(1).unwrap().preceding_whitespace(),
            Some("\n   ")
        );
    }

    #[test]
    fn print_as_ascii_art() {
        let ctx = ctx();
        let tree = ctx.parse_json("{\"foo\": 3, \"bar\": 4}");
        let lang_profile = LangProfile::detect_from_filename("file.json")
            .expect("Could not find JSON language profile");

        let ascii_tree = tree.root().ascii_tree(lang_profile);

        let re = Regex::new("\x1b\\[0(;[0-9]*)?m").unwrap();
        let without_colors = re.replace_all(&ascii_tree, "");

        let expected = r#"└document
  └object Commutative
    ├{
    ├pair Signature [["foo"]]
    │ ├key: string
    │ │ ├"
    │ │ ├string_content foo
    │ │ └"
    │ ├:
    │ └value: number 3
    ├,
    ├pair Signature [["bar"]]
    │ ├key: string
    │ │ ├"
    │ │ ├string_content bar
    │ │ └"
    │ ├:
    │ └value: number 4
    └}
"#;

        assert_eq!(without_colors, expected);
    }
}
