# Adding a language

In this tutorial, we will go through all the steps required to add support for a new language in Mergiraf, with C# as an example. As a prerequisite, we assume that you have cloned [Mergiraf's git repository](https://codeberg.org/mergiraf/mergiraf) and have [installed Rust](https://www.rust-lang.org/tools/install).

## Get a parser

The first step is to find a [tree-sitter parser](https://tree-sitter.github.io/tree-sitter/) for this language. The corresponding Rust crate needs to be added to Mergiraf's `Cargo.toml` file:

```toml
[dependencies]
tree-sitter-csharp = "0.21.3"
```

The version of the parser must be selected so that it is compatible with the version of tree-sitter that Mergiraf currently uses.

## Create a language profile

Then, go to `src/supported_langs.rs` and add a profile for the language. You can start with a minimal one, such as:
```rust
LangProfile {
    name: "C#", // used for the --language CLI option, and generating the list of supported languages
    alternate_names: vec![], // other possible values for --language
    extensions: vec!["cs"], // all file extensions for this language (note the lack of `.`!)
    file_names: vec![], // the full file names which should be handled with this language
    language: tree_sitter_c_sharp::LANGUAGE.into(), // the tree-sitter parser
    // optional settings, explained below
    commutative_parents: vec![],
    signatures: vec![],
    atomic_nodes: vec![],
    injections: None,
    flattened_nodes: &[],
    extra_comment_nodes: &[],
},
```

You can compile your new version of Mergiraf with:
```console
$ cargo build
```

You'll find the binary in `./target/debug/mergiraf`, which supports your language. 

That's all you need to get basic support for this language in Mergiraf. It already enables syntax-aware merging which should already give better results than line-based merging.

## Next steps

The `commutative_parents` and `signature` fields of the language profile can be used to 
[enable commutative merging](./adding-a-language/enabling-commutative-merging.md) for certain node types, which is recommended to improve the merge results. 
The remaining fields are available for [advanced use cases](./adding-a-language/advanced-language-configuration.md).

To submit your language configuration for inclusion in Mergiraf, we ask that you [add some tests](./adding-a-language/language-testing.md) to validate the merge output. The list of supported languages should also be updated in `doc/src/languages.md`.

Mergiraf excitedly awaits your pull request!
