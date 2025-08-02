#[cfg(feature = "dev")]
use std::iter::zip;
use std::{
    borrow::Cow,
    cell::UnsafeCell,
    cmp::max,
    fmt::Display,
    hash::{Hash, Hasher},
    ops::Range,
};
#[cfg(test)]
use std::{ops::Index, slice::SliceIndex};

use either::Either;
use itertools::Itertools;
use nu_ansi_term::Color;
use rustc_hash::FxHashMap;
use tree_sitter::{
    Parser, Query, QueryCursor, Range as TSRange, StreamingIterator, Tree, TreeCursor,
};
use typed_arena::Arena;

use crate::{
    StrExt,
    lang_profile::{CommutativeParent, LangProfile, ParentType},
    signature::{Signature, SignatureDefinition},
};

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
    pub children: Vec<&'a Self>,
    /// The children indexed by field name
    field_to_children: FxHashMap<&'a str, Vec<&'a Self>>,
    /// The portion of the source code which corresponds to this node
    pub source: &'a str,
    /// The type of node as returned by tree-sitter
    pub grammar_name: &'static str,
    /// The field name via which this node can be accessed from its parent
    pub field_name: Option<&'static str>,
    /// The range of bytes in the original source code that the source of this node spans
    pub byte_range: Range<usize>,
    /// An internal node id, guaranteed to be unique within the tree.
    pub id: usize,
    /// A cached number of descendants
    descendant_count: usize,
    /// The parent of this node, if any.
    parent: UnsafeCell<Option<&'a Self>>,
    /// The commutative merging settings associated with this node.
    commutative_parent: Option<&'a CommutativeParent>,
    /// As the DFS of a child is a subslice of the DFS of its parent, we compute the entire DFS of
    /// the root once and slice all child DFS into this slice.
    /// This is computed right after construction and then never written to again.
    /// On nodes that have been truncated (which is rare) this will be `None`.
    dfs: UnsafeCell<Option<&'a [&'a Self]>>,
    /// The language this node was parsed from
    pub lang_profile: &'a LangProfile,
}

