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
    language: tree_sitter_c_sharp::LANGUAGE.into(), // the tree-sitter parser
    // optional settings, explained below
    atomic_nodes: vec![],
    commutative_parents: vec![],
    signatures: vec![],
    injections: None,
},
```

You can compile your new version of Mergiraf with:
```console
$ cargo build
```

You'll find the binary in `./target/debug/mergiraf`, which supports your language. 

That's all you need to get basic support for this language in Mergiraf. It already enables syntax-aware merging which should already give better results than line-based merging.

## Add commutative parents

You can improve conflict resolution for this language by defining "commutative parents".
A node in a syntax tree is a commutative parent when the order of its children is unimportant.
This knowledge allows Mergiraf to [automatically solve most conflicts involving insertion or deletion of children of such a parent](./conflicts.md#neighbouring-insertions-and-deletions-of-elements-whose-order-does-not-matter).

Identifying which node types should commutative is easier with some familiarity with the semantics of the language, but there are usual suspects you can consider:
* **import statements** (such as `import` in Java or Go, `use` in Rust…)
* **field or method declarations** in classes (as in most object-oriented programming languages)
* **declarations of sum-types** (such as `union` in C or functional programming languages)
* **dictionary or set objects** (such as JSON objects, struct instantiations in C/C++…)
* **declarative annotations** of various sorts (such as annotation parameters in Java, trait bounds in Rust, tag attributes in XML / HTML…)

For instance, C# has import statements called `using` declarations and [some IDEs seem to allow sorting them alphabetically](https://stackoverflow.com/questions/30374210/order-of-using-directives-in-c-sharp-alphabetically). This is a good sign that their order is semantically irrelevant, as in many languages, so let's declare that.

First, write a small sample file which contains the syntactic elements you are interested in, such as:
```csharp
using System;
using System.Collections.Generic;
using System.IO;

namespace HelloWorld {

