use std::cmp::Ordering;

use nonempty_collections::{NEVec, nev};
use typed_arena::Arena;

use crate::lang_profile::CommutativeParent;

use super::AstNode;

enum BundlingState<'a> {
    Start,
    /// ```
    /// // first comment
    /// // ..more comments
    /// // last comment
    /// ```
    CollectingComments(NEVec<&'a AstNode<'a>>),
    /// ```
    /// fn foo() {}
    /// ```
    SingleNonComment(&'a AstNode<'a>),
    /// ```
    /// fn foo() {}
    /// // first comment
    /// // ..more comments
    /// // last comment
    /// ```
    ///
    /// NOTE: `non_comment` may be a node that can't be bundled into (e.g. a separator).
    /// That's required for edges case like:
    /// ```
    /// enum Foo {
    ///     A, // comment
    ///     B,
    /// }
    /// ```
    /// which have the following conflict:
    /// - `comment` is closer to the `,` than to `B`
    /// - but `,` can't be bundled into
    ///
    /// And where we therefore want to not bundle `comment` at all.
    BundlingCommentsFromBelow {
        non_comment: &'a AstNode<'a>,
        /// distance between `non_comment` and the first comment
        distance: usize,
        comments: NEVec<&'a AstNode<'a>>,
    },
}

