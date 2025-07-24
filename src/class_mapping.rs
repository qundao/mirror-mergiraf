use std::{fmt::Display, hash::Hash, iter, ops::Deref};

use itertools::Itertools;
use rustc_hash::FxHashMap;

use crate::{
    ast::AstNode,
    lang_profile::{CommutativeParent, LangProfile},
    matching::Matching,
    pcs::Revision,
    signature::SignatureDefinition,
};

/// A node together with a marker of which revision it came from.
#[derive(Debug, Copy, Clone, Eq)]
pub struct RevNode<'a> {
    pub rev: Revision,
    pub node: &'a AstNode<'a>,
}

/// A node at a revision, which happens to be the leader of its class
/// in a class-mapping.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct Leader<'a>(RevNode<'a>);

impl PartialEq for RevNode<'_> {
    fn eq(&self, other: &Self) -> bool {
        // because we know the nodes are from the same revision, it's safe to compare them just by their ids
        self.rev == other.rev && self.node.id == other.node.id
    }
}

impl<'a> RevNode<'a> {
    pub fn new(rev: Revision, node: &'a AstNode<'a>) -> Self {
        Self { rev, node }
    }

    /// Whether the subtree rooted at this node contains another node (up to class mapping).
    pub fn contains(&self, other: &Leader<'a>, class_mapping: &ClassMapping<'a>) -> bool {
        self.node.dfs().any(|descendant| {
            class_mapping.map_to_leader(RevNode::new(self.rev, descendant)) == *other
        })
    }
}

impl<'a> Leader<'a> {
    /// Returns the leader as one of the class representatives.
    /// Uses of this method are generally suspicious, because this is an arbitrary choice
    /// of class representative. It is preferable to choose the representative based on
    /// the revision it belongs to.
    pub fn as_representative(&self) -> RevNode<'a> {
        self.0
    }

    /// The type of this node, which is guaranteed to be the same for all representatives
    /// of this leader.
    pub fn grammar_name(&self) -> &'static str {
        self.0.node.grammar_name
    }

    /// The language from which this was parsed, guaranteed to be invariant across representatives
    pub fn lang_profile(&self) -> &'a LangProfile {
        self.0.node.lang_profile
    }

    /// The commutative parent definition associated with this node
    pub fn commutative_parent_definition(&self) -> Option<&CommutativeParent> {
        self.0.node.commutative_parent_definition()
    }

    /// The signature definition for nodes of this type, which is guaranteed to be the same for all representatives
    pub fn signature_definition(&self) -> Option<&SignatureDefinition> {
        self.0.node.signature_definition()
    }
}

impl Display for RevNode<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}…{}@{}",
            self.node.grammar_name, self.node.byte_range.start, self.node.byte_range.end, self.rev
        )
    }
}

impl Display for Leader<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Hash for RevNode<'_> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.rev.hash(state);
        self.node.id.hash(state);
    }
}

/// Creates classes of nodes across multiple revisions so that
/// they can be equated when converting the corresponding trees
/// to PCS, following the 3DM-Merge algorithm from Spork
#[derive(Debug, Default)]
pub struct ClassMapping<'a> {
    map: FxHashMap<RevNode<'a>, Leader<'a>>,
    representatives: FxHashMap<Leader<'a>, FxHashMap<Revision, RevNode<'a>>>,
    exact_matchings: FxHashMap<Leader<'a>, i8>,
    empty_repr: FxHashMap<Revision, RevNode<'a>>, // stays empty (only there for ownership purposes)
}

