use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::HashSet;

use crate::tree::AstNode;

/// A one-to-one relation between nodes of two trees.
#[derive(Debug, Default, Clone)]
pub struct Matching<'tree> {
    left_to_right: FxHashMap<&'tree AstNode<'tree>, &'tree AstNode<'tree>>,
    right_to_left: FxHashMap<&'tree AstNode<'tree>, &'tree AstNode<'tree>>,
}

impl<'tree> Matching<'tree> {
    /// Creates an empty matching.
    pub fn new() -> Self {
        Self::default()
    }

    /// Gets the matches associated with a node from the left hand tree
    pub fn get_from_left(&self, from: &'tree AstNode<'tree>) -> Option<&'tree AstNode<'tree>> {
        self.left_to_right.get(from).copied()
    }

    /// Gets the matches associated with a node from the right hand tree
    pub fn get_from_right(&self, from: &'tree AstNode<'tree>) -> Option<&'tree AstNode<'tree>> {
        self.right_to_left.get(from).copied()
    }

    /// Does the matching contain this pair?
    pub fn are_matched(&self, from: &'tree AstNode<'tree>, to: &AstNode<'tree>) -> bool {
        self.get_from_left(from) == Some(to)
    }

    /// Is it possible to add this pair while keeping the matching consistent?
    pub fn can_be_matched(&self, from: &AstNode<'tree>, to: &AstNode<'tree>) -> bool {
        from.grammar_name == to.grammar_name
            && !self.left_to_right.contains_key(from)
            && !self.right_to_left.contains_key(to)
            && (!from.is_leaf() || !to.is_leaf() || from.source == to.source) // TODO we could still accept to match them, but introduce content handling to merge them
    }

    /// Set of left node ids that are matched to any node on the right
    pub fn left_matched(&self) -> HashSet<usize> {
        self.left_to_right.keys().map(|c| c.id).collect()
    }

    /// Set of right node ids that are matched to any node on the left
    pub fn right_matched(&self) -> HashSet<usize> {
        self.right_to_left.keys().map(|c| c.id).collect()
    }

    /// Adds a match between two nodes (in both directions)
    pub fn add(&mut self, from: &'tree AstNode<'tree>, to: &'tree AstNode<'tree>) {
        self.remove(from, to);
        self.left_to_right.insert(from, to);
        self.right_to_left.insert(to, from);
    }

    /// Removes matches involving both elements (in both directions)
    pub fn remove(&mut self, from: &'tree AstNode<'tree>, to: &'tree AstNode<'tree>) {
        if let Some(other_right) = self.left_to_right.get(from) {
            self.right_to_left.remove(other_right);
            self.left_to_right.remove(from);
        }
        if let Some(other_left) = self.right_to_left.get(to) {
            self.left_to_right.remove(other_left);
            self.right_to_left.remove(to);
        }
    }

    /// Adds an entire other matching
    pub fn add_matching(&mut self, other: &Self) {
        for (right, left) in other.iter_right_to_left() {
            self.add(left, right);
        }
    }

    /// Number of matched nodes
    pub fn len(&self) -> usize {
        self.left_to_right.len()
    }

    /// Reverse the direction of the matching
    pub fn into_reversed(self) -> Self {
        Matching {
            left_to_right: self.right_to_left,
            right_to_left: self.left_to_right,
        }
    }

    // Compose two matchings together
    pub fn compose(&self, other: &Self) -> Self {
        let mut left_to_right = FxHashMap::default();
        let mut right_to_left = FxHashMap::default();
        for (source, target) in &self.left_to_right {
            if let Some(final_target) = other.get_from_left(target) {
                left_to_right.insert(*source, final_target);
                right_to_left.insert(final_target, *source);
            }
        }
        Self {
            left_to_right,
            right_to_left,
        }
    }

    // Assuming that the matches in this mapping are only between isomorphic nodes,
    // recursively match the descendants of all matched nodes
    pub fn add_submatches(&self) -> Self {
        let mut result = Self::new();
        for (right_match, left_match) in self.iter_right_to_left() {
            for (left_descendant, right_descendant) in left_match.dfs().zip(right_match.dfs()) {
                result.add(left_descendant, right_descendant);
            }
        }
        result
    }

