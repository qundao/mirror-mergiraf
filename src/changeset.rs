use std::{
    fs::File,
    io::{BufWriter, Write},
    path::Path,
};

use itertools::Itertools;

use crate::{
    ast::{Ast, AstNode},
    class_mapping::{ClassMapping, RevNode},
    multimap::MultiMap,
    pcs::{PCS, PCSNode, Revision},
};

/// A set of [PCS] triples, with indices on all three components
/// for easier retrieval.
#[derive(Debug, Default)]
pub struct ChangeSet<'a> {
    successors: MultiMap<PCSNode<'a>, PCS<'a>>,
    predecessors: MultiMap<PCSNode<'a>, PCS<'a>>,
    parents: MultiMap<PCSNode<'a>, PCS<'a>>,
}

impl<'a> ChangeSet<'a> {
    /// Constructs an empty instance
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds PCS triples that encodes a tree
    pub fn add_tree(
        &mut self,
        tree: &'a Ast<'a>,
        revision: Revision,
        classmapping: &ClassMapping<'a>,
    ) {
        let root = self.add_node_recursively(
            tree.root(),
            PCSNode::VirtualRoot,
            PCSNode::LeftMarker,
            revision,
            classmapping,
        );
        self.add(PCS {
            parent: PCSNode::VirtualRoot,
            predecessor: root,
            successor: PCSNode::RightMarker,
            revision,
        });
    }

    fn add_node_recursively(
        &mut self,
        node: &'a AstNode<'a>,
        parent: PCSNode<'a>,
        predecessor: PCSNode<'a>,
        revision: Revision,
        classmapping: &ClassMapping<'a>,
    ) -> PCSNode<'a> {
        let rev_node = RevNode::new(revision, node);
        let leader = classmapping.map_to_leader(rev_node);
        let mut revision_set = classmapping.revision_set(leader);
        revision_set.add(revision); // just in case the node hadn't been mapped at all before

        let wrapped = PCSNode::Node {
            node: leader,
            revisions: revision_set,
        };

        self.add(PCS {
            parent,
            predecessor,
            successor: wrapped,
            revision,
        });

        // If the node happens to be a cluster where all three revisions are present and isomorphic,
        // then no need to do convert its subtree into PCS triples, we can just pretend it's a leaf
        if classmapping.is_isomorphic_in_all_revisions(leader) {
            return wrapped;
        }

        let mut current_predecessor = PCSNode::LeftMarker;
        for child in &node.children {
            current_predecessor = self.add_node_recursively(
                child,
                wrapped,
                current_predecessor,
                revision,
                classmapping,
            );
        }
        self.add(PCS {
            parent: wrapped,
            predecessor: current_predecessor,
            successor: PCSNode::RightMarker,
            revision,
        });