impl<'a> ClassMapping<'a> {
    /// Creates an empty class mapping.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a matching to the mapping. The `from_rev` indicates the revision that's on the left hand side of the mapping.
    /// The `to_rev` indicates the revision that's on the right hand side of the matching.
    /// If we are matching from left to right, then we disregard matchings which are inconsistent with the base matchings added so far.
    /// The `is_exact` parameters indicates if two nodes being matched indicates that they are isomorphic.
    pub fn add_matching(
        &mut self,
        matching: &Matching<'a>,
        from_rev: Revision,
        to_rev: Revision,
        is_exact: bool,
    ) {
        for (right_node, left_node) in matching.iter_right_to_left() {
            let left_rev_node = RevNode::new(from_rev, left_node);
            let right_rev_node = RevNode::new(to_rev, right_node);
            if from_rev == Revision::Left
                && to_rev == Revision::Right
                && let Some(Leader(RevNode {
                    rev: Revision::Base,
                    node: left_leader,
                })) = self.map.get(&left_rev_node)
                && let Some(Leader(RevNode {
                    rev: Revision::Base,
                    node: right_leader,
                })) = self.map.get(&right_rev_node)
                && left_leader != right_leader
            {
                // Adding this matching would render the class mapping inconsistent, as the nodes are
                // already matched to distinct base nodes. So ignore this matching.
                // TODO: consider following Spork in restricting this even more, by requiring that both nodes aren't matched at all
                // and that the parents of both nodes need to be matched together.
                continue;
            }
            let leader_left = self
                .map
                .get(&right_rev_node)
                .map_or(&right_rev_node, |leader| &leader.0);
            let leader_right = self
                .map
                .get(&left_rev_node)
                .map_or(&left_rev_node, |leader| &leader.0);
            let leader = Leader(if leader_left.rev < leader_right.rev {
                *leader_left
            } else {
                *leader_right
            });
            self.map.insert(left_rev_node, leader);
            self.map.insert(right_rev_node, leader);
            let repr = self.representatives.entry(leader).or_default();
            // keep track of exact matchings
            if is_exact && !repr.contains_key(&to_rev) {
                let exacts = self.exact_matchings.entry(leader).or_default();
                *exacts += 1;
            }
            repr.insert(to_rev, right_rev_node);
            repr.insert(from_rev, left_rev_node);
        }
    }

    /// Are the representatives of this leader all isomorphic?
    /// In this case, it's not worth trying to merge their contents.
    pub fn is_isomorphic_in_all_revisions(&self, leader: &Leader<'a>) -> bool {
        // if we know that at least two isomorphisms exist in the cluster, then by transitivity there are three of them
        // and all revisions are isomorphic for this node
        self.exact_matchings.get(leader).is_some_and(|n| *n >= 2)
    }