    public class SomeName {

    }
}
```

You can inspect how this file is parsed with, either with the [Syntax Tree Playground](https://tree-sitter.github.io/tree-sitter/playground) if the language is supported there, or directly via Mergiraf:
```console
$ cargo run --bin mgf_dev parse test_file.cs
```

which gives:
<pre><font color="#5E5C64">└</font>compilation_unit
<font color="#5E5C64">  ├</font>using_directive
<font color="#5E5C64">  │ ├</font><font color="#C01C28">using</font>
<font color="#5E5C64">  │ ├</font>identifier <font color="#C01C28">System</font>
<font color="#5E5C64">  │ └</font><font color="#C01C28">;</font>
<font color="#5E5C64">  ├</font>using_directive
<font color="#5E5C64">  │ ├</font><font color="#C01C28">using</font>
<font color="#5E5C64">  │ ├</font>qualified_name
<font color="#5E5C64">  │ │ ├qualifier: </font>qualified_name
<font color="#5E5C64">  │ │ │ ├qualifier: </font>identifier <font color="#C01C28">System</font>
<font color="#5E5C64">  │ │ │ ├</font><font color="#C01C28">.</font>
<font color="#5E5C64">  │ │ │ └name: </font>identifier <font color="#C01C28">Collections</font>
<font color="#5E5C64">  │ │ ├</font><font color="#C01C28">.</font>
<font color="#5E5C64">  │ │ └name: </font>identifier <font color="#C01C28">Generic</font>
<font color="#5E5C64">  │ └</font><font color="#C01C28">;</font>
<font color="#5E5C64">  ├</font>using_directive
<font color="#5E5C64">  │ ├</font><font color="#C01C28">using</font>
<font color="#5E5C64">  │ ├</font>qualified_name
<font color="#5E5C64">  │ │ ├qualifier: </font>identifier <font color="#C01C28">System</font>
<font color="#5E5C64">  │ │ ├</font><font color="#C01C28">.</font>
<font color="#5E5C64">  │ │ └name: </font>identifier <font color="#C01C28">IO</font>
<font color="#5E5C64">  │ └</font><font color="#C01C28">;</font>
<font color="#5E5C64">  └</font>namespace_declaration
<font color="#5E5C64">    ├</font><font color="#C01C28">namespace</font>
<font color="#5E5C64">    ├name: </font>identifier <font color="#C01C28">HelloWorld</font>
<font color="#5E5C64">    └body: </font>declaration_list
<font color="#5E5C64">      ├</font><font color="#C01C28">{</font>
<font color="#5E5C64">      ├</font>class_declaration
<font color="#5E5C64">      │ ├</font>modifier
<font color="#5E5C64">      │ │ └</font><font color="#C01C28">public</font>
<font color="#5E5C64">      │ ├</font><font color="#C01C28">class</font>
<font color="#5E5C64">      │ ├name: </font>identifier <font color="#C01C28">SomeName</font>
<font color="#5E5C64">      │ └body: </font>declaration_list
<font color="#5E5C64">      │   ├</font><font color="#C01C28">{</font>
<font color="#5E5C64">      │   └</font><font color="#C01C28">}</font>
<font color="#5E5C64">      └</font><font color="#C01C28">}</font>
</pre>

This shows us how our source code is parsed into a tree. We see that the `using` statements are parsed as `using_directive` nodes in the tree.

To let Mergiraf reorder `using` statements to fix conflicts, we declare that their parent is a commutative one, which will by default let them commute with any of their siblings (any other child of their parent in the syntax tree).
In this example, their parent is the root of the tree (with type `compilation_unit`), which means that we'll allow reordering `using` statements with other top-level elements, such as the namespace declaration. 
We'll see later how to restrict this commutativity by defining children groups.

The commutative parent can be defined in the language profile:
```rust
LangProfile {
    commutative_parents: vec![
        CommutativeParent::without_delimiters("compilation_unit", "\n"),
    ],
    ..
},
```

A commutative parent is not only defined by a type of node, but also:
* the expected separator between its children (here, a newline: `"\n"`)
* any delimiters at the beginning and end of the list of children. Here, there are none, but in many cases, such lists start and end with characters such as `(` and `)` or `{` and `}`.

For instance, to declare that a JSON object is a commutative parent, we do so with
```rust
CommutativeParent::new("object", "{", ", ", "}")
```
Note how we use the separator is `", "` and not simply `","`. The separators and delimiters should come with sensible default whitespace around them. This whitespace is used as last resort, as Mergiraf attempts to imitate the surrounding style by reusing similar whitespace and indentation settings as existing delimiters and separators.

After having added our commutative parent definition, we can compile it again with `cargo build`. The resulting binary in `target/debug/mergiraf` will now accept to resolve conflicts like the following one:

<div class="conflict">
<div class="left">
<div class="rev">Left</div>

```csharp
using System;
using System.Collections.Generic;
```

</div>
<div class="base">
<div class="rev">Base</div>

```csharp
using System;
```

</div>
<div class="right">
<div class="rev">Right</div>

```csharp
using System;
using System.IO;
```

</div>
</div>

This will be merged to include all three `using` statements.
When inspecting how a file is parsed with `cargo run --bin mgf_dev parse test_file.cs`, commutative parents are highlighted in the tree, which also helps validate our definitions.

We can add other commutative parent definitions in the language profile. For instance, the declarations in the body of a class (such as `field_declaration` or `method_declaration`) can be freely reordered. This can be modeled by marking `declaration_list` nodes as being commutative parents:
```rust
CommutativeParent::new("declaration_list", "{", "\n\n", "}")
```
and so on. While it is useful to identify as many commutative parent types as possible, not being exhaustive is not a problem as it will only prevent the automated resolution of certain conflicts, but should not otherwise degrade the quality of merges.

## Adding children groups

Within a commutative parent, it is possible to restrict which types of children are able to commute together.
In C#, the `compilation_unit` root node can not only contain `using` statements, but also some global statements, namespace declarations and type declarations, for instance.
We can declare *children groups*, which are sets of node types which are allowed to commute together.
By declaring a children group which contains only `using_directive`, we make sure that `using` directive can only be reordered with other `using` directives:
```rust
CommutativeParent::new("declaration_list", "{", "\n\n", "}")
    .restricted_to_groups(&[
        &["using_directive"]
    ])
```

As soon as a children group is declared, that restricts the commutativity of all children of the commutative parent.
Conflicts can only be solved if all children involved are part of the same group. So, in this case it's also worth
adding other children groups for other types of nodes which can be reordered together:
```rust
CommutativeParent::new("declaration_list", "{", "\n\n", "}")
    .restricted_to_groups(&[
        &["using_directive"],
        &["namespace_declaration"],
        &["global_attribute"],
    ])
