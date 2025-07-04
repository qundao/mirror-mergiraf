use std::{
    collections::HashSet,
    fmt::Display,
    fs::{self, read_dir},
    path::{Path, PathBuf},
    process::Command,
};

use mergiraf::{
    ast::AstNode,
    class_mapping::{ClassMapping, Leader, RevNode, RevisionNESet},
    lang_profile::LangProfile,
    merge_3dm::{create_class_mapping, generate_matchings},
    merged_tree::MergedTree,
    pcs::Revision,
    settings::DisplaySettings,
    tree_matcher::TreeMatcher,
};
use rand::{Rng, SeedableRng, rngs::StdRng, seq::IndexedRandom};
use tempfile::tempdir;
use typed_arena::Arena;

/// Incrementally minimize a test case by removing elements synchronously
/// from all sides where they are present.
#[allow(clippy::too_many_arguments)]
pub fn minimize(
    test_case: &Path,
    script: &str,
    expected_exit_code: i32,
    output: Option<&PathBuf>,
    seed: Option<u64>,
    max_steps: i32,
    max_failures: i32,
    only_unchanged: bool,
) {
    let mut rng = if let Some(seed) = seed {
        StdRng::seed_from_u64(seed)
    } else {
        StdRng::from_os_rng()
    };

    let mut progress_made = true;
    let mut step = 0;
    let mut current_best = test_case.to_path_buf();
    let attempts_dir = tempdir()
        .expect("failed to create a temporary directory to store our minimization attempts");

    // Main loop: incrementally reduce the test case at each iteration
    while progress_made && step < max_steps {
        println!("\n----------- step {step} ---------\n");

        let mut failures = 0;
        progress_made = false;
        // Attempt many different ways to reduce the current test case, as long as they fail,
        // but only up to a maximum number of failures. Note that we're not keeping track of
        // what our failed attempts were, so we will often retry deleting the same element…
        while failures < max_failures && !progress_made {
            let new_test_case = attempts_dir.path().join(format!("{step}_{failures}"));
            progress_made = match attempt_minimization_step(
                &current_best,
                script,
                expected_exit_code,
                only_unchanged,
                &new_test_case,
                &mut rng,
            ) {
                Ok(()) => {
                    println!("New minimized case at {}", new_test_case.display());
                    current_best = new_test_case;
                    true
                }
                Err(failure) => {
                    println!("Failed attempt: {failure}");
                    failures += 1;
                    false
                }
            }
        }
        step += 1
    }

    // We stopped minimizing, let's save the latest test case to the output directory
    let default_output_path = PathBuf::from("/tmp/minimized");
    let final_output = output
        .unwrap_or(&default_output_path)
        .to_str()
        .expect("Invalid output path");
    println!("Finished after {step} minimizing steps.");
    println!("Saving the output to {final_output}");
    // Clear the output directory first
    Command::new("rm")
        .args(["-r", final_output])
        .output()
        .expect("Failed to clear the output path");
    Command::new("cp")
        .arg("-r")
        .arg(current_best)
        .arg(final_output)
        .output()
        .expect("Failed to copy the result to the output path");
}

/// All the possible reasons to fail a minimization attempt.
/// Internal errors are expected to generate panics.
enum AttemptFailure {
    /// Getting lost in the tree looking for a node to delete.
    /// For instance, if the tree is just a root, well, we can't
    /// delete anything.
    LostInTree(String),
    /// Deleting some nodes from a tree made its rendered version
    /// syntactically invalid. That was a bad choice of nodes.
    SyntaxError(String),
    /// Deleting the nodes from a tree still kept it syntactically valid,
    /// but re-parsing it gave us a tree that's not isomorphic to what
    /// we meant. The grammar is likely overly accepting.
    InconsistentTree,
    /// Running the script on the new files didn't give the expected
    /// error code.
    TestFailed(i32),
}

impl Display for AttemptFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AttemptFailure::LostInTree(node) => write!(f, "LostInTree: {node}"),
            AttemptFailure::SyntaxError(error) => write!(f, "SyntaxError: {error}"),
            AttemptFailure::InconsistentTree => write!(f, "InconsistentTree"),
            AttemptFailure::TestFailed(status_code) => write!(f, "TestFailed: {status_code}"),
        }
    }
}