impl<'a> AstNode<'a> {
    pub(super) fn bundle_comments(
        children: Vec<&'a Self>,
        global_source: &'a str,
        commutative_parent: &CommutativeParent,
        arena: &'a Arena<Self>,
        next_node_id: &mut usize,
    ) -> Vec<&'a Self> {
        let mut new_children: Vec<&'a Self> = Vec::with_capacity(children.len());
        let mut state = BundlingState::Start;
        for node in children {
            match (state, node.can_be_bundled()) {
                (BundlingState::Start, true) => {
                    // ```
                    // // node
                    // ```

                    // This comment is starting a new block, nothing to finalize before it.
                    // Start collecting `comments` to (maybe) bundle all together later
                    state = BundlingState::CollectingComments(nev![node]);
                }

                (BundlingState::Start, false) => {
                    // ```
                    // node
                    // ```

                    // Start a new block (which might or might not get bundled later)
                    state = BundlingState::SingleNonComment(node);
                }

                (BundlingState::SingleNonComment(non_comment), true) => {
                    let distance = non_comment.distance_to(node, global_source);
                    if distance <= 1 {
                        // ```
                        // non-comment
                        // // node
                        // ```

                        // Don't finalize `non-comment` yet
                        state = BundlingState::BundlingCommentsFromBelow {
                            non_comment,
                            distance,
                            comments: nev![node],
                        };
                    } else {
                        // ```
                        // non-comment
                        //
                        // // node
                        // ```

                        // This comment is starting a new block, finalize the `non-comment` before it
                        new_children.push(non_comment);
                        state = BundlingState::CollectingComments(nev![node]);
                    }
                }

                (BundlingState::SingleNonComment(non_comment), false) => {
                    // ```
                    // prev non-comment
                    // node
                    // ```

                    // Nothing to bundle -- just finalize `prev non-comment`
                    new_children.push(non_comment);
                    state = BundlingState::SingleNonComment(node);
                }

                (BundlingState::CollectingComments(mut comments), true) => {
                    if comments.last().is_close_enough_to(node, global_source) {
                        // ```
                        // // ..earlier comments
                        // // last comment
                        // // node
                        // ```

                        // This comment continues an existing block, nothing to finalize before it.
                        // Continue collecting `comments` to bundle all together, because it
                        // might be that we'll find a comment directly followed by non-comment,
                        // which would disallow bundling them
                        comments.push(node);
                        state = BundlingState::CollectingComments(comments);
                    } else {
                        // ```
                        // // ..earlier comments
                        // // last comment
                        //
                        // // node
                        // ```

                        // Finalize the current `comments`
                        new_children.extend(comments);
                        // Start new comment block with `node`
                        state = BundlingState::CollectingComments(nev![node]);
                    }
                }

                (BundlingState::CollectingComments(comments), false) => {
                    if node.can_be_bundled_into(commutative_parent)
                        && comments.last().is_close_enough_to(node, global_source)
                    {
                        // ```
                        // // ..earlier comments
                        // // last comment
                        // node
                        // ```

                        // Bundle `comments` into `node`, and make that the `prev non-comment` for the next nodes to look at
                        state = BundlingState::SingleNonComment(Self::bundle_comments_from_above(
                            node,
                            comments,
                            global_source,
                            arena,
                            next_node_id,
                        ));
                    } else {
                        // ```
                        // // ..earlier comments
                        // // last comment
                        //
                        // node
                        // ```

                        // Finalize `comments` separately
                        new_children.extend(comments);

                        state = BundlingState::SingleNonComment(node);
                    }
                }

                (
                    BundlingState::BundlingCommentsFromBelow {
                        non_comment,
                        distance,
                        mut comments,
                    },
                    true,
                ) => {
                    if comments.last().is_close_enough_to(node, global_source) {
                        // ```
                        // non-comm
                        // // first comment
                        // // .. further comments
                        // // node
                        // ```

                        // Just continue collecting `comments`
                        comments.push(node);
                        state = BundlingState::BundlingCommentsFromBelow {
                            non_comment,
                            distance,
                            comments,
                        };
                    } else {
                        // ```
                        // non-comm
                        // // first comment
                        // // .. furher comments
                        //
                        // // node
                        // ```

                        // Bundle existing `comments` into `prev non-comment`, and finalize that
                        new_children.push(Self::bundle_comments_from_below(
                            non_comment,
                            comments,
                            global_source,
                            arena,
                            next_node_id,
                        ));

                        // Start new comment block with `node`
                        state = BundlingState::CollectingComments(nev![node]);
                    }
                }

                (
                    BundlingState::BundlingCommentsFromBelow {
                        non_comment,
                        distance: distance_before,
                        comments,
                    },
                    false,
                ) => {
                    let distance_after = comments.last().distance_to(node, global_source);
                    match (
                        non_comment.can_be_bundled_into(commutative_parent),
                        distance_before.cmp(&distance_after),
                        node.can_be_bundled_into(commutative_parent),
                    ) {
                        (true, Ordering::Less, _) => {
                            // ```
                            // prev non-comment
                            // // ..earlier comments
                            // // last comment
                            //
                            // node
                            // ```

                            // Bundle `comments` into `prev non-comment`, and finalize that
                            new_children.push(Self::bundle_comments_from_below(
                                non_comment,
                                comments,
                                global_source,
                                arena,
                                next_node_id,
                            ));

                            // Make `node` the `prev non-comment` for the next nodes to look at
                            state = BundlingState::SingleNonComment(node);
                        }

                        // ```
                        // prev non-comment
                        // // ..earlier comments
                        // // last comment
                        // node
                        // ```
                        //
                        // Can't bundle the comments because they touch nodes at either side
                        (_, Ordering::Equal, _)
                        // ```
                        // { /* comment */ }
                        // ```
                        //
                        // Can't bundle "up" _or_ "down", because the nodes just can't be bundled into
                        | (false, _, false)
                        // ```
                        // , // comment
                        // node
                        // ```
                        //
                        // Can't bundle "up" because `prev non-comment` can't be bundled into,
                        // but can't bundle "down" because `node` is further away than `prev non-comment`
                        | (false, Ordering::Less, true)
                        // ```
                        // prev non-comment
                        // /* comment */ ,
                        // ```
                        //
                        // Can't bundle "down" because `node` can't be bundled into,
                        // but can't bundle "up" because `prev non-comment` is further away than `node`
                        | (true, Ordering::Greater, false)
                        => {
                            // finalize `prev non-comment`, and all the `comments`, separately
                            new_children.push(non_comment);
                            new_children.extend(comments);

                            // Make `node` the `prev non-comment` for the next nodes to look at
                            state = BundlingState::SingleNonComment(node);
                        }
                        (_, Ordering::Greater, true) => {
                            // ```
                            // prev non-comment
                            //
                            // // ..earlier comments
                            // // last comment
                            // node
                            // ```

                            // Finalize `prev non-comment`
                            new_children.push(non_comment);
                            // Bundle `comments` into `node`, and make that the new `prev non-comment` for the next nodes to look at
                            state =
                                BundlingState::SingleNonComment(Self::bundle_comments_from_above(
                                    node,
                                    comments,
                                    global_source,
                                    arena,
                                    next_node_id,
                                ));
                        }
                    }
                }
            }
        }

