use std::{fmt::Display, hash::Hash};

use itertools::Itertools;
use rustc_hash::FxHashMap;

use crate::{matching::Matching, pcs::Revision, tree::AstNode};

/// A node together with a marker of which revision it came from.
#[derive(Debug, Copy, Clone)]
pub struct RevNode<'a> {
    pub rev: Revision,
    pub node: &'a AstNode<'a>,
}

/// A node at a revision, which happens to be the leader of its class
/// in a class-mapping.
#[derive(Debug, Copy, Clone, PartialEq, Hash)]
pub struct Leader<'a>(RevNode<'a>);

impl PartialEq for RevNode<'_> {
    fn eq(&self, other: &Self) -> bool {
        // because we know the nodes are from the same revision, it's safe to compare them just by their ids
        self.rev == other.rev && self.node.id == other.node.id
    }
}

impl Eq for RevNode<'_> {}
impl Eq for Leader<'_> {}

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
    /// We only add mappings for nodes which are previously not matched.
    /// The `is_exact` parameters indicates if two nodes being matched indicates that they are isomorphic.
    pub fn add_matching(
        &mut self,
        matching: &Matching<'a>,
        from_rev: Revision,
        to_rev: Revision,
        is_exact: bool,
    ) {
        for (right_node, left_match) in matching.iter_right_to_left() {
            let key = RevNode::new(to_rev, right_node);
            let left_rev_node = RevNode::new(from_rev, left_match);
            let leader = *self
                .map
                .entry(left_rev_node)
                .or_insert(Leader(left_rev_node));
            self.map.insert(key, leader);
            let repr = self.representatives.entry(leader).or_default();
            // keep track of exact matchings
            if is_exact && !repr.contains_key(&to_rev) {
                let exacts = self.exact_matchings.entry(leader).or_default();
                *exacts += 1;
            }
            repr.insert(to_rev, key);
            repr.insert(from_rev, left_rev_node);
        }
    }

    /// Are the representatives of this leader all isomorphic?
    /// In this case, it's not worth trying to merge their contents.
    pub fn is_isomorphic_in_all_revisions(&self, leader: Leader<'a>) -> bool {
        // if we know that at least two isomorphisms exist in the cluster, then by transitivity there are three of them
        // and all revisions are isomorphic for this node
        self.exact_matchings.get(&leader).is_some_and(|n| *n >= 2)
    }

    /// Maps a node from some revision to its class representative
    pub fn map_to_leader(&self, rev_node: RevNode<'a>) -> Leader<'a> {
        self.map.get(&rev_node).copied().unwrap_or(Leader(rev_node))
    }

    /// Finds all the representatives in a cluster designated by its leader.
    /// This can return an empty map if the cluster only contains this node!
    fn internal_representatives(&self, leader: Leader<'a>) -> &FxHashMap<Revision, RevNode<'a>> {
        self.representatives
            .get(&leader)
            .unwrap_or(&self.empty_repr)
    }

    /// The set of revisions for which we have a representative for this leader
    pub fn revision_set(&self, leader: Leader<'a>) -> RevisionNESet {
        let mut set = RevisionNESet::singleton(leader.0.rev);
        self.internal_representatives(leader)
            .keys()
            .for_each(|k| set.add(*k));
        set
    }

    /// The set of representatives for this leader
    pub fn representatives(&self, leader: Leader<'a>) -> Vec<RevNode<'a>> {
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
        leader: Leader<'a>,
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
        leader: Leader<'a>,
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
    pub fn is_reformatting(&self, leader: Leader<'a>, revision: Revision) -> bool {
        let base_source = self.node_at_rev(leader, Revision::Base);
        let rev_source = self.node_at_rev(leader, revision);
        if let (Some(base), Some(rev)) = (base_source, rev_source) {
            base.hash == rev.hash && base.unindented_source() != rev.unindented_source()
        } else {
            false
        }
    }

    /// Returns the field name from which a leader can be obtained from its parent.
    /// In some cases it is possible that this field name differs from revision to revision.
    /// We currently ignore this case and just return the first field name of any representative
    /// of this leader.
    pub fn field_name(&self, leader: Leader<'a>) -> Option<&'static str> {
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
        if self.left {
            Some(Revision::Left)
        } else if self.right {
            Some(Revision::Right)
        } else if self.base {
            Some(Revision::Base)
        } else {
            None
        }
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
        let mut vector = Vec::new();
        if self.left {
            vector.push(Revision::Left);
        }
        if self.right {
            vector.push(Revision::Right);
        }
        if self.base {
            vector.push(Revision::Base);
        }
        vector.into_iter()
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
        self.0.set(revision, true);
    }

    /// Adds a revision to the set by taking ownership
    pub fn with(self, revision: Revision) -> Self {
        Self(self.0.with(revision))
    }

    /// Does this set of revisions contain the given revision?
    pub fn contains(self, revision: Revision) -> bool {
        self.0.contains(revision)
    }

    /// Set intersection
    pub fn intersection(self, other: RevisionSet) -> RevisionSet {
        self.0.intersection(other)
    }

    /// Returns any revision contained in the set,
    /// by order of preference Left -> Right -> Base
    pub fn any(self) -> Revision {
        self.0
            .any()
            .expect("RevisionNonEmptySet is actually empty, oops")
    }

    pub fn is_full(self) -> bool {
        self.0.is_full()
    }
}

impl Display for RevisionNESet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