/// Attempt to delete some nodes from the revisions and check
/// that the script still has the expected status code on the resulting files.
/// If successful, it writes the files in the supplied output directory.
fn attempt_minimization_step(
    test_case: &Path,
    script: &str,
    expected_exit_code: i32,
    only_unchanged: bool,
    output_dir: &Path,
    rng: &mut StdRng,
) -> Result<(), AttemptFailure> {
    let suffix = detect_suffix(test_case);
    let base_path = test_case.join(format!("Base{suffix}"));
    let left_path = test_case.join(format!("Left{suffix}"));
    let right_path = test_case.join(format!("Right{suffix}"));

    let contents_base =
        read_file_to_string(&base_path).expect("Could not read base file in test case");
    let contents_left =
        read_file_to_string(&left_path).expect("Could not read left file in attempt");
    let contents_right =
        read_file_to_string(&right_path).expect("Could not read right file in attempt");

    // TODO get better lang detection shared with the tests' logic
    let lang_profile = LangProfile::detect_from_filename(&base_path)
        .expect("Could not detect the language for the test case");

    // Parse the files
    let arena = Arena::new();
    let ref_arena = Arena::new();
    let tree_base = AstNode::parse(&contents_base, lang_profile, &arena, &ref_arena)
        .expect("Base file in test case doesn't parse");
    let tree_left = AstNode::parse(&contents_left, lang_profile, &arena, &ref_arena)
        .expect("Left file in test case doesn't parse");
    let tree_right = AstNode::parse(&contents_right, lang_profile, &arena, &ref_arena)
        .expect("Right file in test case doesn't parse");

    // Match all three pairs of trees
    let primary_matcher = TreeMatcher {
        min_height: 1,
        sim_threshold: 0.4,
        max_recovery_size: 100,
        use_rted: true,
    };
    let auxiliary_matcher = TreeMatcher {
        min_height: 2,
        sim_threshold: 0.6,
        max_recovery_size: 100,
        use_rted: false,
    };
    let (base_left_matching, base_right_matching, left_right_matching) = generate_matchings(
        tree_base,
        tree_left,
        tree_right,
        None,
        &primary_matcher,
        &auxiliary_matcher,
        None,
    );

    // Create a class mapping to identify which nodes belong to which revisions
    let class_mapping = create_class_mapping(
        &base_left_matching,
        &base_right_matching,
        &left_right_matching,
    );

    let nodes_to_delete = {
        let mut nodes_to_delete = HashSet::new();
        let revision_idx = rng.random_range(0..3);
        let (rev, tree) = [
            (Revision::Base, &tree_base),
            (Revision::Left, &tree_left),
            (Revision::Right, &tree_right),
        ][revision_idx];
        pick_nodes_to_delete(
            rev,
            tree,
            only_unchanged,
            &class_mapping,
            &mut nodes_to_delete,
            rng,
        )?;
        Ok(nodes_to_delete)
    }?;

    // Delete the nodes and check that the corresponding trees still parse.
    // More than parsing, we want them to be faithful to the intended AST.
    // TODO there is a lot of duplicated code here, but somehow I couldn't convince the
    //      borrow-checker to allow me refactoring it.
    let deleted_base =
        remove_nodes_in_tree(Revision::Base, tree_base, &class_mapping, &nodes_to_delete);
    let new_contents_base = deleted_base
        .to_merged_text(&class_mapping)
        .render(&DisplaySettings::default());
    check_deleted_output_is_consistent(
        &new_contents_base,
        Revision::Base,
        &deleted_base,
        lang_profile,
        &class_mapping,
        &arena,
        &ref_arena,
    )?;
    let deleted_left =
        remove_nodes_in_tree(Revision::Left, tree_left, &class_mapping, &nodes_to_delete);
    let new_contents_left = deleted_left
        .to_merged_text(&class_mapping)
        .render(&DisplaySettings::default());
    check_deleted_output_is_consistent(
        &new_contents_left,
        Revision::Left,
        &deleted_left,
        lang_profile,
        &class_mapping,
        &arena,
        &ref_arena,
    )?;
    let deleted_right = remove_nodes_in_tree(
        Revision::Right,
        tree_right,
        &class_mapping,
        &nodes_to_delete,
    );
    let new_contents_right = deleted_right
        .to_merged_text(&class_mapping)
        .render(&DisplaySettings::default());
    check_deleted_output_is_consistent(
        &new_contents_right,
        Revision::Right,
        &deleted_right,
        lang_profile,
        &class_mapping,
        &arena,
        &ref_arena,
    )?;

    for node in &nodes_to_delete {
        println!("deleting {node}");
    }

    // Write the attempt to disk
    fs::create_dir(output_dir).expect("Failed to create a new directory for the current attempt");
    fs::write(
        output_dir.join(format!("Base{suffix}")),
        new_contents_base.trim(),
    )
    .expect("Failed to write the base file to the attempt");
    fs::write(
        output_dir.join(format!("Left{suffix}")),
        new_contents_left.trim(),
    )
    .expect("Failed to write the left file to the attempt");
    fs::write(
        output_dir.join(format!("Right{suffix}")),
        new_contents_right.trim(),
    )
    .expect("Failed to write the right file to the attempt");

    // run the provided script and check that it has the expected exit code
    run_testing_command(script, expected_exit_code, output_dir)?;
    println!("successful testing script");

    Ok(())
}

