use std::{
    borrow::Cow,
    fs,
    path::{Path, PathBuf},
    process::exit,
};

use clap::{Parser, Subcommand};
use mergiraf::{
    ast::AstNode,
    lang_profile::LangProfile,
    // XXX: move the uses to lib to avoid making these public?
    newline::normalize_to_lf,
};
use typed_arena::Arena;

/// Dev helper for contributing to Mergiraf
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
#[command(propagate_version = true)]
struct CliArgs {
    #[command(subcommand)]
    command: Command,
    /// Override automatic language detection.
    #[arg(short = 'L', long, global = true)]
    language: Option<String>,
}

#[derive(Subcommand, Debug)]
#[deny(missing_docs)]
enum Command {
    /// Print the parsed tree for a file, for debugging purposes
    Parse {
        /// Path to the file to parse. Its type will be guessed from its extension.
        path: PathBuf,
        /// Limit the depth of the tree
        #[arg(short, long)]
        max_depth: Option<usize>,
    },
    /// Compare two files, returning exit code 0 if their trees are isomorphic, and 1 otherwise
    Compare {
        /// Path to the first file
        first: PathBuf,
        /// Path to the second file
        second: PathBuf,
        /// Enable commutative isomorphism checking, disregarding the order of nodes where it's not significant.
        #[arg(short, long)]
        commutative: bool,
    },
}

fn main() {
    stderrlog::new().module(module_path!()).init().unwrap();

    match real_main(&CliArgs::parse()) {
        Ok(exit_code) => exit(exit_code),
        Err(error) => {
            eprintln!("mgf_dev: {error}");
            exit(-1)
        }
    }
}

fn real_main(args: &CliArgs) -> Result<i32, String> {
    let arena = Arena::new();
    let ref_arena = Arena::new();

    let lang_profile = |language_determining_path| {
        LangProfile::find_by_filename_or_name(language_determining_path, args.language.as_deref())
    };

    let contents = |path: &Path| -> Result<Cow<str>, String> {
        let original_contents = fs::read_to_string(path)
            .map_err(|err| format!("Could not read {}: {err}", path.display()))?;
        let contents = normalize_to_lf(original_contents);

        Ok(contents)
    };

    let exit_code = match &args.command {
        Command::Parse { path, max_depth } => {
            let lang_profile = lang_profile(path)?;

            let contents = contents(path)?;

            let tree = AstNode::parse(&contents, lang_profile, &arena, &ref_arena)
                .map_err(|err| format!("File has parse errors: {err}"))?;

            print!("{}", tree.ascii_tree(*max_depth));
            0
        }
        Command::Compare {
            first,
            second,
            commutative,
        } => {
            let lang_profile = lang_profile(first)?;

            let contents_first = contents(first)?;

            let tree_first = AstNode::parse(&contents_first, lang_profile, &arena, &ref_arena)
                .map_err(|err| format!("File has parse errors: {err}"))?;

            let contents_second = contents(second)?;

            let tree_second = AstNode::parse(&contents_second, lang_profile, &arena, &ref_arena)
                .map_err(|err| format!("File has parse errors: {err}"))?;

            let first_root = tree_first;
            let second_root = tree_second;

            if first_root.isomorphic_to(second_root)
                || (*commutative && first_root.commutatively_isomorphic_to(second_root))
            {
                0
            } else {
                1
            }
        }
    };
    Ok(exit_code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_cli() {
        use clap::CommandFactory;
        CliArgs::command().debug_assert();
    }

    #[test]
    fn isomorphism_identical_files() {
        assert_eq!(
            real_main(&CliArgs::parse_from([
                "mgf_dev",
                "compare",
                "../examples/java/working/demo/Base.java",
                "../examples/java/working/demo/Base.java",
            ])),
            Ok(0)
        );
    }

    #[test]
    fn isomorphism_isomorphic_trees() {
        assert_eq!(
            real_main(&CliArgs::parse_from([
                "mgf_dev",
                "compare",
                "../examples/java/working/reformat/Base.java",
                "../examples/java/working/reformat/Left.java",
            ])),
            Ok(0)
        );
    }

    #[test]
    fn isomorphism_different_trees() {
        assert_eq!(
            real_main(&CliArgs::parse_from([
                "mgf_dev",
                "compare",
                "../examples/java/working/demo/Base.java",
                "../examples/java/working/demo/Left.java",
            ])),
            Ok(1)
        );
    }

    #[test]
    fn disabled_commutative_isomorphism() {
        assert_eq!(
            real_main(&CliArgs::parse_from([
                "mgf_dev",
                "compare",
                "../examples/rust/working/reordering_use_statements/Base.rs",
                "../examples/rust/working/reordering_use_statements/Left.rs",
            ])),
            Ok(1)
        );
    }

    #[test]
    fn enabled_commutative_isomorphism() {
        assert_eq!(
            real_main(&CliArgs::parse_from([
                "mgf_dev",
                "compare",
                "--commutative",
                "../examples/rust/working/reordering_use_statements/Base.rs",
                "../examples/rust/working/reordering_use_statements/Left.rs",
            ])),
            Ok(0)
        );
    }

    #[test]
    fn set_language() {
        let repo_dir = tempfile::tempdir().expect("failed to create the temp dir");
        let test_file = repo_dir.path().join("file.txt");
        fs::copy("../examples/java/working/demo/Base.java", &test_file)
            .expect("Failed to copy the Java file to the temporary directory");
        assert_eq!(
            real_main(&CliArgs::parse_from([
                "mgf_dev",
                "parse",
                "--language",
                "java",
                test_file.to_str().unwrap(),
            ])),
            Ok(0)
        );
    }
}