        // Flush the remaining nodes from `non_comment` and `comments`
        match state {
            BundlingState::Start => {}
            BundlingState::SingleNonComment(non_comment) => {
                new_children.push(non_comment);
            }
            BundlingState::CollectingComments(comments) => {
                new_children.extend(comments);
            }
            BundlingState::BundlingCommentsFromBelow {
                non_comment,
                distance: _,
                comments,
            } => {
                // ```
                // non-comment
                // // ..further comments
                // // last comment
                // ```

                // Bundle `comments` into `prev non-comment`, and finalize that
                new_children.push(Self::bundle_comments_from_below(
                    non_comment,
                    comments,
                    global_source,
                    arena,
                    next_node_id,
                ));
            }
        }
        new_children
    }

    /// Turns this:
    /// ```txt
    /// parent
    /// - // first comment
    /// - // ..further comments
    /// - node
    ///   - child 1
    ///   - child 2
    /// ```
    /// into this:
    /// ```txt
    /// parent
    /// - node
    ///   - // first comment
    ///   - // ..further comments
    ///   - child 1
    ///   - child 2
    /// ```
    fn bundle_comments_from_above(
        node: &'a Self,
        comments: NEVec<&'a Self>,
        global_source: &'a str,
        arena: &'a Arena<Self>,
        next_node_id: &mut usize,
    ) -> &'a Self {
        let bundled_byte_range = comments.first().byte_range.start..node.byte_range.end;
        let bundled_children = (comments.into_iter())
            .chain(node.children.iter().copied())
            .collect();

        Self::internal_finalize_bundled(
            node,
            bundled_byte_range,
            bundled_children,
            global_source,
            arena,
            next_node_id,
        )
    }

    /// Turns this:
    /// ```txt
    /// parent
    /// - node
    ///   - child 1
    ///   - child 2
    /// - // ..earlier comments
    /// - // last comment
    /// ```
    /// into this:
    /// ```txt
    /// parent
    /// - node
    ///   - child 1
    ///   - child 2
    ///   - // ..earlier comments
    ///   - // last comment
    /// ```
    fn bundle_comments_from_below(
        node: &'a Self,
        comments: NEVec<&'a Self>,
        global_source: &'a str,
        arena: &'a Arena<Self>,
        next_node_id: &mut usize,
    ) -> &'a Self {
        let bundled_byte_range = node.byte_range.start..comments.last().byte_range.end;
        let bundled_children = node.children.iter().copied().chain(comments).collect();

        Self::internal_finalize_bundled(
            node,
            bundled_byte_range,
            bundled_children,
            global_source,
            arena,
            next_node_id,
        )
    }

    /// A convenience method to the finalize the creation of a bundled node.
    ///
    /// Delegates to [`Self::internal_finalize`]
    fn internal_finalize_bundled(
        bundled: &'a Self,
        bundled_new_byte_range: std::ops::Range<usize>,
        bundled_new_children: Vec<&'a Self>,
        global_source: &'a str,
        arena: &'a Arena<Self>,
        next_node_id: &mut usize,
    ) -> &'a Self {
        Self::internal_finalize(
            bundled.lang_profile,
            arena,
            next_node_id,
            bundled.field_name,
            bundled.is_extra,
            bundled_new_children,
            &global_source[bundled_new_byte_range.clone()],
            bundled_new_byte_range,
            bundled.kind,
            bundled.commutative_parent,
        )
    }

    /// Whether this is a node that we can bundled into other nodes
    ///
    /// This is mostly `true` for comments/attributes
    fn can_be_bundled(&self) -> bool {
        self.is_extra || self.lang_profile.comment_nodes.contains(&self.kind)
    }

    /// Whether we want to allow bundling things into `self`
    ///
    /// This is mostly `false` for "small" things: separators, delimiters, leaf nodes
    fn can_be_bundled_into(&self, commutative_parent: &CommutativeParent) -> bool {
        // Don't bundle into leaves
        !self.is_leaf()
            // Don't bundle into separators
            //
            // We know that `self` can't commute with anything that can be bundled into it
            // (comments), since comments are never commutative children -- hence
            // `default_separator` and not `child_separator`
            && self.source.trim() != commutative_parent.default_separator()
    }

    /// Whether `self` is close enough to `other` to:
    /// - put them in the same comment block, if they are both comments
    /// - bundle them together, if one is a comment and one is a non-comment
    fn is_close_enough_to(&self, other: &Self, global_source: &str) -> bool {
        self.distance_to(other, global_source) <= 1
    }

    /// The number of newlines separating `self` from `other`
    fn distance_to(&self, other: &Self, global_source: &str) -> usize {
        let first_end = self.byte_range.end;
        let second_start = other.byte_range.start;
        // Checking `.lines().count()` doesn't work here, because in a case like:
        // ```
        // // first line\n  // second line
        //              ^^^^ the `source` between nodes
        // ```
        // will actually contain 2 lines:
        // - "", between `// first line` and `\n`
        // - " ", between `\n` and `// second line`
        //
        // even though there is clearly only one `\n` between those two comments.
        //
        // So we instead count newlines in `source`, which we know are `\n` since we normalize it
        global_source[first_end..second_start]
            .chars()
            .filter(|c| *c == '\n')
            .count()
    }
}