impl<'a> AstNode<'a> {
    /// Parse a string to a tree using the language supplied
    pub fn parse(
        source: &'a str,
        lang_profile: &'a LangProfile,
        arena: &'a Arena<Self>,
        ref_arena: &'a Arena<&'a Self>,
    ) -> Result<&'a Self, String> {
        let mut next_node_id = 1;
        let root = Self::parse_root(source, None, lang_profile, arena, &mut next_node_id)?;
        root.internal_precompute_root_dfs(ref_arena);
        Ok(root)
    }

    /// Internal method to parse a string to a tree,
    /// without doing the DFS precomputation and starting
    /// allocation of node ids at the supplied counter.
    fn parse_root(
        source: &'a str,
        range: Option<TSRange>,
        lang_profile: &'a LangProfile,
        arena: &'a Arena<Self>,
        next_node_id: &mut usize,
    ) -> Result<&'a Self, String> {
        let mut parser = Parser::new();
        parser
            .set_language(&lang_profile.language)
            .map_err(|err| format!("Error loading {lang_profile} grammar: {err}"))?;
        if let Some(range) = range {
            parser
                .set_included_ranges(&[range])
                .map_err(|err| format!("Error while restricting the parser to a range: {err}"))?;
        }
        let tree = parser
            .parse(source, None)
            .expect("Parsing source code failed");
        let node_id_to_injection_lang = Self::locate_injections(&tree, source, lang_profile);
        let node_id_to_commutative_parent =
            Self::locate_commutative_parents_by_query(&tree, source, lang_profile);
        let range_for_root = if let Some(range) = range {
            range.start_byte..range.end_byte
        } else {
            0..source.len()
        };
        Self::internal_new(
            &mut tree.walk(),
            source,
            lang_profile,
            arena,
            next_node_id,
            &node_id_to_injection_lang,
            &node_id_to_commutative_parent,
            Some(range_for_root),
        )
    }

    /// Locate all nodes which are marked as commutative via a tree-sitter query.
    /// This returns a map from node ids to the their commutative parent definition.
    fn locate_commutative_parents_by_query<'b>(
        tree: &Tree,
        source: &'a str,
        lang_profile: &'b LangProfile,
    ) -> FxHashMap<usize, &'b CommutativeParent> {
        let mut node_id_to_commutative_parent = FxHashMap::default();
        // For each commutative parent that is defined by a tree-sitter query
        for commutative_parent in &lang_profile.commutative_parents {
            if let ParentType::ByQuery(query_str) = commutative_parent.parent_type() {
                // Execute this query over the tree
                let query = Query::new(&lang_profile.language, query_str)
                    .expect("Invalid commutative parent query");
                let commutative_capture_index = query
                    .capture_index_for_name("commutative")
                    .expect("Commutative parent query without a '@commutative' capture");
                let mut cursor = QueryCursor::new();
                let matches = cursor.matches(&query, tree.root_node(), source.as_bytes());
                // For each match, mark the captured node(s) as commutative
                matches.for_each(|m| {
                    node_id_to_commutative_parent.extend(
                        m.nodes_for_capture_index(commutative_capture_index)
                            .map(|node| (node.id(), commutative_parent)),
                    );
                });
            }
        }
        node_id_to_commutative_parent
    }

    /// Locate nodes which need re-parsing in a different language
    fn locate_injections(
        tree: &Tree,
        source: &'a str,
        lang_profile: &'a LangProfile,
    ) -> FxHashMap<usize, &'static LangProfile> {
        let Some(query_str) = lang_profile.injections else {
            return FxHashMap::default();
        };
        let mut node_id_to_injection_lang = FxHashMap::default();
        let query = Query::new(&lang_profile.language, query_str).expect("Invalid injection query");
        let content_capture_index = query
            .capture_index_for_name("injection.content")
            .expect("Injection query without an injection.content capture");
        // The injection.language can be defined either as a capture (dynamically changing), with value defined by a node in the AST,
        // or statically defined as a property (fixed by the injection query), in which case the capture below won't be defined.
        let language_capture_index = query.capture_index_for_name("injection.language");
        let mut cursor = QueryCursor::new();
        let matches = cursor.matches(&query, tree.root_node(), source.as_bytes());
        matches.for_each(|m| {
            let pattern_properties = query.property_settings(m.pattern_index);
            // first, check if the language is statically defined in this clause of the query as a property
            let lang_property_value = pattern_properties.iter().find_map(|property| {
                if &*property.key == "injection.language" {
                    property.value.as_deref()
                } else {
                    None
                }
            });

            let language = lang_property_value.unwrap_or_else(|| {
                // otherwise, look if the language is defined via a capture
                let capture_index = language_capture_index
                    .expect("injection.language is set neither as a capture nor as a property");
                let lang_node = m
                    .nodes_for_capture_index(capture_index)
                    .next()
                    .expect("injection.language capture didn't match any node");
                &source[lang_node.byte_range()]
            });
            if let Some(injected_lang) = LangProfile::find_by_name(language) {
                node_id_to_injection_lang.extend(
                    m.nodes_for_capture_index(content_capture_index)
                        .map(|node| (node.id(), injected_lang)),
                );
            } // if we can't find a fitting language, then leave the injection contents unparsed, without failing the overarching parsing
        });
        node_id_to_injection_lang
    }

    #[allow(clippy::too_many_arguments)]
    fn internal_new<'b>(
        cursor: &mut TreeCursor<'b>,
        global_source: &'a str,
        lang_profile: &'a LangProfile,
        arena: &'a Arena<Self>,
        next_node_id: &mut usize,
        node_id_to_injection_lang: &FxHashMap<usize, &'static LangProfile>,
        node_id_to_commutative_parent: &FxHashMap<usize, &'a CommutativeParent>,
        range_for_root: Option<Range<usize>>,
    ) -> Result<&'a Self, String> {
        let field_name = cursor.field_name();
        let node = cursor.node();
        let atomic = lang_profile.is_atomic_node_type(node.grammar_name());

        let mut children = Vec::new();
        let mut field_to_children: FxHashMap<&'a str, Vec<&'a Self>> = FxHashMap::default();
        let mut last_child_end = node.byte_range().start;
        // for nodes that we flatten, track the number of children this will add so that
        // we are able to allocate the new children vector efficiently.
        let mut children_added_by_flattening = 0;

        // check if the current node is an injection
        let injection_lang = node_id_to_injection_lang.get(&node.id());
        if let Some(&injection_lang) = injection_lang {
            let range = node.range();
            if let Ok(injected_root) = Self::parse_root(
                global_source,
                Some(range),
                injection_lang,
                arena,
                next_node_id,
            ) {
                children.push(injected_root);
                last_child_end = injected_root.byte_range.end;
            } // if the parsing of the injection fails, keep the injection node as a leaf but don't abort the entire parsing
        } else if !atomic && cursor.goto_first_child() {
            let mut child_available = true;
            while child_available {
                let child = Self::internal_new(
                    cursor,
                    global_source,
                    lang_profile,
                    arena,
                    next_node_id,
                    node_id_to_injection_lang,
                    node_id_to_commutative_parent,
                    None,
                )?;
                children.push(child);
                if let Some(field_name) = cursor.field_name() {
                    field_to_children.entry(field_name).or_default().push(child);
                }
                if cursor.node().grammar_id() == node.grammar_id() {
                    children_added_by_flattening += child.children.len() - 1;
                }
                debug_assert!(
                    child.byte_range.start >= last_child_end,
                    "Child starts earlier than its previous sibling ends"
                );
                debug_assert!(
                    child.byte_range.end <= node.byte_range().end,
                    "Child expands further than its parent"
                );
                last_child_end = child.byte_range.end;
                child_available = cursor.goto_next_sibling();
            }
            cursor.goto_parent();
        }

        // Strip any trailing newlines from the node's source, because we're better
        // off treating this as whitespace between nodes, to keep track of indentation shifts
        let range = node.byte_range();
        let local_source = &global_source[range.start..range.end];
        let range = if let Some(range_for_root) = range_for_root {
            if children.is_empty() {
                // This is a root with no children, that is to say an empty source.
                // If we were to use `range_for_root` here too, then that would mean
                // that two different source files with a different amount of whitespace
                // wouldn't be treated as isomorphic by Mergiraf. This is because those roots
                // are technically leaves as they don't have children.
                // For leaves, our isomorphism checking logic will require the string contents
                // of those leaves to be exactly equal to declare two empty trees as isomorphic.
                // This is rather bad. So to avoid that, we allow for one exception to the rule,
                // meaning that we won't preserve whitespace when one side to merge is empty,
                // which should be okay.
                range_for_root.start..range_for_root.start
            } else {
                range_for_root.clone()
            }
        } else if local_source.ends_with('\n') && node.parent().is_some() {
            let trimmed_source = local_source.trim_end_matches('\n');
            // The range's end is shifted back by as many newlines we can remove
            // at the end, but may not end before the end of its last child,
            // to maintain the compatibility between the tree structure and the ranges.
            // (Some children can have an empty source and so their own trimming
            // wouldn't keep them contained.)
            let new_end = max(
                range.end - local_source.len() + trimmed_source.len(),
                last_child_end,
            );
            range.start..new_end
        } else {
            range
        };
        let local_source = &global_source[range.start..range.end];
        if node.is_error() {
            let full_range = node.range();

            // it can be that byte 32 doesn't lie on char boundary,
            // so increase the index until it does
            #[expect(unstable_name_collisions)]
            let idx = local_source.ceil_char_boundary(32);

            return Err(format!(
                "parse error at {}:{}..{}:{}, starting with: {}",
                full_range.start_point.row,
                full_range.start_point.column,
                full_range.end_point.row,
                full_range.end_point.column,
                &local_source[..idx]
            ));
        }

        // if this is a leaf that spans multiple lines, create one child per line,
        // to ease matching and diffing (typically, for multi-line comments)
        if children.is_empty() && local_source.contains('\n') {
            let mut offset = range.start;
            for line in local_source.lines() {
                let trimmed = line.trim_start();
                let start_position = offset + line.len() - trimmed.len();
                let mut hasher = crate::fxhasher();
                trimmed.hash(&mut hasher);
                children.push(arena.alloc(Self {
                    hash: hasher.finish(),
                    children: Vec::new(),
                    field_to_children: FxHashMap::default(),
                    source: trimmed,
                    grammar_name: "@virtual_line@",
                    field_name: None,
                    byte_range: start_position..start_position + trimmed.len(),
                    id: *next_node_id,
                    descendant_count: 1,
                    parent: UnsafeCell::new(None),
                    // `@virtual_line@` isn't an actual grammar type, so it cannot be present in
                    // the grammar and thus can't have a commutative parent defined
                    commutative_parent: None,
                    dfs: UnsafeCell::new(None),
                    lang_profile,
                }));
                *next_node_id += 1;
                offset += line.len() + 1;
            }
        }

        let grammar_name = node.grammar_name();

        // check if this node needs flattening.
        if children_added_by_flattening > 0 && lang_profile.flattened_nodes.contains(&grammar_name)
        {
            children = Self::flatten_children(children, children_added_by_flattening, grammar_name);
        }

        // pre-compute a hash value that is invariant under isomorphism
        let mut hasher = crate::fxhasher();
        grammar_name.hash(&mut hasher);
        lang_profile.hash(&mut hasher);
        if children.is_empty() {
            local_source.hash(&mut hasher);
        } else {
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

        // pre-compute the commutative parent, either by node type or via a query.
        let commutative_parent = lang_profile
            .get_commutative_parent_by_grammar_name(grammar_name)
            .or_else(|| node_id_to_commutative_parent.get(&node.id()).copied());

        let result = arena.alloc(Self {
            hash: hasher.finish(),
            children,
            field_to_children,
            source: local_source,
            grammar_name,
            field_name,
            // parse-specific fields not included in hash/isomorphism
            byte_range: range,
            id: *next_node_id,
            descendant_count,
            parent: UnsafeCell::new(None),
            commutative_parent,
            dfs: UnsafeCell::new(None),
            lang_profile,
        });
        *next_node_id += 1;
        result.internal_set_parent_on_children();
        Ok(result)
    }

    fn internal_set_parent_on_children(&'a self) {
        for child in &self.children {
            unsafe { *child.parent.get() = Some(self) }
        }
    }

    fn internal_precompute_root_dfs(&'a self, ref_arena: &'a Arena<&'a Self>) {
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
            unsafe { *node.dfs.get() = Some(&result[start..end]) };
        }

        let mut i = 0;
        process_node(self, result, &mut i);
    }

    /// Pull in all the grandchildren whose parent is of the given grammar name as children,
    /// to flatten binary operators which are associative.
    fn flatten_children(
        children: Vec<&'a AstNode<'a>>,
        children_added_by_flattening: usize,
        grammar_name: &str,
    ) -> Vec<&'a AstNode<'a>> {
        let mut flattened_children =
            Vec::with_capacity(children.len() + children_added_by_flattening);
        for child in children {
            if child.grammar_name == grammar_name {
                flattened_children.extend(&child.children);
            } else {
                flattened_children.push(child);
            }
        }
        flattened_children
    }

    /// The height of the subtree under that node
    pub fn height(&self) -> i32 {
        self.children
            .iter()
            .copied()
            .map(Self::height)
            .max()
            .map_or(0, |x| x + 1)
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
    pub fn child(&self, index: usize) -> Option<&'a Self> {
        self.children.get(index).copied()
    }

    /// Get children by field name (children do not need to be associated to a field name,
    /// those are set in the grammar in particular rules)
    pub fn children_by_field_name(&self, field_name: &str) -> Option<&Vec<&'a Self>> {
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
    pub fn dfs(&'a self) -> impl ExactSizeIterator<Item = &'a Self> + Clone {
        // SAFETY: This is not written to after construction.
        if let Some(dfs) = unsafe { *self.dfs.get() } {
            Either::Left(dfs.iter().copied())
        } else {
            Either::Right(self.calculate_dfs())
        }
    }

    /// Helper function mainly used for testing.
    /// Circumvents the cached `dfs` field and instead computes DFS manually.
    fn calculate_dfs(&'a self) -> DfsIterator<'a> {
        DfsIterator {
            current: vec![&self],
        }
    }

    /// Postfix iterator
    pub fn postfix(&'a self) -> impl Iterator<Item = &'a Self> {
        PostfixIterator {
            queue: vec![(self, 0)],
        }
    }

    /// Ancestors iterator (which includes the node itself)
    pub fn ancestors(&'a self) -> impl Iterator<Item = &'a Self> {
        AncestorsIterator { cursor: Some(self) }
    }

    /// The root of the tree this node is part of
    pub fn root(&'a self) -> &'a Self {
        self.ancestors()
            .last()
            .expect("There must be at least one ancestor of any node: the node itself")
    }

    /// Whether this node is isomorphic to another.
    /// This doesn't take commutativity into account.
    pub fn isomorphic_to<'b>(&'a self, other: &'b AstNode<'b>) -> bool {
        let mut zipped = self.dfs().zip(other.dfs());
        self.hash == other.hash
            && zipped.all(|(n1, n2)| {
                n1.grammar_name == n2.grammar_name
                    && n1.children.len() == n2.children.len()
                    && (!n1.children.is_empty() || n1.source == n2.source)
            })
    }

    /// The commutative merging settings associated with this node.
    pub fn commutative_parent_definition(&self) -> Option<&CommutativeParent> {
        self.commutative_parent
    }

    /// The signature definition associated with this node.
    pub fn signature_definition(&self) -> Option<&SignatureDefinition> {
        self.lang_profile
            .find_signature_definition_by_grammar_name(self.grammar_name)
    }

    /// Checks whether a node is isomorphic to another,
    /// taking commutativity into account. This can be
    /// very expensive in the worst cases, so this is not
    /// meant to be used as part of the merging process, but
    /// only as a helper to evaluate merging during development.
    ///
    /// Possible improvements:
    /// - we could ignore differences in separators (to ignore
    ///   optional separators at the end of a list).
    /// - we could accept duplicate elements (for instance,
    ///   duplicate Java imports on one side but not on the other)
    #[cfg(feature = "dev")] // only used in `mgf_dev compare`
    pub fn commutatively_isomorphic_to(&'a self, other: &'a Self) -> bool {
        let mut hashes_self = vec![0; self.id + 1];
        self.precompute_commutative_hashes(&mut hashes_self);
        let mut hashes_other = vec![0; other.id + 1];
        other.precompute_commutative_hashes(&mut hashes_other);

        self._commutatively_isomorphic_to(other, &hashes_self, &hashes_other)
    }

    /// Precomputes hashes tailored to commutative isomorphism in the vector
    /// supplied, and return the hash of this node.
    ///
    /// The storage of the hashes in the vector relies on the fact that
    /// node ids are allocated consecutively: the hash for a node is stored
    /// at the index given by its id. This will panic if the vector isn't big enough.
    #[cfg(feature = "dev")]
    fn precompute_commutative_hashes(&self, hash_store: &mut Vec<u64>) -> u64 {
        let mut hasher = crate::fxhasher();
        // like for non-commutative hashes, take the grammar name (node type) and language into account
        self.grammar_name.hash(&mut hasher);
        self.lang_profile.hash(&mut hasher);

        if self.children.is_empty() {
            // for leaves, the hash is just the source of the node
            self.source.hash(&mut hasher);
        } else {
            // For internal nodes, it takes the hashes of the children
            let mut hashed_children: Vec<u64> = self
                .children
                .iter()
                .map(|child| child.precompute_commutative_hashes(hash_store))
                .collect();

            if self.commutative_parent_definition().is_some() {
                // if the node is commutative, the order of the children is disregarded
                hashed_children.sort_unstable();
            }

            hashed_children.hash(&mut hasher);
        }
        let hash = hasher.finish();
        hash_store[self.id] = hash;
        hash
    }

    #[cfg(feature = "dev")]
    fn _commutatively_isomorphic_to(
        &self,
        other: &'a Self,
        hashes_self: &[u64],
        hashes_other: &[u64],
    ) -> bool {
        use crate::multimap::MultiMap;

        if hashes_self[self.id] != hashes_other[other.id] || self.grammar_name != other.grammar_name
        {
            return false;
        }

        // two isomorphic leaves
        let isomorphic_leaves = || {
            (self.children.is_empty() && other.children.is_empty())
                && self.hash == other.hash
                && self.source == other.source
        };

        // regular nodes whose children are one-to-one isomorphic, in the same order
        let parents_with_pairwise_isomorphic_children = || {
            !self.children.is_empty()
                && self.children.len() == other.children.len()
                && zip(&self.children, &other.children)
                    .all(|(n1, n2)| n1._commutatively_isomorphic_to(n2, hashes_self, hashes_other))
        };

        // commutative nodes whose children are one-to-one isomorphic, but not in the same order
        let commutative_parents_with_unordered_isomorphic_children = || {
            if self.commutative_parent_definition().is_none()
                || self.children.len() != other.children.len()
            {
                return false;
            }
            let mut hashed_other_children: MultiMap<u64, &'a Self> = other
                .children
                .iter()
                .map(|child| (hashes_other[child.id], *child))
                .collect();
            self.children.iter().all(|child| {
                hashed_other_children
                    .remove_one(hashes_self[child.id], |other_child| {
                        child._commutatively_isomorphic_to(other_child, hashes_self, hashes_other)
                    })
                    .is_some()
            })
        };

        isomorphic_leaves()
            || parents_with_pairwise_isomorphic_children()
            || commutative_parents_with_unordered_isomorphic_children()
    }

    /// Get the parent of this node, if any
    pub fn parent(&'a self) -> Option<&'a Self> {
        unsafe { *self.parent.get() }
    }

    /// The node that comes just before this node in the list of children
    /// of its parent (if any).
    pub fn predecessor(&'a self) -> Option<&'a Self> {
        self.parent()?
            .children
            .iter()
            .rev()
            .skip_while(|sibling| sibling.id != self.id)
            .skip(1)
            .copied()
            .next()
    }

    /// The node that comes just after this node in the list of children
    /// of its parent (if any).
    pub fn successor(&'a self) -> Option<&'a Self> {
        self.parent()?
            .children
            .iter()
            .skip_while(|sibling| sibling.id != self.id)
            .skip(1)
            .copied()
            .next()
    }

    /// Truncate a tree so that all nodes selected by the predicate are treated as leaves
    pub fn truncate<'b, F>(&'a self, predicate: F, arena: &'b Arena<AstNode<'b>>) -> &'b AstNode<'b>
    where
        F: Fn(&'a Self) -> bool,
        'a: 'b,
    {
        fn _truncate<'a, 'b, F>(
            node: &'a AstNode<'a>,
            predicate: &F,
            arena: &'b Arena<AstNode<'b>>,
        ) -> &'b AstNode<'b>
        where
            F: Fn(&'a AstNode<'a>) -> bool,
            'a: 'b,
        {
            let truncate = predicate(node);
            let children = if truncate {
                Vec::new()
            } else {
                node.children
                    .iter()
                    .map(|child| _truncate(child, predicate, arena))
                    .collect()
            };
            let field_to_children = if truncate {
                FxHashMap::default()
            } else {
                let child_id_map: FxHashMap<usize, &'b AstNode<'b>> =
                    children.iter().map(|child| (child.id, *child)).collect();
                node.field_to_children
                    .iter()
                    .map(|(k, v)| (*k, v.iter().map(|child| child_id_map[&child.id]).collect()))
                    .collect()
            };
            let result = arena.alloc(AstNode {
                children,
                field_to_children,
                byte_range: node.byte_range.clone(),
                parent: UnsafeCell::new(None),
                dfs: UnsafeCell::new(None),
                ..*node
            });
            result.internal_set_parent_on_children();
            result
        }
        _truncate(self, &predicate, arena)
    }

    /// Any part of the source between the start of this node and
    /// the start of its first child (if any). There generally isn't any,
    /// but it can be present as leading whitespace at the root of a document
    /// for instance.
    pub fn leading_source(&'a self) -> Option<&'a str> {
        let first_child = self.children.first()?;
        let offset = first_child.byte_range.start - self.byte_range.start;
        if offset > 0 {
            Some(&self.source[..offset])
        } else {
            None
        }
    }

    /// Any whitespace that precedes this node.
    /// This will be None if the node doesn't have a predecessor,
    /// otherwise it's the whitespace between its predecessor and itself.
    pub fn preceding_whitespace(&'a self) -> Option<&'a str> {
        let parent = self.parent()?;
        let predecessor = self.predecessor()?;
        let start = predecessor.byte_range.end - parent.byte_range.start;
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
            .find_map(Self::preceding_indentation)
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
        match (self.preceding_indentation()).or_else(|| self.ancestor_indentation()) {
            Some(indentation) => {
                // TODO FIXME this is invalid for multiline string literals!
                Cow::from(self.source.replace(&format!("\n{indentation}"), "\n"))
            }
            None => Cow::from(self.source),
        }
    }

    /// The source of this node, stripped from any indentation inherited by the node or its ancestors
    /// and shifted back to the desired indentation.
    pub fn reindented_source(&'a self, new_indentation: &str) -> Cow<'a, str> {
        let indentation = (self.preceding_indentation())
            .or_else(|| self.ancestor_indentation())
            .unwrap_or("");
        if indentation == new_indentation {
            return Cow::from(self.source);
        }
        let newlines = format!("\n{indentation}");

        let new_newlines = format!("\n{new_indentation}");
        Cow::from(self.source.replace(&newlines, &new_newlines)) // TODO FIXME this is invalid for multiline string literals!
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

    /// Represents the node and its sub-structure in ASCII art, optionally printing only the nodes
    /// up to a given depth
    pub fn ascii_tree(&'a self, max_depth: Option<usize>) -> String {
        self.internal_ascii_tree(
            0,
            max_depth,
            &Color::DarkGray.prefix().to_string(),
            true,
            None,
        )
    }

    fn internal_ascii_tree(
        &'a self,
        depth: usize,
        max_depth: Option<usize>,
        prefix: &str,
        last_child: bool,
        parent: Option<&CommutativeParent>,
    ) -> String {
        if max_depth == Some(depth) {
            return String::new();
        }

        let num_children = self.children.len();
        let next_parent = self.commutative_parent_definition();

        let tree_sym = if last_child { "└" } else { "├" };

        let escape_whitespace = |string: &str| string.replace('\n', "\\n").replace('\t', "\\t");

        let key = if let Some(key) = self.field_name {
            format!("{key}: ")
        } else {
            String::new()
        };

        let escaped_grammar_name = escape_whitespace(self.grammar_name);
        let grammar_name = if self.source != self.grammar_name {
            escaped_grammar_name
        } else {
            Color::Red.paint(escaped_grammar_name).to_string()
        };

        let source = if num_children == 0 && self.source != self.grammar_name {
            format!(" {}", Color::Red.paint(escape_whitespace(self.source)))
        } else {
            String::new()
        };

        let commutative = if next_parent.is_some() {
            Color::LightPurple.paint(" Commutative").to_string()
        } else {
            String::new()
        };

        let sig = if parent.is_some()
            && let Some(sig) = self.signature()
        {
            format!(" {}", Color::LightCyan.paint(sig.to_string()))
        } else {
            String::new()
        };

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
                        depth + 1,
                        max_depth,
                        &new_prefix,
                        index == num_children - 1,
                        next_parent,
                    )
                }),
        )
        .collect()
    }

    /// Checks if a tree has any signature conflicts in it
    pub(crate) fn has_signature_conflicts(&self) -> bool {
        let conflict_in_children = || {
            self.children
                .iter()
                .any(|child| child.has_signature_conflicts())
        };

        let conflict_in_self = || {
            self.children.len() >= 2
                && self.commutative_parent_definition().is_some()
                && !self
                    .children
                    .iter()
                    .copied()
                    .filter_map(AstNode::signature)
                    .all_unique()
        };

        conflict_in_self() || conflict_in_children()
    }

    /// Extracts a signature for this node if we have a signature definition
    /// for this type of nodes in the language profile.
    pub(crate) fn signature(&'a self) -> Option<Signature<'a, 'a>> {
        let definition = self.signature_definition()?;
        Some(definition.extract_signature_from_original_node(self))
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
            && self.lang_profile == other.lang_profile
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

#[cfg(test)] // should avoid panicking code elsewhere
impl<'a, T> Index<T> for AstNode<'a>
where
    T: SliceIndex<[&'a Self]>,
{
    type Output = <[&'a Self] as Index<T>>::Output;

    fn index(&self, index: T) -> &Self::Output {
        &self.children[index]
    }
}

#[derive(Clone)]
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

    fn size_hint(&self) -> (usize, Option<usize>) {
        let size = self.current.iter().copied().map(AstNode::size).sum();
        (size, Some(size))
    }
}

impl ExactSizeIterator for DfsIterator<'_> {}

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

    use crate::test_utils::ctx;

    use super::*;

    #[test]
    fn parse_error() {
        let ctx = ctx();
        let lang_profile = LangProfile::detect_from_filename("test.json")
            .expect("could not load language profile");

        let parse = AstNode::parse("[\n {,\n]", lang_profile, &ctx.arena, &ctx.ref_arena);

        assert_eq!(
            parse,
            Err("parse error at 1:1..1:3, starting with: {,".to_string())
        );

        let parse = AstNode::parse(
            "属于个人的非赢利性开源项目",
            lang_profile,
            &ctx.arena,
            &ctx.ref_arena,
        );

        assert_eq!(
            parse,
            Err("parse error at 0:0..0:39, starting with: 属于个人的非赢利性开源".to_string())
        );
    }

    #[test]
    fn heights() {
        let ctx = ctx();

        assert_eq!(ctx.parse("a.json", "null").height(), 1);
        assert_eq!(ctx.parse("a.json", "[1]").height(), 2);
        assert_eq!(ctx.parse("a.json", "{\"foo\": 3}").height(), 4);
    }

    #[test]
    fn sizes() {
        let ctx = ctx();

        assert_eq!(ctx.parse("a.json", "null").size(), 2);
        assert_eq!(ctx.parse("a.json", "[1]").size(), 5);
        assert_eq!(ctx.parse("a.json", "{\"foo\": 3}").size(), 11);
    }

    #[test]
    fn children_by_field_names() {
        let ctx = ctx();

        let root = ctx.parse("a.json", "{\"foo\": 3}");
        let object = root[0];
        let pair = object[1];
        assert_eq!(root.children_by_field_name("non_existent"), None);
        assert_eq!(
            pair.children_by_field_name("key").unwrap()[0].source,
            "\"foo\""
        );
    }

    #[test]
    fn children_by_field_names_with_modifiers() {
        let ctx = ctx();

        let root = ctx.parse("a.java", "public class MyCls {}");
        let class_declaration = root[0];
        assert_eq!(
            class_declaration.children_by_field_name("name"),
            Some(&vec![class_declaration[2]])
        );
    }

    #[test]
    fn atomic_nodes() {
        let ctx = ctx();

        let root = ctx.parse("a.java", "import java.io.InputStream;");
        let import_statement = root[0];
        assert_eq!(import_statement.children.len(), 0);
    }

    #[test]
    fn trailing_newlines_are_stripped_from_nodes() {
        let ctx = ctx();
        let tree = ctx.parse("a.rs", "  /// test\n  fn foo() {\n    ()\n  }\n");
        let comment = tree[0][0][0];
        assert_eq!(comment.grammar_name, "line_outer_doc_comment");
        // tree-sitter-rust includes a newline at the end of the source for this node,
        // but we strip it when converting the tree to our own data structure (`AstNode`)
        assert_eq!(comment.source, "/// test");
    }

    #[test]
    fn hashing_does_not_depend_on_whitespace_but_on_content() {
        let ctx = ctx();

        let hash_1 = &ctx.parse("a.rs", "fn x() -> i32 { 7 - 1 }").hash;
        let hash_2 = &ctx.parse("a.rs", "fn x() -> i32 {\n 7-1 }").hash;
        let hash_3 = &ctx.parse("a.rs", "fn x() -> i32 {\n 9-2 }").hash;

        assert_eq!(hash_1, hash_2); // whitespace and indentation differences are insignificant
        assert_ne!(hash_2, hash_3);

        let hash_4 = &ctx.parse("a.rs", "fn x() { \"some string\" }").hash;
        let hash_5 = &ctx.parse("a.rs", "fn x() { \" some string\" }").hash;
        let hash_6 = &ctx.parse("a.rs", "fn x() {   \"some string\" }").hash;
        assert_ne!(hash_4, hash_5); // whitespace inside of a string is significant
        assert_eq!(hash_4, hash_6);
    }

    #[test]
    fn isomorphism_is_not_just_hashing() {
        let ctx = ctx();

        let node_1 = ctx.parse("a.rs", "fn x() -> i32 { 7 - 1 }");
        let node_2 = ctx.parse("a.rs", "fn x() -> i32 { 8 - 1 }");
        let fake_hash_collision = AstNode {
            hash: node_1.hash,
            parent: UnsafeCell::new(None),
            commutative_parent: node_2.commutative_parent,
            dfs: UnsafeCell::new(None),
            children: node_2.children.to_owned(),
            field_to_children: FxHashMap::default(),
            byte_range: node_2.byte_range.to_owned(),
            ..*node_2
        };

        assert_eq!(node_1.hash, fake_hash_collision.hash);
        assert!(!node_1.isomorphic_to(&fake_hash_collision));
    }

    #[test]
    fn isomorphism_of_empty_roots() {
        let ctx = ctx();
        let tree_1 = ctx.parse("a.rs", "    ");
        let tree_2 = ctx.parse("a.rs", "  ");
        assert!(tree_1.isomorphic_to(tree_2));
        assert!(tree_2.isomorphic_to(tree_1));
    }

    #[test]
    fn isomorphism_for_different_languages() {
        let ctx = ctx();

        let tree_python = ctx.parse("a.py", "foo()");
        let tree_java = ctx.parse("a.java", "foo();");
        let arguments_python = tree_python[0][0][1];
        let arguments_java = tree_java[0][0][1];

        // those nodes would satisfy all other conditions to be isomorphic…
        assert_eq!(arguments_python.grammar_name, "argument_list");
        assert_eq!(arguments_java.grammar_name, "argument_list");
        assert_eq!(arguments_python.children.len(), 2);
        assert_eq!(arguments_java.children.len(), 2);
        assert_eq!(arguments_python.source, "()");
        assert_eq!(arguments_java.source, "()");

        // but they aren't, because the languages differ
        assert!(!arguments_java.isomorphic_to(arguments_python));
    }

    #[test]
    fn parents_are_accessible() {
        let ctx = ctx();
        let tree = ctx.parse("a.json", "{\"foo\": 3}");
        let root = tree;
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
        let tree = ctx.parse("a.json", "{\"foo\": 3}");

        let node_types = tree.dfs().map(|n| n.grammar_name).collect_vec();

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
    fn dfs_exact_size_iterator() {
        let ctx = ctx();
        let tree = ctx.parse("a.json", "{\"foo\": 3}");

        // using the cached version
        let mut nodes = tree.dfs();

        for len in (0..=11).rev() {
            assert_eq!(nodes.len(), len);
            nodes.next();
        }

        // using the manually calculated version
        let mut nodes = tree.calculate_dfs();

        for len in (0..=11).rev() {
            assert_eq!(nodes.len(), len);
            nodes.next();
        }
    }

    #[test]
    fn postfix_traversal() {
        let ctx = ctx();
        let tree = ctx.parse("a.json", "{\"foo\": 3}");

        let node_types = tree.postfix().map(|n| n.grammar_name).collect_vec();

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
        let tree = ctx.parse("a.json", "{\"foo\": 3, \"bar\": 4}");

        let arena = Arena::new();
        let truncated = tree.truncate(|node| node.grammar_name == "pair", &arena);

        let node_types = truncated.postfix().map(|n| n.grammar_name).collect_vec();

        let truncated_object = truncated.root()[0];
        let original_object = tree[0];
        let truncated_first_pair = truncated_object[1];
        let original_first_pair = original_object[1];

        assert_eq!(truncated_object.id, original_object.id);
        assert_eq!(truncated_first_pair.id, original_first_pair.id);
        assert_eq!(truncated.size(), tree.size());
        assert_eq!(
            node_types,
            vec!["{", "pair", ",", "pair", "}", "object", "document"]
        );
    }

    #[test]
    fn leading_source() {
        let ctx = ctx();
        let tree = ctx.parse("a.rs", "\n let x = 1;\n");
        assert_eq!(tree.byte_range.start, 0);
        assert_eq!(tree.leading_source(), Some("\n "));
    }

    #[test]
    fn preceding_whitespace() {
        let ctx = ctx();
        let tree = ctx.parse("a.json", "[1, 2,\n 3]");

        let root = tree[0];
        let [bracket, one, comma, two, _, three] = root[0..=5] else {
            unreachable!()
        };

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
        let tree = ctx.parse("a.go", "import (\n    \"fmt\"\n    \"core\"\n)\n");
        let root = tree[0];
        let import_list = root[1];
        let core = import_list[2];
        assert_eq!(core.source, "\"core\"");
        assert_eq!(core.preceding_whitespace(), Some("\n    "));
        assert_eq!(core.ancestor_indentation(), None);
    }

    #[test]
    fn trailing_whitespace_toml() {
        let ctx = ctx();
        let tree = ctx.parse("a.toml", "[foo]\na = 1\n\n[bar]\nb = 2");
        let first_table = tree[0];
        let second_table = tree[1];
        assert_eq!(first_table.source, "[foo]\na = 1");
        assert_eq!(first_table.trailing_whitespace(), None);
        assert_eq!(second_table.source, "[bar]\nb = 2");
        assert_eq!(second_table.trailing_whitespace(), None);
    }

    #[test]
    fn preceding_indentation_shift() {
        let ctx = ctx();
        let tree = ctx.parse("a.java", "\nclass MyCls {\n    int attr;\n}");
        let class_decl = tree[0];
        let class_body = class_decl[2];
        let attr = class_body[1];

        assert_eq!(attr.indentation_shift(), Some("    "));
    }

    #[test]
    fn preceding_indentation_shift_tabs() {
        let ctx = ctx();
        let source = "class Outer {\n\tclass MyCls {\n\t\tint attr;\n\t}\n}\n";
        let tree = ctx.parse("a.java", source);
        let class_decl = tree[0][2][1];
        let class_body = class_decl[2];
        let attr = class_body[1];

        assert_eq!(attr.indentation_shift(), Some("\t"));
    }

    #[test]
    fn preceding_indentation_shift_mixed_spaces_and_tabs() {
        let ctx = ctx();
        let source = "class Outer {\n\tclass MyCls {\n        int attr;\n\t}\n}\n";
        let tree = ctx.parse("a.java", source);
        let class_decl = tree[0][2][1];
        let class_body = class_decl[2];
        let attr = class_body[1];

        assert_eq!(attr.indentation_shift(), Some("    "));
    }

    #[test]
    fn preceding_indentation_shift_mixed_tabs_and_spaces() {
        let ctx = ctx();
        let source = "class Outer {\n    class MyCls {\n\t\tint attr;\n    }\n}\n";
        let tree = ctx.parse("a.java", source);
        let class_decl = tree[0][2][1];
        let class_body = class_decl[2];
        let attr = class_body[1];

        assert_eq!(attr.indentation_shift(), Some("\t"));
    }

    #[test]
    fn reindent_yaml() {
        let ctx = ctx();
        let tree = ctx.parse("a.yaml", "hello:\n  foo: 2\nbar: 4\n");
        let block_node = tree[0][0];
        assert_eq!(block_node.grammar_name, "block_node");
        let value = block_node[0][0][2];
        assert_eq!(value.grammar_name, "block_node");

        assert_eq!(block_node.indentation_shift(), None);
        assert_eq!(value.indentation_shift(), Some("  "));
    }

    #[test]
    fn source_with_whitespace() {
        let ctx = ctx();
        let tree = ctx.parse("a.json", " [ 1 , 2,\n 3]");

        let root = tree[0];
        let [bracket, one, comma, two, comma_2] = root[0..=4] else {
            unreachable!()
        };

        assert_eq!(bracket.source_with_surrounding_whitespace(), "[ ");
        assert_eq!(one.source_with_surrounding_whitespace(), " 1 ");
        assert_eq!(comma.source_with_surrounding_whitespace(), " , ");
        assert_eq!(two.source_with_surrounding_whitespace(), " 2");
        assert_eq!(comma_2.source_with_surrounding_whitespace(), ",\n ");
    }

    #[test]
    fn removing_indentation() {
        let ctx = ctx();

        let source = r#"
{
    "a": [
        1,
        2,
    ],
    "b": {
        "c": "foo"
    }
}
"#;
        let tree = ctx.parse("a.json", source);

        let root = &tree[0];
        let entry_a = &root[1];
        let array = &entry_a[2];

        assert_eq!(
            entry_a.source,
            "\
\"a\": [
        1,
        2,
    ]"
        );
        assert_eq!(entry_a.indentation_shift(), Some("    "));
        assert_eq!(entry_a.ancestor_indentation(), None);
        assert_eq!(
            entry_a.unindented_source(),
            "\
\"a\": [
    1,
    2,
]"
        );
        assert_eq!(
            entry_a.reindented_source("  "),
            "\
\"a\": [
      1,
      2,
  ]"
        );

        assert_eq!(
            array.source,
            "\
[
        1,
        2,
    ]"
        );
        assert_eq!(array.indentation_shift(), None);
        assert_eq!(array.ancestor_indentation(), Some("    "));
        assert_eq!(
            array.unindented_source(),
            "\
[
    1,
    2,
]"
        );
        assert_eq!(
            array.reindented_source("  "),
            "\
[
      1,
      2,
  ]"
        );
    }

    #[test]
    fn multiline_comments_are_isomorphic() {
        let ctx = ctx();

        let source_1 = "/**\n * This is a comment\n * spanning on many lines\n*/";
        let comment_1 = ctx.parse("a.java", source_1)[0];
        let source_2 = "  /**\n   * This is a comment\n   * spanning on many lines\n  */";
        let comment_2 = ctx.parse("a.java", source_2)[0];

        assert!(comment_1.isomorphic_to(comment_2));
        assert_eq!(comment_1.children.len(), 4);
        assert_eq!(comment_2[0].source, "/**");
        assert_eq!(comment_2[1].source, "* This is a comment");
        assert_eq!(comment_2[2].source, "* spanning on many lines");
        assert_eq!(comment_2[3].source, "*/");
        assert_eq!(comment_2[0].byte_range, 2..5);
        assert_eq!(comment_2[1].byte_range, 9..28);
        assert_eq!(comment_2[2].byte_range, 32..56);
        assert_eq!(comment_2[3].byte_range, 59..61);
        assert_eq!(comment_2[1].preceding_whitespace(), Some("\n   "));
    }

    #[test]
    fn print_as_ascii_art() {
        let ctx = ctx();
        let tree = ctx.parse("a.json", "{\"foo\": 3, \"bar\": 4}");

        let expected = "\
\u{1b}[90m└\u{1b}[0mdocument
\u{1b}[90m  └\u{1b}[0mobject\u{1b}[95m Commutative\u{1b}[0m
\u{1b}[90m    ├\u{1b}[0m\u{1b}[31m{\u{1b}[0m
\u{1b}[90m    ├\u{1b}[0mpair \u{1b}[96mSignature [[\"foo\"]]\u{1b}[0m
\u{1b}[90m    │ ├key: \u{1b}[0mstring
\u{1b}[90m    │ │ ├\u{1b}[0m\u{1b}[31m\"\u{1b}[0m
\u{1b}[90m    │ │ ├\u{1b}[0mstring_content \u{1b}[31mfoo\u{1b}[0m
\u{1b}[90m    │ │ └\u{1b}[0m\u{1b}[31m\"\u{1b}[0m
\u{1b}[90m    │ ├\u{1b}[0m\u{1b}[31m:\u{1b}[0m
\u{1b}[90m    │ └value: \u{1b}[0mnumber \u{1b}[31m3\u{1b}[0m
\u{1b}[90m    ├\u{1b}[0m\u{1b}[31m,\u{1b}[0m
\u{1b}[90m    ├\u{1b}[0mpair \u{1b}[96mSignature [[\"bar\"]]\u{1b}[0m
\u{1b}[90m    │ ├key: \u{1b}[0mstring
\u{1b}[90m    │ │ ├\u{1b}[0m\u{1b}[31m\"\u{1b}[0m
\u{1b}[90m    │ │ ├\u{1b}[0mstring_content \u{1b}[31mbar\u{1b}[0m
\u{1b}[90m    │ │ └\u{1b}[0m\u{1b}[31m\"\u{1b}[0m
\u{1b}[90m    │ ├\u{1b}[0m\u{1b}[31m:\u{1b}[0m
\u{1b}[90m    │ └value: \u{1b}[0mnumber \u{1b}[31m4\u{1b}[0m
\u{1b}[90m    └\u{1b}[0m\u{1b}[31m}\u{1b}[0m
";

        assert_eq!(tree.ascii_tree(None), expected);
        assert_eq!(tree.ascii_tree(Some(5)), expected);

        let expected = "\
\u{1b}[90m└\u{1b}[0mdocument
\u{1b}[90m  └\u{1b}[0mobject\u{1b}[95m Commutative\u{1b}[0m
\u{1b}[90m    ├\u{1b}[0m\u{1b}[31m{\u{1b}[0m
\u{1b}[90m    ├\u{1b}[0mpair \u{1b}[96mSignature [[\"foo\"]]\u{1b}[0m
\u{1b}[90m    │ ├key: \u{1b}[0mstring
\u{1b}[90m    │ ├\u{1b}[0m\u{1b}[31m:\u{1b}[0m
\u{1b}[90m    │ └value: \u{1b}[0mnumber \u{1b}[31m3\u{1b}[0m
\u{1b}[90m    ├\u{1b}[0m\u{1b}[31m,\u{1b}[0m
\u{1b}[90m    ├\u{1b}[0mpair \u{1b}[96mSignature [[\"bar\"]]\u{1b}[0m
\u{1b}[90m    │ ├key: \u{1b}[0mstring
\u{1b}[90m    │ ├\u{1b}[0m\u{1b}[31m:\u{1b}[0m
\u{1b}[90m    │ └value: \u{1b}[0mnumber \u{1b}[31m4\u{1b}[0m
\u{1b}[90m    └\u{1b}[0m\u{1b}[31m}\u{1b}[0m
";

        assert_eq!(tree.ascii_tree(Some(4)), expected);

        let expected = "\
\u{1b}[90m└\u{1b}[0mdocument
";

        assert_eq!(tree.ascii_tree(Some(1)), expected);

        assert_eq!(tree.ascii_tree(Some(0)), "");
    }

    #[test]
    #[cfg(feature = "dev")]
    fn commutative_isomorphism() {
        let ctx = ctx();
        let obj_1 = ctx.parse("a.json", "{\"foo\": 3, \"bar\": 4}");
        let obj_2 = ctx.parse("a.json", "{\"bar\": 4, \"foo\": 3}");
        let obj_3 = ctx.parse("a.json", "{\"bar\": 3, \"foo\": 4}");
        let obj_4 = ctx.parse("a.json", "{\n  \"foo\": 3,\n  \"bar\": 4\n}");
        let array_1 = ctx.parse("a.json", "[ 1, 2 ]");
        let array_2 = ctx.parse("a.json", "[ 2, 1 ]");

        assert!(obj_1.commutatively_isomorphic_to(obj_2));
        assert!(!obj_1.commutatively_isomorphic_to(obj_3));
        assert!(!obj_2.commutatively_isomorphic_to(obj_3));
        assert!(obj_1.commutatively_isomorphic_to(obj_4));
        assert!(!obj_1.commutatively_isomorphic_to(array_1));
        assert!(!array_1.commutatively_isomorphic_to(array_2));

        let method1 = ctx.parse("a.java", "public final void main();");
        let method2 = ctx.parse("a.java", "public final static void main();");

        // `public`, `final` and `static` are all commutative children of (function) `modifiers`,
        // but the second tree doesn't have `static`. A naive `zip` would only check the first two
        // children, see that they're equal, and incorrectly decide that the parents are equal as well
        assert!(!method1.commutatively_isomorphic_to(method2));
    }

    #[test]
    #[cfg(feature = "dev")]
    fn commutative_isomorphism_with_hash_collisions() {
        let ctx = ctx();
        let obj_1 = ctx.parse("a.json", "{\"foo\": 3, \"bar\": 4}");
        let obj_2 = ctx.parse("a.json", "{\"foo\": 3, \"foo\": 3}");

        // pretend that all elememts have the same hashes,
        // for the sake of simulating many hash collisions.
        let hashes_1 = vec![444; obj_1.id + 1];
        let hashes_2 = vec![444; obj_2.id + 1];

        // since the hashes are only used for optimization purposes,
        // we should still be able to detect that the objects are not isomorphic
        assert!(!obj_1._commutatively_isomorphic_to(obj_2, &hashes_1, &hashes_2));
        assert!(!obj_2._commutatively_isomorphic_to(obj_1, &hashes_2, &hashes_1));
    }

    #[test]
    /// A stricter version of [`node_ids_all_unique`]
    fn node_ids() {
        let ctx = ctx();

        let src = r#"let foo = "line 1
line 2
line 3";"#;
        let tree = ctx.parse("a.rs", src);

        let root = tree;
        assert_eq!(root.id, 13);

        let let_declaration = root[0];
        assert_eq!(let_declaration.id, 12);

        let [let_kw, foo, eq, str_literal, semicolon] = let_declaration[..] else {
            unreachable!()
        };
        assert_eq!(let_kw.id, 1);
        assert_eq!(foo.id, 2);
        assert_eq!(eq.id, 3);
        assert_eq!(str_literal.id, 10);
        assert_eq!(semicolon.id, 11);

        let [quote_open, lines, quote_close] = str_literal[..] else {
            unreachable!()
        };
        assert_eq!(quote_open.id, 4);
        assert_eq!(lines.id, 8);
        assert_eq!(quote_close.id, 9);

        let [line1, line2, line3] = lines[..] else {
            unreachable!()
        };
        assert_eq!(line1.source, "line 1");
        assert_eq!(line1.id, 5);
        assert_eq!(line2.source, "line 2");
        assert_eq!(line2.id, 6);
        assert_eq!(line3.source, "line 3");
        assert_eq!(line3.id, 7);
    }

    #[test]
    fn node_ids_all_unique() {
        let ctx = ctx();

        let src = r#"let foo = "line 1
line 2
line 3";"#;
        let tree = ctx.parse("a.rs", src);

        let ids = tree.dfs().map(|n| n.id).collect_vec();

        assert!(ids.iter().all_unique());

        // all the available ids (0-len) are used, i.e. none are skipped
        // not strictly necessary, but nice to have
        assert_eq!(*ids.iter().max().unwrap(), ids.len());
    }

    #[test]
    fn parse_html_with_js() {
        let ctx = ctx();
        let source = "<html><head><script>console.log('hi');</script></head></html>";
        let html = ctx.parse("a.html", source);

        assert_eq!(html.grammar_name, "document");
        assert_eq!(html.lang_profile.name, "HTML");
        let script_element = html[0][1][1];
        assert_eq!(script_element.grammar_name, "script_element");
        assert_eq!(script_element[1].grammar_name, "raw_text");
        assert_eq!(script_element[1].lang_profile.name, "HTML");
        assert_eq!(script_element[1][0].grammar_name, "program");
        assert_eq!(script_element[1][0].lang_profile.name, "Javascript");
        assert_eq!(script_element[1][0][0].grammar_name, "expression_statement");
        assert_eq!(script_element[1][0][0].lang_profile.name, "Javascript");
    }

    #[test]
    fn parse_injection_with_syntax_error() {
        let ctx = ctx();
        let source = "<html><head><script>invalid(][)</script></head></html>";
        let html = ctx.parse("a.html", source);

        assert_eq!(html.grammar_name, "document");
        assert_eq!(html.lang_profile.name, "HTML");
        let script_element = html[0][1][1];
        assert_eq!(script_element.grammar_name, "script_element");
        assert_eq!(script_element[1].grammar_name, "raw_text");
        assert_eq!(script_element[1].lang_profile.name, "HTML");
        assert_eq!(script_element[1].children.len(), 0);
    }

    #[test]
    fn parse_empty_child_out_of_trimmed_parent() {
        let ctx = ctx();
        // Parsing this source with the tree-sitter-md 0.3.2 grammar
        // gives an empty "block_continuation" node as a child of the
        // "paragraph" node, after the trailing newline in it.
        // Because we trim the newline at the end of the paragraph node,
        // the child ends up falling outside of the new computed range,
        // which violates the assumption that the ranges of all children
        // fall into the range of their parent.
        let source = "\
A list:
- Hello

";
        let markdown = ctx.parse("a.md", source);

        let paragraph = markdown[0][1][0][1];
        assert_eq!(paragraph.grammar_name, "paragraph");
        assert_eq!(paragraph.children.len(), 2);
        assert_eq!(paragraph[0].grammar_name, "paragraph_repeat1");
        assert_eq!(paragraph[1].grammar_name, "block_continuation");
        assert_eq!(paragraph[1].preceding_whitespace(), Some("\n"));
    }

    /// issue: https://codeberg.org/mergiraf/mergiraf/issues/532
    #[test]
    fn empty_injection() {
        let ctx = ctx();
        let source = "<html><script></script></html>";

        let root = ctx.parse("a.html", source);

        let raw_text = root[0][1][1];
        assert_eq!(raw_text.grammar_name, "raw_text");
        assert_eq!(raw_text.byte_range, 14..14);

        let program = raw_text[0];
        assert_eq!(program.grammar_name, "program");
        assert_eq!(program.byte_range, 14..14);

        assert_eq!(raw_text.trailing_whitespace(), None);

        let source_2 = "<html><script>   </script></html>";

        let root_2 = ctx.parse("a.html", source_2);

        let raw_text_2 = root_2[0][1][1];
        assert_eq!(raw_text_2.grammar_name, "raw_text");
        assert_eq!(raw_text_2.byte_range, 14..17);

        let program_2 = raw_text_2[0];
        assert_eq!(program_2.grammar_name, "program");
        // the source of this node is shrunk to an empty string to ensure
        // isomorphism regardless of the amount of whitespace
        assert_eq!(program_2.byte_range, 14..14);

        assert_eq!(raw_text_2.trailing_whitespace(), Some("   "));

        assert!(root.isomorphic_to(root_2));
    }

    #[test]
    fn commutative_parent_via_query() {
        let ctx = ctx();
        let source = "\
__all__ = [ 'foo', 'bar' ]
other = [ 1, 2 ]
";
        let python = ctx.parse("a.py", source);

        let first_list = python[0][0][2];
        let second_list = python[1][0][2];
        assert_eq!(first_list.grammar_name, "list");
        assert_eq!(second_list.grammar_name, "list");

        // the __all__ assignment is captured by the query defining the commutative parent
        assert!(first_list.commutative_parent_definition().is_some());
        // the other list isn't captured, so it's not associated to any commutative parent
        assert!(second_list.commutative_parent_definition().is_none());
    }

    #[test]
    fn flatten_binary_operators() {
        let ctx = ctx();
        let source = "\
interface MyInterface {
  level: 'debug' | 'info' | 'warn' | 'error';
}";
        let ts = ctx.parse("a.ts", source);
        let union_type = ts[0][2][1][1][1];
        assert_eq!(union_type.grammar_name, "union_type");
        assert_eq!(union_type.children.len(), 7);
        assert_eq!(union_type.children[0].grammar_name, "literal_type");
        assert_eq!(union_type.children[1].grammar_name, "|");
        assert_eq!(union_type.children[2].grammar_name, "literal_type");
        assert_eq!(union_type.children[3].grammar_name, "|");
    }

    #[test]
    fn dont_flatten_different_operators_together() {
        let ctx = ctx();
        let source = "\
interface Foo {
  field: 'first' | 'second' & 'third',
}";
        let ts = ctx.parse("a.ts", source);
        let union_type = ts[0][2][1][1][1];
        assert_eq!(union_type.grammar_name, "union_type");
        assert_eq!(union_type.children.len(), 3);
        assert_eq!(union_type.children[0].grammar_name, "literal_type");
        assert_eq!(union_type.children[1].grammar_name, "|");
        assert_eq!(union_type.children[2].grammar_name, "intersection_type");
    }
}