/// Randomly select a set of nodes by climbing up the tree.
/// The nodes are guaranteed to appear in the same set of revisions and to be contiguous.
/// It returns an error if it got lost somewhere in the tree where there wasn't anything interesting to delete.
fn pick_nodes_to_delete<'a>(
    revision: Revision,
    tree: &'a AstNode<'a>,
    only_unchanged: bool,
    class_mapping: &ClassMapping<'a>,
    results: &mut HashSet<Leader<'a>>,
    rng: &mut StdRng,
) -> Result<(), AttemptFailure> {
    let child = tree
        .children
        .choose(rng)
        .ok_or_else(|| AttemptFailure::LostInTree(format!("{tree}")))?;

    let leader = class_mapping.map_to_leader(RevNode::new(revision, child));

    // We have two choices:
    // - either delete the child we picked
    // - or recurse into the child to delete a descendant of theirs
    let can_delete = !only_unchanged || is_unchanged(&leader, class_mapping);
    let can_recurse = !child.is_leaf();

    let probability_to_recurse = 0.8;

    if can_recurse && (!can_delete || rng.random_range(0.0..1.0) < probability_to_recurse) {
        pick_nodes_to_delete(revision, child, only_unchanged, class_mapping, results, rng)
    } else if can_delete {
        // Let's delete this node
        results.insert(leader);
        // TODO delete the following siblings if they have the same revision set?
        Ok(())
    } else {
        let revset = class_mapping.revision_set(&leader);
        Err(AttemptFailure::LostInTree(format!(
            "can't delete {leader}, present in {revset}"
        )))
    }
}

/// Check if a node is unchanged in all three revisions
fn is_unchanged<'a>(leader: &Leader<'a>, class_mapping: &ClassMapping<'a>) -> bool {
    if let (Some(base), Some(left), Some(right)) = (
        class_mapping.node_at_rev(leader, Revision::Base),
        class_mapping.node_at_rev(leader, Revision::Left),
        class_mapping.node_at_rev(leader, Revision::Right),
    ) {
        base.isomorphic_to(left) && base.isomorphic_to(right)
    } else {
        false
    }
}

/// Produce a new version of a tree with the specified nodes deleted
fn remove_nodes_in_tree<'a>(
    revision: Revision,
    tree: &'a AstNode<'a>,
    class_mapping: &ClassMapping<'a>,
    nodes_to_delete: &HashSet<Leader<'a>>,
) -> MergedTree<'a> {
    let processed_children: Vec<MergedTree<'a>> = tree
        .children
        .iter()
        .map(|child| class_mapping.map_to_leader(RevNode::new(revision, child)))
        .filter(|leader| !nodes_to_delete.contains(leader))
        .map(|leader| {
            remove_nodes_in_tree(
                revision,
                class_mapping
                    .node_at_rev(&leader, revision)
                    .expect("inconsistent class mapping, child is known to exist in this revision"),
                class_mapping,
                nodes_to_delete,
            )
        })
        .collect();
    let leader = class_mapping.map_to_leader(RevNode::new(revision, tree));
    if processed_children.len() == tree.children.len()
        && processed_children
            .iter()
            .all(|child| matches!(child, MergedTree::ExactTree { .. }))
    {
        MergedTree::new_exact(leader, RevisionNESet::singleton(revision), class_mapping)
    } else {
        MergedTree::new_mixed(leader, processed_children)
    }
}

/// Check that the rendered source code from the modified AST is still
/// syntactically valid and that the corresponding tree is isomorphic to the one we generated.
fn check_deleted_output_is_consistent<'a>(
    new_contents: &'a str,
    revision: Revision,
    merged_tree: &'a MergedTree<'a>,
    lang_profile: &'a LangProfile,
    class_mapping: &ClassMapping<'a>,
    arena: &'a Arena<AstNode<'a>>,
    ref_arena: &'a Arena<&AstNode<'a>>,
) -> Result<(), AttemptFailure> {
    if !merged_tree.isomorphic_to_source(
        AstNode::parse(new_contents, lang_profile, arena, ref_arena)
            .map_err(AttemptFailure::SyntaxError)?,
        revision,
        class_mapping,
    ) {
        Err(AttemptFailure::InconsistentTree)
    } else {
        Ok(())
    }
}

/// Run the testing script on an example and check that it has the expected status code
fn run_testing_command(
    script: &str,
    expected_exit_code: i32,
    path: &Path,
) -> Result<(), AttemptFailure> {
    let full_script = if script.contains("$1") {
        script.to_owned()
    } else {
        // if the script doesn't contain a $1, add one at the end so that it captures the path we give it
        format!("{script} $1")
    };
    let testing_result = Command::new("bash")
        .arg("-c")
        .arg(&full_script)
        .arg("testing_script")
        .arg(path)
        .output()
        .expect("failed to execute testing program via bash");
    let exit_code = testing_result
        .status
        .code()
        .expect("Expected an exit code from the testing program");
    let passes_test = exit_code == expected_exit_code;
    if passes_test {
        Ok(())
    } else {
        Err(AttemptFailure::TestFailed(exit_code))
    }
}

/// TODO copied from src/main.rs
fn read_file_to_string(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|err| format!("Could not read {}: {err}", path.display()))
}

/// TODO Copied from tests/common/mod.rs - is there a sensible place where to put this so it can be shared?
pub(crate) fn detect_suffix(test_dir: &Path) -> String {
    read_dir(test_dir)
        .expect("Could not list files in test directory")
        .find_map(|filename| {
            filename
                .unwrap()
                .file_name()
                .into_string()
                .expect("Unable to read filename in test directory")
                .strip_prefix("Base")
                .map(String::from)
        })
        .expect("Could not find a Base.* file in the test directory")
}