        wrapped
    }

    /// Adds a new PCS to the set
    pub fn add(&mut self, pcs: PCS<'a>) {
        self.successors.insert(pcs.successor, pcs);
        self.predecessors.insert(pcs.predecessor, pcs);
        self.parents.insert(pcs.parent, pcs);
    }

    /// Finds all the PCS which contain either the successor or predecessor of this PCS as successor or predecessor,
    /// and whose parent is different.
    pub fn other_roots(&self, pcs: &PCS<'a>) -> impl Iterator<Item = &PCS<'a>> {
        let mut results = Vec::new();
        if let PCSNode::Node { .. } = pcs.predecessor {
            results.extend(
                (self.predecessors.get(&pcs.predecessor).iter())
                    .chain(self.successors.get(&pcs.predecessor).iter())
                    .filter(|other| other.parent != pcs.parent),
            );
        }
        if let PCSNode::Node { .. } = pcs.successor {
            results.extend(
                (self.predecessors.get(&pcs.successor).iter())
                    .chain(self.successors.get(&pcs.successor).iter())
                    .filter(|other| other.parent != pcs.parent),
            );
        }
        results.into_iter()
    }

    /// Finds all the PCS that are successor-conflicting with this PCS
    #[cfg(test)]
    pub(crate) fn other_successors<'s, 'b>(
        &'s self,
        pcs: &'b PCS<'a>,
    ) -> impl Iterator<Item = &'s PCS<'a>> {
        self.parents.get(&pcs.parent).iter().filter(move |other| {
            other.successor != pcs.successor && other.predecessor == pcs.predecessor
        })
    }

    /// Finds all the inconsistent triples
    pub fn inconsistent_triples<'s, 'b>(
        &'s self,
        pcs: &'b PCS<'a>,
    ) -> impl Iterator<Item = &'s PCS<'a>> {
        self.parents
            .get(&pcs.parent)
            .iter()
            .filter(move |other| {
                (other.predecessor == pcs.predecessor) != (other.successor == pcs.successor)
            })
            .chain(self.other_roots(pcs))
    }

    /// Iterate over the PCS triples contained in this `ChangeSet`
    pub fn iter(&self) -> impl Iterator<Item = &PCS<'a>> {
        self.predecessors.values()
    }

    /// Number of PCS triples
    pub fn len(&self) -> usize {
        self.predecessors.len()
    }

    /// Save to file, for debugging purposes
    pub fn save(&self, fname: impl AsRef<Path>) {
        let f = File::create(fname).expect("Unable to open changeset file");
        let mut f = BufWriter::new(f);

        for pcs in self.iter().sorted() {
            writeln!(f, "{pcs}").expect("Unable to write changeset file");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use log::debug;
    use tempfile::tempdir;

    use crate::test_utils::ctx;

    use super::*;

    #[test]
    fn from_tree() {
        let ctx = ctx();

        let tree = ctx.parse_json("[1, [2, 3]]");

        let classmapping = ClassMapping::new();
        let mut changeset = ChangeSet::new();
        changeset.add_tree(&tree, Revision::Base, &classmapping);

        let as_strings = changeset
            .iter()
            .sorted()
            .map(|pcs| format!("({}, {}, {})", pcs.parent, pcs.predecessor, pcs.successor))
            .collect_vec();

        let expected = vec![
            "(⊥, ⊣, document:0…11@Base)",
            "(⊥, document:0…11@Base, ⊢)",
            "(document:0…11@Base, ⊣, array:0…11@Base)",
            "(document:0…11@Base, array:0…11@Base, ⊢)",
            "(array:0…11@Base, ⊣, [:0…1@Base)",
            "(array:0…11@Base, [:0…1@Base, number:1…2@Base)",
            "(array:0…11@Base, number:1…2@Base, ,:2…3@Base)",
            "(array:0…11@Base, ,:2…3@Base, array:4…10@Base)",
            "(array:0…11@Base, array:4…10@Base, ]:10…11@Base)",
            "(array:0…11@Base, ]:10…11@Base, ⊢)",
            "([:0…1@Base, ⊣, ⊢)",
            "(number:1…2@Base, ⊣, ⊢)",
            "(,:2…3@Base, ⊣, ⊢)",
            "(array:4…10@Base, ⊣, [:4…5@Base)",
            "(array:4…10@Base, [:4…5@Base, number:5…6@Base)",
            "(array:4…10@Base, number:5…6@Base, ,:6…7@Base)",
            "(array:4…10@Base, ,:6…7@Base, number:8…9@Base)",
            "(array:4…10@Base, number:8…9@Base, ]:9…10@Base)",
            "(array:4…10@Base, ]:9…10@Base, ⊢)",
            "([:4…5@Base, ⊣, ⊢)",
            "(number:5…6@Base, ⊣, ⊢)",
            "(,:6…7@Base, ⊣, ⊢)",
            "(number:8…9@Base, ⊣, ⊢)",
            "(]:9…10@Base, ⊣, ⊢)",
            "(]:10…11@Base, ⊣, ⊢)",
        ];

        assert_eq!(as_strings, expected);
    }

    #[test]
    fn single_tree_has_no_conflicts() {
        let ctx = ctx();

        let tree = ctx.parse_json("[1, [2, 3]]");

        let classmapping = ClassMapping::new();
        let mut changeset = ChangeSet::new();
        changeset.add_tree(&tree, Revision::Base, &classmapping);

        let empty_conflicts: Vec<&PCS> = vec![];
        for pcs in changeset.iter() {
            let conflicts = changeset.other_successors(pcs).collect_vec();
            for conflicting_pcs in &conflicts {
                debug!("conflict between {pcs} and {conflicting_pcs}");
            }
            assert_eq!(conflicts, empty_conflicts);
        }
    }

    #[test]
    fn write_to_file() {
        let ctx = ctx();

        let tree = ctx.parse_json("[1, 2]");

        let classmapping = ClassMapping::new();
        let mut changeset = ChangeSet::new();
        changeset.add_tree(&tree, Revision::Base, &classmapping);

        let tmp_dir = tempdir().expect("failed to create a temp dir");

        let path = tmp_dir.path().to_owned().join("changeset.txt");
        changeset.save(&path);

        let contents = fs::read_to_string(&path).expect("Failed to read the changeset.txt file");

        let expected_contents = r"(⊥, ⊣, document:0…6@Base, Base)
(⊥, document:0…6@Base, ⊢, Base)
(document:0…6@Base, ⊣, array:0…6@Base, Base)
(document:0…6@Base, array:0…6@Base, ⊢, Base)
(array:0…6@Base, ⊣, [:0…1@Base, Base)
(array:0…6@Base, [:0…1@Base, number:1…2@Base, Base)
(array:0…6@Base, number:1…2@Base, ,:2…3@Base, Base)
(array:0…6@Base, ,:2…3@Base, number:4…5@Base, Base)
(array:0…6@Base, number:4…5@Base, ]:5…6@Base, Base)
(array:0…6@Base, ]:5…6@Base, ⊢, Base)
([:0…1@Base, ⊣, ⊢, Base)
(number:1…2@Base, ⊣, ⊢, Base)
(,:2…3@Base, ⊣, ⊢, Base)
(number:4…5@Base, ⊣, ⊢, Base)
(]:5…6@Base, ⊣, ⊢, Base)
";
        assert_eq!(contents, expected_contents);
    }
}
