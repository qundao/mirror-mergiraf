use std::{cmp::Ordering, collections::BinaryHeap};

use crate::tree::AstNode;

/// A priority queue which indexes trees by their height.
/// This follows the "indexed priority list" of
/// [Fine-grained and accurate source code differencing](https://hal.science/hal-01054552), Falleri et al. 2014.
#[derive(Debug, Default)]
pub struct PriorityList<'tree> {
    heap: BinaryHeap<Entry<'tree>>,
}

#[derive(Debug, PartialEq, Eq)]
struct Entry<'tree> {
    height: i32,
    node: &'tree AstNode<'tree>,
}

impl<'tree> From<&'tree AstNode<'tree>> for Entry<'tree> {
    fn from(node: &'tree AstNode<'tree>) -> Self {
        Self {
            height: node.height(),
            node,
        }
    }
}

impl<'tree> PriorityList<'tree> {
    /// Creates an empty priority list
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a new node to the priority list
    pub fn push(&mut self, node: &'tree AstNode<'tree>) {
        self.heap.push(Entry::from(node));
    }

    /// Returns the maximum height of the tree in the list
    pub fn peek_max(&self) -> Option<i32> {
        self.heap.peek().map(|entry| entry.height)
    }

    /// Returns the list of all nodes with maximum height
    pub fn pop<'a>(&'a mut self) -> Vec<&'tree AstNode<'tree>> {
        let desired_height = self.peek_max();
        let mut results = Vec::new();
        while desired_height.is_some() && desired_height == self.peek_max() {
            results.push(self.heap.pop().unwrap().node);
        }
        results
    }

    /// Adds all of the direct children of a node into the queue
    pub fn open(&mut self, node: &'tree AstNode<'tree>) {
        let entries = node.children.iter().copied().map(Entry::from);
        self.heap.extend(entries);
    }
}

impl Ord for Entry<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.height, self.node.source).cmp(&(other.height, other.node.source))
    }
}

impl PartialOrd for Entry<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::ctx;

    #[test]
    fn empty() {
        let mut priority_list = PriorityList::new();

        assert_eq!(priority_list.peek_max(), None);
        assert_eq!(priority_list.pop().len(), 0);
    }

    #[test]
    fn one_element() {
        let ctx = ctx();
        let mut priority_list = PriorityList::new();

        let node = ctx.parse_rust("fn x() -> i32 { 1 + 2 }").root();
        priority_list.push(node);

        assert_eq!(priority_list.peek_max(), Some(4));
        assert_eq!(priority_list.pop(), vec![node]);
    }

    #[test]
    fn two_elements_same_height() {
        let ctx = ctx();
        let mut priority_list = PriorityList::new();

        let node1 = ctx.parse_rust("fn y() -> u8 { 1 + 2 }").root();
        priority_list.push(node1);
        let node2 = ctx.parse_rust("fn z() { 3 *  5 }").root();
        priority_list.push(node2);

        assert_eq!(priority_list.peek_max(), Some(4));
        assert_eq!(priority_list.pop(), vec![node2, node1]);
    }

    #[test]
    fn two_elements_increasing_height() {
        let ctx = ctx();
        let mut priority_list = PriorityList::new();

        let node1 = ctx.parse_rust("fn a() { 1 + 2 }").root();
        priority_list.push(node1);
        let node2 = ctx.parse_rust("fn b() { 3 * (5 + 1) }").root();
        priority_list.push(node2);

        assert_eq!(priority_list.peek_max(), Some(6));
        assert_eq!(priority_list.pop(), vec![node2]);
    }

    #[test]
    fn two_elements_decreasing_height() {
        let ctx = ctx();
        let mut priority_list = PriorityList::new();

        let node1 = ctx.parse_rust("fn c() { 1 + (2 + 5) }").root();
        priority_list.push(node1);
        let node2 = ctx.parse_rust("fn d() { 3 * 9 }").root();
        priority_list.push(node2);

        assert_eq!(priority_list.peek_max(), Some(6));
        assert_eq!(priority_list.pop(), vec![node1]);
    }

    #[test]
    fn open() {
        let ctx = ctx();
        let mut priority_list = PriorityList::new();

        let node1 = ctx.parse_rust("fn x() { 1 + (2 + 5) }").root();
        let node1 = node1.child(0).unwrap().child(3).unwrap().child(1).unwrap();
        priority_list.open(node1);
        let child1 = node1.child(0).unwrap();
        let child2 = node1.child(1).unwrap();
        let child3 = node1.child(2).unwrap();

        assert_eq!(priority_list.peek_max(), Some(2));
        assert_eq!(priority_list.pop(), vec![child3]);
        assert_eq!(priority_list.peek_max(), Some(0));
        assert_eq!(priority_list.pop(), vec![child1, child2]);
    }
}
