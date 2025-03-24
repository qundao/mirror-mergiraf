# Conflicts solved

This page gives an overview of the sorts of conflicts that Mergiraf is able to handle and which ones are left for the user to solve.

## Changes to independent syntax elements

Consider the following situation:
<div class="conflict">
<div class="left">
<div class="rev">Left</div>

```c
void notify_attendees(long status_code);
```

</div>
<div class="base">
<div class="rev">Base</div>

```c
void notify_attendees(int status_code);
```

</div>
<div class="right">
<div class="rev">Right</div>

```c
int notify_attendees(int status_code);
```

</div>
</div>

The left side changes the type of the argument of this C function while the right side changes its return type.
Both changes can be done independently, so this is resolved to:
```c
int notify_attendees(long status_code);
```

## Neighbouring insertions and deletions of elements whose order does not matter

Another example:

<div class="conflict">
<div class="left">
<div class="rev">Left</div>

```java
{{#include ../../examples/java/working/class_fields/Left.java}}
```

</div>
<div class="base">
<div class="rev">Base</div>

```java
{{#include ../../examples/java/working/class_fields/Base.java}}
```

</div>
<div class="right">
<div class="rev">Right</div>

```java
{{#include ../../examples/java/working/class_fields/Right.java}}
```

</div>
</div>

The left and right sides add different attributes to the same Java class. The order of declaration of those attributes does not matter, so the conflict can be resolved to:
```java
{{#include ../../examples/java/working/class_fields/Expected.java}}
```

In contrast to this, conflicting additions of instructions in a block, or conflicting additions of arguments to a function declaration are not automatically resolved as above, given that the order in which they are inserted matters.
<div class="conflict">
<div class="left">
<div class="rev">Left</div>

```java
{{#include ../../examples/java/working/statements/Left.java}}
```

</div>
<div class="base">
<div class="rev">Base</div>

```java
{{#include ../../examples/java/working/statements/Base.java}}
```

</div>
<div class="right">
<div class="rev">Right</div>

```java
{{#include ../../examples/java/working/statements/Right.java}}
```

</div>
</div>

In this example, human intervention is needed to decide in which order the `rechargeBatteries()` and `returnToHomeBase()` statements should be inserted, so a conflict is created.

This type of conflict resolution is enabled for a set of syntactic contexts (called "commutative parents") configured on a per-language basis. To enable the resolution above, we mark `field_declaration_list` nodes in Java to be commutative parents. Similarly, the order of attributes in an HTML tag is irrelevant, so given the following revisions

<div class="conflict">
<div class="left">
<div class="rev">Left</div>

```xml
<input type="text" autocomplete="off" />
```

</div>
<div class="base">
<div class="rev">Base</div>

```xml
<input type="text" />
```

</div>
<div class="base">
<div class="rev">Right</div>

```xml
<input type="text" value="hello" />
```

</div>
</div>

Mergiraf will output the following, since `self_closing_tag` is marked as a commutative parent in HTML:
```xml
<input type="text" autocomplete="off" value="hello" />
```

<div class="warning">
Strictly speaking, the order of declaration of class attributes or methods in Java can have an influence on program execution (for instance via the use of reflection). Explicit reliance on this order is broadly discouraged so this tool assumes that such conflicts can be resolved without human intervention. The same judgment is done for other types of syntactic elements, in Java and in other languages. In all cases where Mergiraf relies on order independence to solve conflicts, it still attempts to preserve the order of elements on both sides, and does not reorder elements at all in the absence of a conflict.
</div>


## Conflicting formatting and content changes

When one side reformats the file and the other makes changes to its contents, Mergiraf attempts to retain both the new formatting and the new contents.

In the following example, the left side reformats a function declaration and the right side changes the type of one of its arguments.
<div class="conflict">
<div class="left">
<div class="rev">Left</div>

```rust
{{#include ../../examples/rust/working/reformat/Left.rs}}
```

</div>
<div class="base">
<div class="rev">Base</div>

```rust
{{#include ../../examples/rust/working/reformat/Base.rs}}
```

</div>
<div class="right">
<div class="rev">Right</div>

```rust
{{#include ../../examples/rust/working/reformat/Right.rs}}
```

</div>
</div>

In this case, Mergiraf produces the following merge:
```rust
{{#include ../../examples/rust/working/reformat/Expected.rs}}
```

<div class="warning">
Merging formatting and content changes is done on a best-effort basis and is bound to produce imperfect formatting
in certain cases. Those types of conflicts are best avoided by enforcing a particular style via a linter.
</div>

## Moving edited elements

Mergiraf can detect that a particular section of source code has been moved in one revision and has been modified by the other revision.
In this case, the changes on the latter branch are replayed at the new location. Consider this example:
<div class="conflict">
<div class="left">
<div class="rev">Left</div>

```rust
{{#include ../../examples/rust/working/move_to_method/Left.rs}}
```

</div>
<div class="base">
<div class="rev">Base</div>

```rust
{{#include ../../examples/rust/working/move_to_method/Base.rs}}
```

</div>
<div class="right">
<div class="rev">Right</div>

```rust
{{#include ../../examples/rust/working/move_to_method/Right.rs}}
```

</div>
</div>

The left revision extracts the boolean condition out of the closure to turn it into a method, making some changes to it in the same go.
The right revision makes some other changes to the boolean expression (turning `Red` into `Blue`).
In such a case, Mergiraf is able to replay the changes of the right branch onto the new location of the boolean expression in the left branch,
which gives the following result:

```rust
{{#include ../../examples/rust/working/move_to_method/Expected.rs}}
```

<div class="warning">

Resolving this sort of conflicts is generally not possible in [fast mode](./architecture.md#fast-mode) and will only work when Mergiraf has access to the original base, left and right revisions.

</div>

## Line-based merges

In addition to the above, if the files merge cleanly using Git's usual line-based merging algorithm, so will they with Mergiraf.
There is however one notable exception. Consider the following situation:

<div class="conflict">
<div class="left">
<div class="rev">Left</div>

```json
{{#include ../../examples/json/working/for_docs/Left.json}}
```

</div>
<div class="base">
<div class="rev">Base</div>

```json
{{#include ../../examples/json/working/for_docs/Base.json}}
```

</div>
<div class="right">
<div class="rev">Right</div>

```json
{{#include ../../examples/json/working/for_docs/Right.json}}
```

</div>
</div>

Git's line-based merging algorithm happily merges those revisions into:

```json
{
    "new_letter": "left value",
    "alpha": "α",
    "beta": "β",
    "gamma": "γ",
    "delta": "δ",
    "new_letter": "right value"
}
```

This is a problem, as the `"new_letter"` key appears twice. In such a situation, Mergiraf outputs a conflict:

```json
{{#include ../../examples/json/working/for_docs/Expected.json}}
```

This works by defining so-called "signatures" for certain children of commutative parents. A signature defines how to build a key for a syntactic element, child of a commutative parent.
Such keys should be unique among all children of a given commutative parent. Beyond this example in JSON, this mechanism is used to ensure the uniqueness of import statements, method signatures, struct fields and many other syntactic constructs in various
languages. For more details about how they are defined, see [the tutorial to teach Mergiraf a new language](./adding-a-language.md#add-signatures).

## And what about human conflicts?

Have you ever heard of [nonviolent communication](https://en.wikipedia.org/wiki/Nonviolent_Communication), also known as giraffe language? It's an interesting framework, suggesting which communication patterns to use or avoid when tensions arise. Check it out!
