use std::{
    borrow::Cow,
    fs,
    path::{Path, PathBuf},
    process::exit,
};

use clap::{Parser, Subcommand};
use mergiraf::{
    lang_profile::LangProfile,
    // XXX: move the uses to lib to avoid making these public?
    newline::normalize_to_lf,
};
use tree_sitter::Parser as TSParser;
use typed_arena::Arena;

/// Dev helper for contributing to Mergiraf
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
#[command(propagate_version = true)]
struct CliArgs {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
#[deny(missing_docs)]
enum Command {
    /// Print the parsed tree for a file, for debugging purposes
    Parse {
        /// Path to the file to parse. Its type will be guessed from its extension.
        path: PathBuf,
    },
    /// Compare two files, returning exit code 0 if their trees are isomorphic, and 1 otherwise
    Compare {
        /// Path to the first file
        first: PathBuf,
        /// Path to the second file
        second: PathBuf,
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

    let language_determining_path = match &args.command {
        Command::Parse { path } => path,
        Command::Compare { first, .. } => first,
    };

    let lang_profile =
        LangProfile::detect_from_filename(language_determining_path).ok_or_else(|| {
            format!(
                "Could not detect a supported language for {}",
                language_determining_path.display()
            )
        })?;

    let mut parser = TSParser::new();
    parser
        .set_language(&lang_profile.language)
        .map_err(|err| format!("Error loading {} grammar: {}", lang_profile.name, err))?;

    let contents = |path: &Path| -> Result<Cow<str>, String> {
        let original_contents = fs::read_to_string(path)
            .map_err(|err| format!("Could not read {}: {err}", path.display()))?;
        let contents = normalize_to_lf(original_contents);

        Ok(contents)
    };

    match &args.command {
        Command::Parse { path } => {
            let contents = contents(path)?;

            let tree = mergiraf::parse(&mut parser, &contents, lang_profile, &arena, &ref_arena)
                .map_err(|err| format!("File has parse errors: {err}"))?;

            print!("{}", tree.root().ascii_tree(lang_profile));
            Ok(0)
        }
        Command::Compare { first, second } => {
            let contents_first = contents(first)?;

            let tree_first = mergiraf::parse(
                &mut parser,
                &contents_first,
                lang_profile,
                &arena,
                &ref_arena,
            )
            .map_err(|err| format!("File has parse errors: {err}"))?;

            let contents_second = contents(second)?;

            let tree_second = mergiraf::parse(
                &mut parser,
                &contents_second,
                lang_profile,
                &arena,
                &ref_arena,
            )
            .map_err(|err| format!("File has parse errors: {err}"))?;

            Ok(if tree_first.root().isomorphic_to(tree_second.root()) {
                0
            } else {
                1
            })
        }
    }
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
    fn test_isomorphism_identical_files() {
        assert_eq!(
            real_main(&CliArgs::parse_from([
                "mgf_dev",
                "compare",
                "examples/java/working/demo/Base.java",
                "examples/java/working/demo/Base.java",
            ])),
            Ok(0)
        );
    }

    #[test]
    fn test_isomorphism_isomorphic_trees() {
        assert_eq!(
            real_main(&CliArgs::parse_from([
                "mgf_dev",
                "compare",
                "examples/java/working/reformat/Base.java",
                "examples/java/working/reformat/Left.java",
            ])),
            Ok(0)
        );
    }

    #[test]
    fn test_isomorphism_different_trees() {
        assert_eq!(
            real_main(&CliArgs::parse_from([
                "mgf_dev",
                "compare",
                "examples/java/working/demo/Base.java",
                "examples/java/working/demo/Left.java",
            ])),
            Ok(1)
        );
    }
}