    /// Maps a node from some revision to its class representative
    pub fn map_to_leader(&self, rev_node: RevNode<'a>) -> Leader<'a> {
        self.map.get(&rev_node).copied().unwrap_or(Leader(rev_node))
    }

    /// Finds all the representatives in a cluster designated by its leader.
    /// This can return an empty map if the cluster only contains this node!
    fn internal_representatives(&self, leader: &Leader<'a>) -> &FxHashMap<Revision, RevNode<'a>> {
        self.representatives.get(leader).unwrap_or(&self.empty_repr)
    }

    /// The set of revisions for which we have a representative for this leader
    pub fn revision_set(&self, leader: &Leader<'a>) -> RevisionNESet {
        let mut set = RevisionNESet::singleton(leader.0.rev);
        self.internal_representatives(leader)
            .keys()
            .for_each(|k| set.add(*k));
        set
    }

    /// The set of representatives for this leader
    pub fn representatives(&self, leader: &Leader<'a>) -> Vec<RevNode<'a>> {
        let mut vec = self
            .internal_representatives(leader)
            .values()
            .copied()
            .collect_vec();
        if vec.is_empty() {
            vec.push(leader.as_representative());
        }
        vec
    }

    /// The list of children of the representative of the given leader at that revision,
    /// represented themselves as leaders of their own clusters.
    pub fn children_at_revision(
        &self,
        leader: &Leader<'a>,
        revision: Revision,
    ) -> Option<Vec<Leader<'a>>> {
        let repr = if leader.0.rev == revision {
            &leader.0
        } else {
            self.internal_representatives(leader).get(&revision)?
        };
        Some(
            repr.node
                .children
                .iter()
                .map(|c| self.map_to_leader(RevNode::new(revision, c)))
                .collect_vec(),
        )
    }

    /// The AST node corresponding to this leader at a given revision
    pub fn node_at_rev(
        &self,
        leader: &Leader<'a>,
        picked_revision: Revision,
    ) -> Option<&'a AstNode<'a>> {
        if leader.0.rev == picked_revision {
            Some(leader.0.node)
        } else {
            self.internal_representatives(leader)
                .get(&picked_revision)
                .map(|rn| rn.node)
        }
    }

    /// Checks whether the supplied revision (left or right) is only reformatting
    /// the source (the unindented sources are different as strings but the trees are
    /// isomorphic)
    pub fn is_reformatting(&self, leader: &Leader<'a>, revision: Revision) -> bool {
        if let Some(base) = self.node_at_rev(leader, Revision::Base)
            && let Some(rev) = self.node_at_rev(leader, revision)
        {
            base.hash == rev.hash && base.unindented_source() != rev.unindented_source()
        } else {
            false
        }
    }

    /// Returns the field name from which a leader can be obtained from its parent.
    /// In some cases it is possible that this field name differs from revision to revision.
    /// We currently ignore this case and just return the first field name of any representative
    /// of this leader.
    pub fn field_name(&self, leader: &Leader<'a>) -> Option<&'static str> {
        leader.as_representative().node.field_name.or_else(|| {
            self.internal_representatives(leader)
                .iter()
                .find_map(|(_, node)| node.node.field_name)
        })
    }
}

/// A set of [Revision]s
#[derive(Debug, PartialEq, Eq, Copy, Clone, PartialOrd, Ord, Hash)]
pub struct RevisionSet {
    base: bool,
    left: bool,
    right: bool,
}

impl RevisionSet {
    /// A set containing no revision
    pub fn new() -> Self {
        Self {
            base: false,
            left: false,
            right: false,
        }
    }

    /// Adds a revision to the set by modifying it
    pub fn add(&mut self, revision: Revision) {
        self.set(revision, true);
    }

    /// Adds a revision to the set by taking ownership
    pub fn with(mut self, revision: Revision) -> Self {
        self.add(revision);
        self
    }

    /// Removes a revision from this set
    pub fn remove(&mut self, revision: Revision) {
        self.set(revision, false);
    }

    /// Sets whether the revision belongs to the set
    pub fn set(&mut self, revision: Revision, presence: bool) {
        match revision {
            Revision::Base => self.base = presence,
            Revision::Left => self.left = presence,
            Revision::Right => self.right = presence,
        }
    }

    /// Does this set of revisions contain the given revision?
    pub fn contains(self, revision: Revision) -> bool {
        match revision {
            Revision::Base => self.base,
            Revision::Left => self.left,
            Revision::Right => self.right,
        }
    }

    /// Set intersection
    pub fn intersection(self, other: Self) -> Self {
        Self {
            base: self.base && other.base,
            left: self.left && other.left,
            right: self.right && other.right,
        }
    }

    /// Returns any revision contained in the set,
    /// by order of preference Left -> Right -> Base
    pub fn any(self) -> Option<Revision> {
        self.iter().next()
    }

    pub fn is_empty(self) -> bool {
        !(self.base || self.left || self.right)
    }

    /// Checked version of `is_empty`
    pub fn as_nonempty(self) -> Option<RevisionNESet> {
        if self.is_empty() {
            None
        } else {
            Some(RevisionNESet(self))
        }
    }

    pub fn is_full(self) -> bool {
        self.base && self.left && self.right
    }

    /// Iterates on the revisions contained in this set (returned in decreasing priority)
    pub fn iter(self) -> impl Iterator<Item = Revision> {
        iter::empty()
            .chain(self.left.then_some(Revision::Left))
            .chain(self.right.then_some(Revision::Right))
            .chain(self.base.then_some(Revision::Base))
    }
}

impl Default for RevisionSet {
    fn default() -> Self {
        Self::new()
    }
}

impl Display for RevisionSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "/{}{}{}/",
            if self.base { "B" } else { "." },
            if self.left { "L" } else { "." },
            if self.right { "R" } else { "." }
        )
    }
}

