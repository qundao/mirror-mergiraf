use std::{cmp::Ordering, fmt::Display, hash::Hash};

use crate::class_mapping::{Leader, RevisionNESet};

/// One of the three sides to be merged
#[derive(Debug, PartialEq, Eq, Hash, PartialOrd, Copy, Clone, Ord)]
pub enum Revision {
    Base,
    Left,
    Right,
}

/// A component of a [PCS] triple.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum PCSNode<'a> {
    /// A virtual marker corresponding to the root of the document, denoted by `⊥`
    VirtualRoot,
    /// A sentinel marking the start of a list of children, denoted by `⊣`
    LeftMarker,
    /// An actual node from the syntax trees to merge
    Node {
        /// The set of revisions in which this node is present
        revisions: RevisionNESet,
        /// The leader of its class in the class mapping
        node: Leader<'a>,
    },
    /// A sentinel marking the end of a list of children, denoted by `⊢`
    RightMarker,
}

/// A PCS triple, encoding a part of the structure of a tree.
/// It records that:
/// * the `parent` node is the parent of both `predecessor` and `successor`
/// * the `predecessor` appears immediately before `successor` in the list of children of `parent`
///
/// The PCS triple also records in which revision this fact holds.
/// To encode that a given node is the first child of its parent, we use [`PCSNode::LeftMarker`] as
/// predecessor, and similarly [`PCSNode::RightMarker`] is used as successor to encode the last child.
/// The actual root of the tree is encoded by marking it as root of the [`PCSNode::VirtualRoot`].
#[derive(Debug, Copy, Clone, PartialOrd, Ord)]
#[allow(clippy::upper_case_acronyms)]
pub struct PCS<'a> {
    /// The common parent of both the predecessor and successor
    pub parent: PCSNode<'a>,
    /// One of the children of the parent
    pub predecessor: PCSNode<'a>,
    /// The node immediately following the predecessor
    pub successor: PCSNode<'a>,
    /// The revision in which this relationship between (parent, predecessor, successor) holds
    pub revision: Revision,
}

impl PartialEq for PCS<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.parent == other.parent
            && self.predecessor == other.predecessor
            && self.successor == other.successor
    }
}

impl Eq for PCS<'_> {}

impl Hash for PCS<'_> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.parent.hash(state);
        self.predecessor.hash(state);
        self.successor.hash(state);
    }
}

impl Display for PCSNode<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PCSNode::VirtualRoot => write!(f, "⊥"),
            PCSNode::LeftMarker => write!(f, "⊣"),
            PCSNode::Node { node: rn, .. } => write!(f, "{rn}"),
            PCSNode::RightMarker => write!(f, "⊢"),
        }
    }
}

// only useful to list a changeset in a sort of meaningful way for debugging purposes
impl Ord for PCSNode<'_> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        #[allow(clippy::match_same_arms)]
        match (self, other) {
            (PCSNode::VirtualRoot, PCSNode::VirtualRoot) => Ordering::Equal,
            (PCSNode::VirtualRoot, _) => Ordering::Less,
            (_, PCSNode::VirtualRoot) => Ordering::Greater,
            (PCSNode::LeftMarker, PCSNode::LeftMarker) => Ordering::Equal,
            (PCSNode::LeftMarker, _) => Ordering::Less,
            (_, PCSNode::LeftMarker) => Ordering::Greater,
            (PCSNode::RightMarker, PCSNode::RightMarker) => Ordering::Equal,
            (PCSNode::RightMarker, _) => Ordering::Greater,
            (_, PCSNode::RightMarker) => Ordering::Less,
            (PCSNode::Node { node: leader_a, .. }, PCSNode::Node { node: leader_b, .. }) => {
                let a = leader_a.as_representative().node;
                let b = leader_b.as_representative().node;
                let key_a = (
                    a.byte_range.start,
                    a.byte_range.start as i32 - (a.byte_range.end as i32),
                    -a.height(),
                );
                let key_b = (
                    b.byte_range.start,
                    b.byte_range.start as i32 - (b.byte_range.end as i32),
                    -b.height(),
                );
                key_a.cmp(&key_b)
            }
        }
    }
}

impl PartialOrd for PCSNode<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(Ord::cmp(self, other))
    }
}

impl Display for Revision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Base => "Base",
            Self::Left => "Left",
            Self::Right => "Right",
        })
    }
}

impl Display for PCS<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "({}, {}, {}, {})",
            self.parent, self.predecessor, self.successor, self.revision
        )
    }
}