#[cfg(test)]
mod test {
    use crate::test_utils::ctx;

    #[test]
    fn is_close_enough_to() {
        let ctx = ctx();
        macro_rules! test {
            ($source:literal) => {
                let node = ctx.parse("a.rs", $source);
                let &[first, second] = node.children.as_slice().try_into().unwrap();
                assert!(first.is_close_enough_to(second, $source));
            };
            (not $source:literal) => {
                let node = ctx.parse("a.rs", $source);
                let &[first, second] = node.children.as_slice().try_into().unwrap();
                assert!(!first.is_close_enough_to(second, $source));
            };
        }

        test!("// first comment  \n // second comment");
        // these two don't really work since the nodes get bundled together
        // test!("// comment before \n fn function() {}");
        // test!("fn function() {}  \n // comment after");
        test!("fn function1() {} \n fn function2() {}");
        test!(not "// first line     \n\n // far away");
        // these two don't really work since the nodes get bundled together
        // test!(not "// comment before \n\n fn function() {}");
        // test!(not "fn function() {}  \n\n // comment after");
        test!(not "fn function1() {} \n\n fn function2() {}");
    }

    #[test]
    fn it_works() {
        let ctx = ctx();
        let source = "
// this is a comment
mod foo;
";
        let rs = ctx.parse("a.rs", source);

        let expected = "\
└source_file Commutative
  └mod_item Signature [[foo]]
    ├line_comment // this is a comment
    ├mod
    ├name: identifier foo
    └;
";
        assert_eq!(rs.ascii_tree(Some(3), false), expected);
    }

    #[test]
    fn only_move_one_level_down() {
        let ctx = ctx();
        let source = "
// this is a comment
mod foo {
    mod bar;
}
";
        let rs = ctx.parse("a.rs", source);

        let expected = "\
└source_file Commutative
  └mod_item Signature [[foo]]
    ├line_comment // this is a comment
    ├mod
    ├name: identifier foo
    └body: declaration_list Commutative
";
        assert_eq!(rs.ascii_tree(Some(3), false), expected);
    }

    #[test]
    fn dont_bundle_into_delims() {
        let ctx = ctx();
        let source = "(/* this is a comment */);";
        let rs = ctx.parse("a.rs", source);

        let expected = "\
└source_file Commutative
  └expression_statement
    ├unit_expression
    │ ├(
    │ ├block_comment /* this is a comment */
    │ └)
    └;
";
        assert_eq!(rs.ascii_tree(Some(4), false), expected);
    }