```

It is also possible to specify a different separator to be used when joining children of a specific group.
For instance, if we want nodes of `using_directive` type to be separated by one newline instead of two for the other types of nodes, we can specify it as follows:
```rust
CommutativeParent::new("declaration_list", "{", "\n\n", "}")
    .restricted_to(vec![
        ChildrenGroup::with_separator(&["using_directive"], "\n"),
        ChildrenGroup::new(&["namespace_declaration"]),
        ChildrenGroup::new(&["global_attribute"]),
    ]),
```

Note that the separator for a children group and the separator for the commutative parent can only differ in leading and trailing whitespace.

## Add signatures

One piece of knowledge we have not encoded yet is the fact that `using` statements should be unique: there is no point in importing the same thing twice. This is specified using so-called signatures, which associate keys to the children of commutative parents. Those keys are then required to be unique among the children of a particular commutative parent. This mechanism can be used to define such keys for a lot of other elements. For instance, class fields are keyed by their name only, given that field names should be unique in a given class, regardless of their type. Keys can also be generated for methods, which not only includes their name but also the types of the arguments the function takes, as [C# supports method overloading](https://learn.microsoft.com/en-us/dotnet/standard/design-guidelines/member-overloading).

To define a signature, we need to provide two pieces of information:
* the type of node we want to generate a key for, such as `field_declaration`
* a list of paths from the element being keyed to the descendant(s) making up its key. A path is itself a list of steps leading from the element to the descendant.

For instance:
```rust
signature("using_directive", vec![vec![]]),
signature("field_declaration", vec![vec![Field("name")]]),
signature("method_declaration", vec![
    vec![Field("name")],
    vec![Field("parameters"), ChildType("parameter"), Field("type")]
]),
```

Let's unpack what this all means.
* for `using_directive`, we supply a list containing a single path: the empty one. The empty path goes from the element being keyed to itself.
* for `field_declaration`, we supply again a list containing a single path, which has one step. This step fetches the `name` field of the element being keyed.
* for `method_declaration`, we have this time a list of two paths. The first one fetches the name, the second selects the types of the parameters in three steps.

This gives rise to the following signatures:

| Element type         | Example                              | Signature              |
|----------------------|--------------------------------------|------------------------|
| `using_directive`    | `using System.IO;`                   | `[[using System.IO]]`  |
| `field_declaration`  | `public int familySize;`             | `[[familySize]]`       |
| `method_declaration` | `void Run(int times, bool fast) { }` | `[[Run], [int, bool]]` |

Again, this can be checked with `cargo run --bin mgf_dev parse test_file.cs`, which shows the computed signatures in the tree.

To understand the difference between `Field` and `ChildType` in the signature definition for `method_declaration`, consider the structure of a method declaration as parsed by tree-sitter:

<pre><font color="#5E5C64"> └</font>method_declaration
<font color="#5E5C64">  ├type: </font>predefined_type <font color="#C01C28">void</font>
<font color="#5E5C64">  ├name: </font>identifier <font color="#C01C28">Run</font>
<font color="#5E5C64">  ├parameters: </font>parameter_list
<font color="#5E5C64">  │ ├</font><font color="#C01C28">(</font>
<font color="#5E5C64">  │ ├</font>parameter
<font color="#5E5C64">  │ │ ├type: </font>predefined_type <font color="#C01C28">int</font>
<font color="#5E5C64">  │ │ └name: </font>identifier <font color="#C01C28">times</font>
<font color="#5E5C64">  │ ├</font><font color="#C01C28">,</font>
<font color="#5E5C64">  │ ├</font>parameter
<font color="#5E5C64">  │ │ ├type: </font>predefined_type <font color="#C01C28">bool</font>
<font color="#5E5C64">  │ │ └name: </font>identifier <font color="#C01C28">fast</font>
<font color="#5E5C64">  │ └</font><font color="#C01C28">)</font>
<font color="#5E5C64">  └body: </font>block
<font color="#5E5C64">    ├</font><font color="#C01C28">{</font>
<font color="#5E5C64">    └</font><font color="#C01C28">}</font>
</pre>

Notice that some nodes have two labels attached to them:
* the field name, such as `name`, indicating which [field](https://tree-sitter.github.io/tree-sitter/creating-parsers#using-fields) of its parent node it belongs to. It is optional: some nodes like `parameter` ones are not associated to any field.
* the grammar name, such as `identifier`, which is the type of AST node. Every node has one (for separators or keywords, the source text is the grammar name)

In general, when descending into a single predetermined child of a given node, one should use a `Field`. If the number of children is variable then we expect to select them by grammar name using `ChildType`.

The grammar of a tree-sitter parser is defined in [a `grammar.js` file](https://github.com/tree-sitter/tree-sitter-c-sharp/blob/master/grammar.js) and reading it directly can be useful, for instance to understand what are the possible children or parent of a given type of node. Note that node types starting with `_` are private, meaning that they are not exposed to Mergiraf. In case of doubt, just parse some small example to check.

## Atomic nodes

Sometimes, the parser analyzes certain constructs with a granularity that is finer than what we need for structured merging. To treat a particular type of node as atomic and ignore any further structure in it, one can add its type to the `atomic_nodes` field.

This is also useful to work around [certain issues with parsers which don't expose the contents of certain string literals in the syntax trees](https://github.com/tree-sitter/tree-sitter-go/issues/150).

## Injections

Certain languages can contain text fragments in other languages. For instance, HTML can contain inline Javascript or CSS code.
The `injections` field on the `LangProfile` object can be used to provide a [tree-sitter query locating such fragments](https://tree-sitter.github.io/tree-sitter/3-syntax-highlighting.html#language-injection).
Such a query is normally exposed by the Rust crate for the parser as the `INJECTIONS_QUERY` constant if it has been defined by the parser authors, so it just needs wiring up as `injections: Some(tree_sitter_html::INJECTIONS_QUERY)`.

## Commutative parents via tree-sitter queries

Sometimes, commutative parents can't be defined just by specifying a node type inside which children should commute. It depends from the context whether this particular node should be treated as commutative or not.
For example, Python lists aren't commutative in general (the order matters for iteration,
indexing etc.), but they can be seen as commutative in an [`__all__` declaration](https://docs.python.org/3/tutorial/modules.html#importing-from-a-package).

We can define a [tree-sitter query](https://tree-sitter.github.io/tree-sitter/using-parsers/queries/1-syntax.html) to select which nodes should be treated as commutative.
The [tree-sitter playground](https://tree-sitter.github.io/tree-sitter/7-playground.html) can be useful to experiment with queries and find one that matches the desired set of nodes.
In the example above, the following query selects the relevant lists:
```tree-sitter
(expression_statement (assignment
  left: (identifier) @variable (#eq? @variable "__all__")
  right: (list) @commutative
))
```
Note the use of the `@commutative` capture to select which node matched by the query should be treated as commutative.
This query can then be used to define a commutative parent as part of the language profile:
```rust
CommutativeParent::from_query(
  r#"(expression_statement (assignment
     left: (identifier) @variable (#eq? @variable "__all__")
     right: (list) @commutative)
  )"#,
  "[", ", ", "]",
)
```

## Add tests

We didn't write any code, just declarative things, but it's still worth checking that the merging that they enable works as expected, and that it keeps doing so in the future.

You can add test cases to the end-to-end suite by following the directory structure of other such test cases. Create a directory of the form:
```
examples/csharp/working/add_imports
```

The naming of the `csharp` directory does not matter, nor does `add_imports` which describes the test case we are about to write. In this directory go the following files:
```
Base.cs
Left.cs
Right.cs
Expected.cs
```

All files should have an extension which matches what you defined in the language profile, for them to be parsed correctly. The `Base`, `Left` and `Right` files contain the contents of a sample file at all three revisions, and `Expected` contains the expected merge output of the tool (including any conflict markers).

To run an individual test, you can use a helper:
```console
$ helpers/inspect.sh examples/csharp/working/add_imports
```

This will show any differences between the expected output of the merge and the actual one. It also saves the result of some intermediate stages
of the merging process in the `debug` directory, such as the matchings between the three trees as Dotty graphs.
Those can be viewed as SVG files by running `helpers/generate_svg.sh`.


To run a test with a debugger, you can use the test defined in `tests/integration_tests.rs`:
```rust
// use this test to debug a specific test case by changing the path in it.
#[test]
fn debug_test() {
    run_test_from_dir(Path::new("examples/go/working/remove_and_add_imports"))
}
```
You can then use an IDE (such as Codium with Rust-analyzer) to set up breakpoints to inspect the execution of the test.

## Add documentation

The list of supported languages can be updated in `doc/src/languages.md`.

Mergiraf excitedly awaits your pull request!
