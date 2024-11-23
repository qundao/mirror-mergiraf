use std::collections::HashSet;

use dot_generator::*;
use graphviz_rust::{dot_generator, dot_structures::*};

use crate::{
    tree::{Ast, AstNode},
    tree_matcher::DetailedMatching,
};

/// Renders a mapping between two trees as a dotty graph
pub fn matching_to_graph<'a>(
    left: &Ast<'a>,
    right: &Ast<'a>,
    mapping: &DetailedMatching<'a>,
) -> Graph {
    let left_prefix = "l";
    let right_prefix = "r";
    let (left_graph, visited_left) = tree_to_graph(
        left,
        left_prefix,
        &mapping.full.left_matched(),
        &mapping.exact.left_matched(),
    );
    let (right_graph, visited_right) = tree_to_graph(
        right,
        right_prefix,
        &mapping.full.right_matched(),
        &mapping.exact.right_matched(),
    );

    let mut g = graph!(id!("matching"));
    g.add_stmt(Stmt::Subgraph(left_graph));
    g.add_stmt(Stmt::Subgraph(right_graph));

    for (source_id, target_id) in mapping.exact.as_ids() {
        if visited_left.contains(&source_id) && visited_right.contains(&target_id) {
            let matching_edge = edge!(node_id!(format!("{}{}", left_prefix, source_id)) => node_id!(format!("{}{}", right_prefix, target_id)),
            vec![attr!("color", "red"), attr!("constraint", "false")]);
            g.add_stmt(Stmt::Edge(matching_edge));
        }
    }

    for (source_id, target_id) in mapping.container.as_ids() {
        if visited_left.contains(&source_id) && visited_right.contains(&target_id) {
            let matching_edge = edge!(node_id!(format!("{}{}", left_prefix, source_id)) => node_id!(format!("{}{}", right_prefix, target_id)),
            vec![attr!("color", "blue"), attr!("constraint", "false")]);
            g.add_stmt(Stmt::Edge(matching_edge));
        }
    }

    for (source_id, target_id) in mapping.recovery.as_ids() {
        if visited_left.contains(&source_id) && visited_right.contains(&target_id) {
            let matching_edge = edge!(node_id!(format!("{}{}", left_prefix, source_id)) => node_id!(format!("{}{}", right_prefix, target_id)),
            vec![attr!("color", "green"), attr!("constraint", "false")]);
            g.add_stmt(Stmt::Edge(matching_edge));
        }
    }
    g
}

/// Renders a tree as a dotty graph
pub fn tree_to_graph(
    node: &Ast<'_>,
    prefix: &str,
    matched: &HashSet<usize>,
    exactly_matched: &HashSet<usize>,
) -> (Subgraph, HashSet<usize>) {
    use dot_generator::*;
    let mut statements = Vec::new();
    let mut visited = HashSet::new();
    add_node(
        node.root(),
        &mut statements,
        prefix,
        matched,
        exactly_matched,
        &mut visited,
    );
    (
        Subgraph {
            id: id!(prefix),
            stmts: statements,
        },
        visited,
    )
}

fn add_node(
    node: &AstNode<'_>,
    graph: &mut Vec<Stmt>,
    prefix: &str,
    matched: &HashSet<usize>,
    exactly_matched: &HashSet<usize>,
    visited: &mut HashSet<usize>,
) -> String {
    visited.insert(node.id);
    let nodeid = format!("{}{}", prefix, node.id);
    let mut attrs = Vec::new();
    let label = if node.children.is_empty() {
        node.source.to_string()
    } else {
        node.grammar_name.to_string()
    };
    let label_with_range = format!(
        "{}:{}_{}",
        label, node.byte_range.start, node.byte_range.end
    );
    let shape = if node.children.is_empty() {
        "box"
    } else {
        "oval"
    };
    attrs.push(attr!("label", esc label_with_range.replace('\\', "\\\\").replace('"', "\\\"")));
    attrs.push(attr!("shape", esc shape));
    let is_exact_match = exactly_matched.contains(&node.id);
    if is_exact_match {
        attrs.push(attr!("style", "filled"));
        attrs.push(attr!("fillcolor", esc "#ff2222"));
    } else if !matched.contains(&node.id) {
        attrs.push(attr!("style", "filled"));
        attrs.push(attr!("fillcolor", esc "#40e0d0"));
    }
    let n = Node::new(NodeId(Id::Plain(nodeid.clone()), None), attrs);
    graph.push(Stmt::Node(n));
    if !is_exact_match {
        for child in node.children.iter() {
            let child_id = add_node(child, graph, prefix, matched, exactly_matched, visited);
            let edge = edge!(node_id!(nodeid) => node_id!(child_id));
            graph.push(Stmt::Edge(edge))
        }
    }
    nodeid
}

#[cfg(test)]
mod tests {
    use graphviz_rust::printer::{DotPrinter, PrinterContext};

    use crate::test_utils::ctx;

    use super::*;

    #[test]
    fn print_to_graphviz() {
        let ctx = ctx();
        let parsed = ctx.parse_json("{\"foo\": 3}");

        let (graph, _) = tree_to_graph(&parsed, "n", &HashSet::new(), &HashSet::new());
        let mut ctx = PrinterContext::default();

        let printed = graph.print(&mut ctx);
        assert!(printed.contains("subgraph ")) // yes, not a very great assertion… but node ids are all unstable!
    }
}