    #[test]
    fn dont_interfere_with_internal_top_comments() {
        let ctx = ctx();
        let source = "
// this is a comment
mod foo {
    // this is inner comment
}
";
        let rs = ctx.parse("a.rs", source);

        let expected = "\
└source_file Commutative
  └mod_item Signature [[foo]]
    ├line_comment // this is a comment
    ├mod
    ├name: identifier foo
    └body: declaration_list Commutative
      ├{
      ├line_comment // this is inner comment
      └}
";
        assert_eq!(rs.ascii_tree(Some(4), false), expected);
    }

    #[test]
    fn multiple_line_comments_are_moved_together() {
        let ctx = ctx();
        let source = "
// line 1
// line 2
// line 3
mod foo;
";
        let rs = ctx.parse("a.rs", source);

        let expected = "\
└source_file Commutative
  └mod_item Signature [[foo]]
    ├line_comment // line 1
    ├line_comment // line 2
    ├line_comment // line 3
    ├mod
    ├name: identifier foo
    └;
";
        assert_eq!(rs.ascii_tree(Some(4), false), expected);
    }

    #[test]
    fn line_and_block_comments_are_moved_together() {
        let ctx = ctx();
        let source = "
// line 1
/* line 2
 * continuation of line 2 */
// line 3
mod foo;
";
        let rs = ctx.parse("a.rs", source);

        let expected = "\
└source_file Commutative
  └mod_item Signature [[foo]]
    ├line_comment // line 1
    ├block_comment
    ├line_comment // line 3
    ├mod
    ├name: identifier foo
    └;
";
        assert_eq!(rs.ascii_tree(Some(4), false), expected);
    }

    // doubles as a test for everything for comments bundled from below
    #[test]
    fn below_after_above() {
        let ctx = ctx();
        let source = "
// line 1 above
// line 2 above
mod foo {
    mod bar;
}
// line 1 below
/* line 2 below
 * continuation of line 2 below */
// line 3 below
";
        let rs = ctx.parse("a.rs", source);

        let expected = "\
└source_file Commutative
  └mod_item Signature [[foo]]
    ├line_comment // line 1 above
    ├line_comment // line 2 above
    ├mod
    ├name: identifier foo
    ├body: declaration_list Commutative
    │ ├{
    │ ├mod_item Signature [[bar]]
    │ └}
    ├line_comment // line 1 below
    ├block_comment
    └line_comment // line 3 below
";
        assert_eq!(rs.ascii_tree(Some(4), false), expected);
    }

    mod affinities {
        use super::*;

        mod node_node {
            use super::*;

            const BUNDLED_INTO_ABOVE: &str = "\
└source_file Commutative
  ├mod_item Signature [[foo]]
  │ ├mod
  │ ├name: identifier foo
  │ ├;
  │ └line_comment // comment below
  └mod_item Signature [[bar]]
    ├mod
    ├name: identifier bar
    └;
";

            #[test]
            // don't bundle -- both too far
            fn node_2_comment_2_node() {
                let ctx = ctx();
                let source = "\
mod foo;

// lonely comment :(

mod bar;";
                let rs = ctx.parse("a.rs", source);

                let expected = "\
└source_file Commutative
  ├mod_item Signature [[foo]]
  ├line_comment // lonely comment :(
  └mod_item Signature [[bar]]
";
                assert_eq!(rs.ascii_tree(Some(2), false), expected);
            }

            #[test]
            fn node_1_comment_2_node() {
                let ctx = ctx();
                let source = "\
mod foo;
// comment below

mod bar;";
                let rs = ctx.parse("a.rs", source);

                assert_eq!(rs.ascii_tree(Some(3), false), BUNDLED_INTO_ABOVE);
            }

            #[test]
            // bundle into below
            fn node_2_comment_1_node() {
                let ctx = ctx();
                let source = "\
mod foo;

// comment above
mod bar;";
                let rs = ctx.parse("a.rs", source);

                let expected = "\
└source_file Commutative
  ├mod_item Signature [[foo]]
  │ ├mod
  │ ├name: identifier foo
  │ └;
  └mod_item Signature [[bar]]
    ├line_comment // comment above
    ├mod
    ├name: identifier bar
    └;
";
                assert_eq!(rs.ascii_tree(Some(4), false), expected);
            }