/// A non-empty [`RevisionSet`]
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct RevisionNESet(RevisionSet);

// do NOT implement `DerefMut` as well, since that would allow removing revisions, resulting in a
// possibly-no-longer-non-empty revision set
impl Deref for RevisionNESet {
    type Target = RevisionSet;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl RevisionNESet {
    /// Forget non-emptiness
    pub fn set(self) -> RevisionSet {
        self.0
    }

    /// A set containing a single revision
    pub fn singleton(revision: Revision) -> Self {
        let mut revisions = RevisionSet::new();
        revisions.add(revision);
        Self(revisions)
    }

    /// Adds a revision to the set by modifying it
    pub fn add(&mut self, revision: Revision) {
        self.0.add(revision);
    }

    /// Adds a revision to the set by taking ownership
    pub fn with(self, revision: Revision) -> Self {
        Self(self.0.with(revision))
    }

    /// Returns any revision contained in the set,
    /// by order of preference Left -> Right -> Base
    pub fn any(self) -> Revision {
        self.0
            .any()
            .expect("RevisionNonEmptySet is actually empty, oops")
    }
}

impl Display for RevisionNESet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use crate::test_utils::ctx;

    use super::*;

    /// If the left to right matching is inconsistent with the base to left and base to right matchings,
    /// then it is ignored.
    #[test]
    fn left_right_matching_does_not_override_base_matchings() {
        let ctx = ctx();

        let base_tree = ctx.parse("a.rs", "struct Foo;\nstruct Bar;\n");
        let left_tree = ctx.parse("a.rs", "struct Foo;\n");
        let right_tree = ctx.parse("a.rs", "struct Bar;\n");

        let foo_base = RevNode::new(Revision::Base, base_tree[0]);
        assert_eq!(foo_base.node.source, "struct Foo;");
        let bar_base = RevNode::new(Revision::Base, base_tree[1]);
        assert_eq!(bar_base.node.source, "struct Bar;");
        let foo_left = RevNode::new(Revision::Left, left_tree[0]);
        assert_eq!(foo_left.node.source, "struct Foo;");
        let bar_right = RevNode::new(Revision::Right, right_tree[0]);
        assert_eq!(bar_right.node.source, "struct Bar;");

        let mut base_left = Matching::new();
        base_left.add(base_tree, left_tree);
        base_left.add(foo_base.node, foo_left.node);
        let mut base_right = Matching::new();
        base_right.add(base_tree, right_tree);
        base_right.add(bar_base.node, bar_right.node);
        let mut left_right = Matching::new();
        left_right.add(left_tree, right_tree);
        left_right.add(foo_left.node, bar_right.node); // this matching is wrong!

        let mut class_mapping = ClassMapping::new();
        class_mapping.add_matching(&base_left, Revision::Base, Revision::Left, false);
        class_mapping.add_matching(&base_right, Revision::Base, Revision::Right, false);
        class_mapping.add_matching(&left_right, Revision::Left, Revision::Right, false);

        // because the wrong left-right matching is between nodes that were already matched to the base,
        // it was ignored and has not merged the classes of both nodes
        assert_ne!(
            class_mapping.map_to_leader(foo_left),
            class_mapping.map_to_leader(bar_right)
        );
    }

