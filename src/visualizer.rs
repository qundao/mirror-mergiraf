use std::collections::HashSet;
use std::fs::OpenOptions;
use std::io::{self, BufWriter, Write};
use std::path::Path;

use itertools::Itertools;
use log::error;

use crate::{
    ast::{Ast, AstNode},
    tree_matcher::DetailedMatching,
};

const COLOR_EXACTLY_MATCHED_NODE: &str = "#ff2222";
const COLOR_NON_FULLY_MATCHED_NODE: &str = "#40e0d0";
const COLOR_EXACT_MATCHING: &str = "red";
const COLOR_CONTAINER_MATCHING: &str = "blue";
const COLOR_RECOVERY_MATCHING: &str = "green";

/// Renders a mapping between two trees as a dotty graph
pub fn write_matching_to_dotty_file<'a>(
    path: impl AsRef<Path>,
    left: &Ast<'a>,
    right: &Ast<'a>,
    mapping: &DetailedMatching<'a>,
) {
    let path = path.as_ref();
    if let Err(err) = matching_to_graph(path, left, right, mapping) {
        error!(
            "Mergiraf: Could not write matching to {}: {err}",
            path.display()
        );
    }
}

pub fn matching_to_graph<'a>(
    path: &Path,
    left: &Ast<'a>,
    right: &Ast<'a>,
    mapping: &DetailedMatching<'a>,
) -> io::Result<()> {
    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)?;

    let mut writer = BufWriter::new(file);

    writeln!(writer, "graph matching {{")?;
    let left_prefix = "l";
    let right_prefix = "r";
    let visited_left = tree_to_graph(
        &mut writer,
        left,
        left_prefix,
        &mapping.full.left_matched(),
        &mapping.exact.left_matched(),
    )?;
    let visited_right = tree_to_graph(
        &mut writer,
        right,
        right_prefix,
        &mapping.full.right_matched(),
        &mapping.exact.right_matched(),
    )?;

    for (source_id, target_id) in mapping.exact.as_ids() {
        if visited_left.contains(&source_id) && visited_right.contains(&target_id) {
            writeln!(
                writer,
                "  {left_prefix}{source_id} -- {right_prefix}{target_id} [color={COLOR_EXACT_MATCHING},constraint=false]"
            )?;
        }
    }

    for (source_id, target_id) in mapping.container.as_ids() {
        if visited_left.contains(&source_id) && visited_right.contains(&target_id) {
            writeln!(
                writer,
                "  {left_prefix}{source_id} -- {right_prefix}{target_id} [color={COLOR_CONTAINER_MATCHING},constraint=false]"
            )?;
        }
    }

    for (source_id, target_id) in mapping.recovery.as_ids() {
        if visited_left.contains(&source_id) && visited_right.contains(&target_id) {
            writeln!(
                writer,
                "  {left_prefix}{source_id} -- {right_prefix}{target_id} [color={COLOR_RECOVERY_MATCHING},constraint=false]"
            )?;
        }
    }
    writeln!(writer, "}}")?;

    writer.flush()?;

    Ok(())
}

/// Renders a tree as a dotty graph
pub fn tree_to_graph<W: Write>(
    writer: &mut W,
    node: &Ast<'_>,
    prefix: &str,
    matched: &HashSet<usize>,
    exactly_matched: &HashSet<usize>,
) -> io::Result<HashSet<usize>> {
    let mut visited = HashSet::new();
    writeln!(writer, "  subgraph {prefix} {{")?;
    add_node(
        node.root(),
        writer,
        prefix,
        matched,
        exactly_matched,
        &mut visited,
    )?;
    writeln!(writer, "  }}")?;
    Ok(visited)
}

fn add_node<W: Write>(
    node: &AstNode<'_>,
    writer: &mut W,
    prefix: &str,
    matched: &HashSet<usize>,
    exactly_matched: &HashSet<usize>,
    visited: &mut HashSet<usize>,
) -> io::Result<String> {
    visited.insert(node.id);
    let nodeid = format!("{}{}", prefix, node.id);
    let mut attrs: Vec<(&str, &str)> = Vec::new();

    let label = if node.children.is_empty() {
        node.source
    } else {
        node.grammar_name
    };
    let label = label.replace('\\', "\\\\").replace('"', "\\\"");
    let label_with_range = format!(
        "{}:{}_{}",
        label, node.byte_range.start, node.byte_range.end
    );
    attrs.push(("label", &label_with_range));

    let shape = if node.is_leaf() { "box" } else { "oval" };
    attrs.push(("shape", shape));

    let is_exact_match = exactly_matched.contains(&node.id);
    let is_full_match = matched.contains(&node.id);
    if is_exact_match {
        attrs.push(("style", "filled"));
        attrs.push(("fillcolor", COLOR_EXACTLY_MATCHED_NODE));
    } else if !is_full_match {
        attrs.push(("style", "filled"));
        attrs.push(("fillcolor", COLOR_NON_FULLY_MATCHED_NODE));
    }

    writeln!(
        writer,
        "    {nodeid}[{}]",
        attrs
            .iter()
            .format_with(",", |(k, v), f| f(&format_args!("{k}=\"{v}\"")))
    )?;

    if !is_exact_match {
        for child in &node.children {
            let child_id = add_node(child, writer, prefix, matched, exactly_matched, visited)?;
            writeln!(writer, "    {nodeid} -- {child_id}")?;
        }
    }

    Ok(nodeid)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::test_utils::ctx;

    use super::*;

    #[test]
    fn print_to_graphviz() {
        let repo_dir = tempfile::tempdir().expect("failed to create the temp dir");
        let target_path = repo_dir.path().join("graph.dot");

        let ctx = ctx();
        let parsed_left = ctx.parse_json("{\"foo\": 3}");
        let parsed_right = ctx.parse_json("{\"foo\": 4}");
        let matching = DetailedMatching::default();

        matching_to_graph(&target_path, &parsed_left, &parsed_right, &matching).unwrap();

        let contents =
            fs::read_to_string(&target_path).expect("Could not read the generated graph.dot file");
        assert!(contents.contains("subgraph ")); // yes, not a very great assertion… but node ids are all unstable!
    }
}