            #[test]
            // don't bundle -- same distance
            fn node_1_comment_1_node() {
                let ctx = ctx();
                let source = "\
mod foo;
// Buridan's comment
mod bar;";
                let rs = ctx.parse("a.rs", source);

                let expected = "\
└source_file Commutative
  ├mod_item Signature [[foo]]
  ├line_comment // Buridan's comment
  └mod_item Signature [[bar]]
";
                assert_eq!(rs.ascii_tree(Some(2), false), expected);
            }

            #[test]
            fn node_0_comment_1_node() {
                let ctx = ctx();
                let source = "\
mod foo; // comment below
mod bar;";
                let rs = ctx.parse("a.rs", source);

                assert_eq!(rs.ascii_tree(Some(4), false), BUNDLED_INTO_ABOVE);
            }

            #[test]
            // bundle into below
            fn node_1_comment_0_node() {
                let ctx = ctx();
                let source = "\
mod foo;
/* comment above */ mod bar;";
                let rs = ctx.parse("a.rs", source);

                let expected = "\
└source_file Commutative
  ├mod_item Signature [[foo]]
  │ ├mod
  │ ├name: identifier foo
  │ └;
  └mod_item Signature [[bar]]
    ├block_comment /* comment above */
    ├mod
    ├name: identifier bar
    └;
";
                assert_eq!(rs.ascii_tree(Some(4), false), expected);
            }

            #[test]
            // don't bundle
            fn node_0_comment_0_node() {
                let ctx = ctx();
                let source = "\
mod foo; /* Buridan's comment */ mod bar;";
                let rs = ctx.parse("a.rs", source);

                let expected = "\
└source_file Commutative
  ├mod_item Signature [[foo]]
  ├block_comment /* Buridan's comment */
  └mod_item Signature [[bar]]
";
                assert_eq!(rs.ascii_tree(Some(2), false), expected);
            }
        }

        mod sep_node {
            use super::*;

            const UNBUNDLED: &str = "\
└source_file Commutative
  └enum_item Signature [[Foo]]
    ├enum
    ├name: type_identifier Foo
    └body: enum_variant_list Commutative
      ├{
      ├enum_variant Signature [[A]]
      ├,
      ├line_comment // comment
      ├enum_variant Signature [[B]]
      └}
";
            const BUNDLED_INTO_BELOW: &str = "\
└source_file Commutative
  └enum_item Signature [[Foo]]
    ├enum
    ├name: type_identifier Foo
    └body: enum_variant_list Commutative
      ├{
      ├enum_variant Signature [[A]]
      │ └name: identifier A
      ├,
      ├enum_variant Signature [[B]]
      │ ├line_comment // comment
      │ └name: identifier B
      └}
";

            #[test]
            fn sep_2_comment_2_node() {
                let ctx = ctx();
                let source = "\
enum Foo {
    A,

    // comment

    B
}";
                let rs = ctx.parse("a.rs", source);
                assert_eq!(rs.ascii_tree(Some(4), false), UNBUNDLED);
            }

            #[test]
            fn sep_1_comment_2_node() {
                let ctx = ctx();
                let source = "\
enum Foo {
    A,
    // comment

    B
}";
                let rs = ctx.parse("a.rs", source);
                assert_eq!(rs.ascii_tree(Some(4), false), UNBUNDLED);
            }