    /// If two out of three revisions are matched, then all three revisions get mapped to the same leader
    #[test]
    fn classes_are_properly_merged() {
        let ctx = ctx();

        let base_tree = ctx.parse(
            "a.rs",
            "struct FooBase;\nstruct BarBase;\nstruct HeyBase;\n",
        );
        let left_tree = ctx.parse(
            "a.rs",
            "struct FooLeft;\nstruct BarLeft;\nstruct HeyLeft;\n",
        );
        let right_tree = ctx.parse(
            "a.rs",
            "struct FooRight;\nstruct BarRight;\nstruct HeyRight;\n",
        );

        let foo_base = RevNode::new(Revision::Base, base_tree[0]);
        let foo_left = RevNode::new(Revision::Left, left_tree[0]);
        let foo_right = RevNode::new(Revision::Right, right_tree[0]);
        let bar_base = RevNode::new(Revision::Base, base_tree[1]);
        let bar_left = RevNode::new(Revision::Left, left_tree[1]);
        let bar_right = RevNode::new(Revision::Right, right_tree[1]);
        let hey_base = RevNode::new(Revision::Base, base_tree[2]);
        let hey_left = RevNode::new(Revision::Left, left_tree[2]);
        let hey_right = RevNode::new(Revision::Right, right_tree[2]);

        let mut base_left = Matching::new();
        base_left.add(base_tree, left_tree);
        //                                              FooBase and FooLeft are NOT matched
        base_left.add(bar_base.node, bar_left.node); // BarBase and BarLeft are matched
        base_left.add(hey_base.node, hey_left.node); // HeyBase and HeyLeft are matched

        let mut base_right = Matching::new();
        base_right.add(base_tree, right_tree);
        base_right.add(foo_base.node, foo_right.node); // FooBase and FooRight are matched
        //                                                BarBase and BarRight are NOT matched
        base_right.add(hey_base.node, hey_right.node); // HeyBase and HeyRight are matched

        let mut left_right = Matching::new();
        left_right.add(left_tree, right_tree);
        left_right.add(foo_left.node, foo_right.node); // FooLeft and FooRight are matched
        left_right.add(bar_left.node, bar_right.node); // BarLeft and BarRight are matched
        //                                                HeyLeft and HeyRight are NOT matched

        let mut class_mapping = ClassMapping::new();
        class_mapping.add_matching(&base_left, Revision::Base, Revision::Left, false);
        class_mapping.add_matching(&base_right, Revision::Base, Revision::Right, false);
        class_mapping.add_matching(&left_right, Revision::Left, Revision::Right, false);

        // matchings of Foo look like: Base <-> Right <-> Left
        let expected_foo_leader = Leader(foo_base);
        assert_eq!(class_mapping.map_to_leader(foo_base), expected_foo_leader);
        assert_eq!(class_mapping.map_to_leader(foo_left), expected_foo_leader);
        assert_eq!(class_mapping.map_to_leader(foo_right), expected_foo_leader);
        assert!(class_mapping.revision_set(&expected_foo_leader).is_full());

        // matchings of Bar look like: Base <-> Left <-> Right
        let expected_bar_leader = Leader(bar_base);
        assert_eq!(class_mapping.map_to_leader(bar_base), expected_bar_leader);
        assert_eq!(class_mapping.map_to_leader(bar_left), expected_bar_leader);
        assert_eq!(class_mapping.map_to_leader(bar_right), expected_bar_leader);
        assert!(class_mapping.revision_set(&expected_bar_leader).is_full());

        // matchings of Hey look like: Left <-> Base <-> Right
        let expected_hey_leader = Leader(hey_base);
        assert_eq!(class_mapping.map_to_leader(hey_base), expected_hey_leader);
        assert_eq!(class_mapping.map_to_leader(hey_left), expected_hey_leader);
        assert_eq!(class_mapping.map_to_leader(hey_right), expected_hey_leader);
        assert!(class_mapping.revision_set(&expected_hey_leader).is_full());
    }
}