    /// Retrieve match ids from left to right
    pub fn as_ids<'s>(&'s self) -> impl Iterator<Item = (usize, usize)> + use<'s, 'tree> {
        self.left_to_right
            .iter()
            .map(|(source, target)| (source.id, target.id))
    }

    /// Computes the dice coefficient of two trees according to this matching
    pub fn dice(&self, left: &'tree AstNode<'tree>, right: &'tree AstNode<'tree>) -> f32 {
        let size_left = left.size();
        let size_right = right.size();

        let right_descendants: FxHashSet<&AstNode<'_>> = right.dfs().collect();
        let mapped = left
            .dfs()
            .filter_map(|left_descendant| self.get_from_left(left_descendant))
            .filter(|mapped| right_descendants.contains(*mapped))
            .map(AstNode::own_weight)
            .sum::<usize>();
        2.0_f32 * (mapped as f32) / ((size_left + size_right) as f32)
    }

    /// Iterate over the matches, from right to left
    pub fn iter_right_to_left(
        &self,
    ) -> impl Iterator<Item = (&&'tree AstNode<'tree>, &&'tree AstNode<'tree>)> {
        self.right_to_left.iter()
    }

    /// Translate the matching to new trees, by assuming that the ids of each node are preserved
    pub fn translate<'b>(
        &self,
        new_left: &'b AstNode<'b>,
        new_right: &'b AstNode<'b>,
    ) -> Matching<'b> {
        let mapping_left = Self::index_tree(new_left);
        let mapping_right = Self::index_tree(new_right);
        let mut matching = Matching::new();
        for (right, left) in self.iter_right_to_left() {
            if let (Some(right_mapped), Some(left_mapped)) =
                (mapping_right.get(&right.id), mapping_left.get(&left.id))
            {
                matching.add(left_mapped, right_mapped);
            }
        }
        matching
    }

    fn index_tree<'a>(node: &'a AstNode<'a>) -> FxHashMap<usize, &'a AstNode<'a>> {
        node.dfs().map(|node| (node.id, node)).collect()
    }
}

#[cfg(test)]
mod tests {
    use itertools::Itertools;

    use crate::test_utils::ctx;

    use super::*;

    #[test]
    fn retrieve_match() {
        let ctx = ctx();

        let tree = ctx.parse_rust("fn t() { 3 }");
        let tree2 = ctx.parse_rust("fn t() { 1 }");

        let mut matching = Matching::new();
        assert_eq!(matching.len(), 0);

        matching.add(tree.root(), tree2.root());
        assert_eq!(matching.len(), 1);
        assert_eq!(
            matching.as_ids().collect_vec(),
            vec![(tree.root().id, tree2.root().id)]
        );
    }

    #[test]
    fn remove_previously_matched() {
        let ctx = ctx();

        let tree1 = ctx.parse_json("[1, 2, 3]");
        let tree2 = ctx.parse_json("[4, 5, 6]");

        let mut matching = Matching::new();

        let array1 = tree1.root().child(0).unwrap();
        let array2 = tree2.root().child(0).unwrap();

        let elem1 = array1.child(1).unwrap();
        assert_eq!(elem1.source, "1");
        let elem4 = array2.child(1).unwrap();
        let elem5 = array2.child(3).unwrap();

        matching.add(elem1, elem4);
        matching.add(elem1, elem5);

        assert_eq!(matching.get_from_right(elem5), Some(elem1));
        assert_eq!(matching.get_from_left(elem1), Some(elem5));
        assert_eq!(matching.get_from_right(elem4), None);

        matching.remove(elem1, elem4);
        assert_eq!(matching.get_from_right(elem4), None);
        assert_eq!(matching.get_from_left(elem1), None);
        assert_eq!(matching.get_from_right(elem5), None);
    }

    #[test]
    fn dice() {
        let ctx = ctx();

        let root = ctx.parse_rust("fn t() { 3 }").root();
        let mut matching = Matching::new();

        assert_eq!(matching.dice(root, root), 0.0_f32);
        root.dfs().for_each(|n| matching.add(n, n));

        assert_eq!(matching.dice(root, root), 1.0_f32);
    }
}