            #[test]
            fn sep_2_comment_1_node() {
                let ctx = ctx();
                let source = "\
enum Foo {
    A,

    // comment
    B
}";
                let rs = ctx.parse("a.rs", source);
                assert_eq!(rs.ascii_tree(Some(5), false), BUNDLED_INTO_BELOW);
            }

            #[test]
            // don't bundle -- same distance
            fn sep_1_comment_1_node() {
                let ctx = ctx();
                let source = "\
enum Foo {
    A,
    // comment
    B
}";
                let rs = ctx.parse("a.rs", source);
                assert_eq!(rs.ascii_tree(Some(4), false), UNBUNDLED);
            }

            #[test]
            fn sep_0_comment_1_node() {
                let ctx = ctx();
                let source = "\
enum Foo {
    A, // comment
    B
}";
                let rs = ctx.parse("a.rs", source);
                assert_eq!(rs.ascii_tree(Some(4), false), UNBUNDLED);
            }

            #[test]
            // bundle into below
            fn sep_1_comment_0_node() {
                let ctx = ctx();
                let source = "\
enum Foo {
    A,
    /* comment */ B
}";

                let rs = ctx.parse("a.rs", source);

                let expected = "\
└source_file Commutative
  └enum_item Signature [[Foo]]
    ├enum
    ├name: type_identifier Foo
    └body: enum_variant_list Commutative
      ├{
      ├enum_variant Signature [[A]]
      │ └name: identifier A
      ├,
      ├enum_variant Signature [[B]]
      │ ├block_comment /* comment */
      │ └name: identifier B
      └}
";
                assert_eq!(rs.ascii_tree(Some(5), false), expected);
            }

            #[test]
            // don't bundle
            fn sep_0_comment_0_node() {
                let ctx = ctx();
                let source = "\
enum Foo {
    A, /* comment */ B
}";
                let rs = ctx.parse("a.rs", source);

                let expected = "\
└source_file Commutative
  └enum_item Signature [[Foo]]
    ├enum
    ├name: type_identifier Foo
    └body: enum_variant_list Commutative
      ├{
      ├enum_variant Signature [[A]]
      ├,
      ├block_comment /* comment */
      ├enum_variant Signature [[B]]
      └}
";
                assert_eq!(rs.ascii_tree(Some(4), false), expected);
            }
        }

        mod node_sep {
            use super::*;

            const BUNDLED_INTO_ABOVE: &str = "\
└source_file Commutative
  └enum_item Signature [[Foo]]
    ├enum
    ├name: type_identifier Foo
    └body: enum_variant_list Commutative
      ├{
      ├enum_variant Signature [[A]]
      │ ├name: identifier A
      │ └line_comment // comment
      ├,
      ├enum_variant Signature [[B]]
      │ └name: identifier B
      └}
";
            const UNBUNDLED: &str = "\
└source_file Commutative
  └enum_item Signature [[Foo]]
    ├enum
    ├name: type_identifier Foo
    └body: enum_variant_list Commutative
      ├{
      ├enum_variant Signature [[A]]
      ├line_comment // comment
      ├,
      ├enum_variant Signature [[B]]
      └}
";

            #[test]
            fn node_2_comment_2_sep() {
                let ctx = ctx();
                let source = "\
enum Foo {
    A

    // comment

    , B
}";
                let rs = ctx.parse("a.rs", source);
                assert_eq!(rs.ascii_tree(Some(4), false), UNBUNDLED);
            }

            #[test]
            fn node_1_comment_2_sep() {
                let ctx = ctx();
                let source = "\
enum Foo {
    A
    // comment

    , B
}";
                let rs = ctx.parse("a.rs", source);
                assert_eq!(rs.ascii_tree(Some(5), false), BUNDLED_INTO_ABOVE);
            }

            #[test]
            fn node_2_comment_1_sep() {
                let ctx = ctx();
                let source = "\
enum Foo {
    A

    // comment
    , B
}";
                let rs = ctx.parse("a.rs", source);
                assert_eq!(rs.ascii_tree(Some(4), false), UNBUNDLED);
            }

            #[test]
            // don't bundle -- same distance
            fn node_1_comment_1_sep() {
                let ctx = ctx();
                let source = "\
enum Foo {
    A
    // comment
    , B
}";
                let rs = ctx.parse("a.rs", source);
                assert_eq!(rs.ascii_tree(Some(4), false), UNBUNDLED);
            }

            #[test]
            fn node_0_comment_1_sep() {
                let ctx = ctx();
                let source = "\
enum Foo {
    A // comment
    , B
}";
                let rs = ctx.parse("a.rs", source);
                assert_eq!(rs.ascii_tree(Some(5), false), BUNDLED_INTO_ABOVE);
            }

            const UNBUNDLED_SAME_LINE: &str = "\
└source_file Commutative
  └enum_item Signature [[Foo]]
    ├enum
    ├name: type_identifier Foo
    └body: enum_variant_list Commutative
      ├{
      ├enum_variant Signature [[A]]
      ├block_comment /* comment */
      ├,
      ├enum_variant Signature [[B]]
      └}
