# Advanced langugage configuration

## Atomic nodes

Sometimes, the parser analyzes certain constructs with a granularity that is finer than what we need for structured merging. To treat a particular type of node as atomic and ignore any further structure in it, one can add its type to the `atomic_nodes` field.

This is also useful to work around [certain issues with parsers which don't expose the contents of certain string literals in the syntax trees](https://github.com/tree-sitter/tree-sitter-go/issues/150).

## Injections

Certain languages can contain text fragments in other languages. For instance, HTML can contain inline Javascript or CSS code.
The `injections` field on the `LangProfile` object can be used to provide a [tree-sitter query locating such fragments](https://tree-sitter.github.io/tree-sitter/3-syntax-highlighting.html#language-injection).
Such a query is normally exposed by the Rust crate for the parser as the `INJECTIONS_QUERY` constant if it has been defined by the parser authors, so it just needs wiring up as `injections: Some(tree_sitter_html::INJECTIONS_QUERY)`.

## Flattened nodes

Some parsers will represent certain constructs as nested applications of a binary operation. For instance, type unions in Typescript, such as:

```ts
export interface MyInterface {
  level: 'debug' | 'info' | 'warn' | 'error';
}
```

are represented by the grammar as:
```
└union_type
  ├union_type
  │ ├union_type
  │ │ ├literal_type
  │ │ │ └string
  │ │ │   ├'
  │ │ │   ├string_fragment debug
  │ │ │   └'
  │ │ ├|
  │ │ └literal_type
  │ │   └string
  │ │     ├'
  │ │     ├string_fragment info
  │ │     └'
  │ ├|
  │ └literal_type
  │   └string
  │     ├'
  │     ├string_fragment warn
  │     └'
  ├|
  └literal_type
    └string
      ├'
      ├string_fragment error
      └'
```

This nested structure prevents commutative merging of changes in such fragments. To work around that, Mergiraf supports flattening binary such binary operators into the following structure:
```
└union_type
  ├literal_type
  │ └string
  │   ├'
  │   ├string_fragment debug
  │   └'
  ├|
  ├literal_type
  │ └string
  │   ├'
  │   ├string_fragment info
  │   └'
  ├|
  ├literal_type
  │ └string
  │   ├'
  │   ├string_fragment warn
  │   └'
  ├|
  └literal_type
    └string
      ├'
      ├string_fragment error
      └'
```

This is achieved by specifying `flattened_nodes: &["union_type"]` in the language profile.

## Comment nodes

Another tweak that Mergiraf does on top of the parser's output is attaching comment nodes to the syntactic elements they annotate. This eases the commutative merging of such elements, by preventing those comments to get detached to their elements in the
merged output.

This heuristic is applied to all nodes that are [marked as "extra" by the tree-sitter grammar](https://tree-sitter.github.io/tree-sitter/creating-parsers/3-writing-the-grammar.html#using-extras) (meaning that the parser accepts to include them anywhere in the tree, even if they are not mentioned in a rule).
In certain cases, it can be useful to extend this heuristic to also attach other nodes, which behave as comments but aren't marked as "extra" in the grammar. This can be done by adding their node type to the `extra_comment_nodes` field of the language profile.