";
            #[test]
            fn node_1_comment_0_sep() {
                let ctx = ctx();
                let source = "\
enum Foo {
    A
    /* comment */ , B
}";
                let rs = ctx.parse("a.rs", source);
                assert_eq!(rs.ascii_tree(Some(4), false), UNBUNDLED_SAME_LINE);
            }

            #[test]
            // don't bundle -- same distance
            fn node_0_comment_0_sep() {
                let ctx = ctx();
                let source = "\
enum Foo {
    A /* comment */ , B
}";
                let rs = ctx.parse("a.rs", source);
                assert_eq!(rs.ascii_tree(Some(4), false), UNBUNDLED_SAME_LINE);
            }
        }

        mod sep_sep {
            use super::*;

            const UNBUNDLED: &str = "\
└source_file Commutative
  └enum_item Signature [[Foo]]
    ├enum
    ├name: type_identifier Foo
    └body: enum_variant_list Commutative
      ├{
      ├block_comment /* comment */
      └}
";
            #[test]
            fn its_all_the_same() {
                let ctx = ctx();

                let sources = [
                    "\
enum Foo {

    /* comment */

}",
                    "\
enum Foo {
    /* comment */

}",
                    "\
enum Foo {

/* comment */
}",
                    "\
enum Foo {
/* comment */
}",
                    "\
enum Foo { /* comment */
}",
                    "\
enum Foo {
/* comment */ }",
                    "\
enum Foo { /* comment */ }",
                ];

                for source in sources {
                    let rs = ctx.parse("a.rs", source);
                    assert_eq!(rs.ascii_tree(Some(4), false), UNBUNDLED);
                }
            }
        }
    }

    // integration-y tests

    #[test]
    fn comment_node_newline_comment() {
        let ctx = ctx();
        let source = "\
// comment above
mod foo;

// another comment";
        let rs = ctx.parse("a.rs", source);

        let expected = "\
└source_file Commutative
  ├mod_item Signature [[foo]]
  │ ├line_comment // comment above
  │ ├mod
  │ ├name: identifier foo
  │ └;
  └line_comment // another comment
";
        assert_eq!(rs.ascii_tree(Some(3), false), expected);
    }

    #[test]
    fn node_comment_newline_comment() {
        let ctx = ctx();
        let source = "\
mod foo;
// comment below

// another comment";
        let rs = ctx.parse("a.rs", source);

        let expected = "\
└source_file Commutative
  ├mod_item Signature [[foo]]
  │ ├mod
  │ ├name: identifier foo
  │ ├;
  │ └line_comment // comment below
  └line_comment // another comment
";
        assert_eq!(rs.ascii_tree(Some(3), false), expected);
    }

    #[test]
    fn comment_node_comment_newline_comment() {
        let ctx = ctx();
        let source = "\
// comment above
mod foo;
// comment below

// another comment";
        let rs = ctx.parse("a.rs", source);

        let expected = "\
└source_file Commutative
  ├mod_item Signature [[foo]]
  │ ├line_comment // comment above
  │ ├mod
  │ ├name: identifier foo
  │ ├;
  │ └line_comment // comment below
  └line_comment // another comment
";
        assert_eq!(rs.ascii_tree(Some(3), false), expected);
    }

    #[test]
    fn comment_node_comment_newline_node() {
        let ctx = ctx();
        let source = "\
// comment above
mod foo;
// comment below

mod bar;";
        let rs = ctx.parse("a.rs", source);

        let expected = "\
└source_file Commutative
  ├mod_item Signature [[foo]]
  │ ├line_comment // comment above
  │ ├mod
  │ ├name: identifier foo
  │ ├;
  │ └line_comment // comment below
  └mod_item Signature [[bar]]
    ├mod
    ├name: identifier bar
    └;
";
        assert_eq!(rs.ascii_tree(Some(3), false), expected);
    }
}
