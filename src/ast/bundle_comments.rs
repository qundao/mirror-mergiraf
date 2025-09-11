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
                    if non_comment.can_be_bundled_into(commutative_parent)
                        && let distance = non_comment.distance_to(node, global_source)
                        && distance <= 1
                    {
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
                        //
                        // NOTE: we know that `first_comment` can be bundled with `non-comm`,
                        // otherwise we wouldn't have ended up in this state

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
                        //
                        // NOTE: see above

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
                    if !node.can_be_bundled_into(commutative_parent)
                        || distance_before < distance_after
                    {
                        // ```
                        // prev non-comment
                        // // ..earlier comments
                        // // last comment
                        //
                        // node
                        // ```
                        //
                        // NOTE: we know that the comments could be bundled with `prev non-comment`,
                        // otherwise we would've had finalized it

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
                    } else if distance_before == distance_after {
                        // ```
                        // prev non-comment
                        // // ..earlier comments
                        // // last comment
                        // node
                        // ```
                        //
                        // NOTE: see above

                        // The comments can't be bundled because they touch nodes at either side --
                        // finalize `prev non-comment`, and all the `comments`, separately
                        new_children.push(non_comment);
                        new_children.extend(comments);

                        // Make `node` the `prev non-comment` for the next nodes to look at
                        state = BundlingState::SingleNonComment(node);
                    } else if distance_before > distance_after {
                        // ```
                        // prev non-comment
                        // // ..earlier comments
                        // /* last comment */ node
                        // ```
                        //
                        // NOTE: see above

                        // Finalize `prev non-comment`
                        new_children.push(non_comment);
                        // Bundle `comments` into `node`, and make that the new `prev non-comment` for the next nodes to look at
                        state = BundlingState::SingleNonComment(Self::bundle_comments_from_above(
                            node,
                            comments,
                            global_source,
                            arena,
                            next_node_id,
                        ));
                    } else {
                        unreachable!(
                            "checked all 3 orderings of `distance_before` vs `distance_after`"
                        )
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
        self.is_extra
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
    use super::*;
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

    #[track_caller]
    fn assert_n_children<'a>(node: &'a AstNode<'a>, n: usize) {
        assert_eq!(node.children.len(), n, "\n{}", node.ascii_tree(None));
    }

    #[test]
    fn it_works() {
        let ctx = ctx();
        let source = "
// this is a comment
fn foo() {}
";
        let rs = ctx.parse("a.rs", source);

        assert_n_children(rs, 1);
        let function = rs[0];
        // between the start of the comment, and the end of the function
        assert_eq!(function.source, "// this is a comment\nfn foo() {}");
        assert_n_children(function, 5);
        let &[comment, _fn, _name, _params, _body] = &function.children[..] else {
            unreachable!()
        };
        assert_eq!(comment.kind, "line_comment");
        assert_eq!(comment.source, "// this is a comment");
    }

    #[test]
    fn only_move_one_level_down() {
        let ctx = ctx();
        let source = "
// this is a comment
fn foo() {
    let _ = 0;
}
";
        let rs = ctx.parse("a.rs", source);

        assert_n_children(rs, 1);
        let function = rs[0];
        // between the start of the comment, and the end of the function
        assert_eq!(
            function.source,
            "// this is a comment\nfn foo() {\n    let _ = 0;\n}"
        );
        assert_n_children(function, 5);
        let &[comment, _fn, _name, _params, _body] = &function.children[..] else {
            unreachable!()
        };
        assert_eq!(comment.kind, "line_comment");
        assert_eq!(comment.source, "// this is a comment");
    }

    #[test]
    fn dont_bundle_into_delims() {
        let ctx = ctx();
        let source = "fn test(/* this is a comment */) {}";
        let rs = ctx.parse("a.rs", source);

        let tup = rs[0][2];
        assert_n_children(tup, 3);
        let comment = tup[1];
        assert_eq!(comment.kind, "block_comment");
        assert_eq!(comment.source, "/* this is a comment */");
    }

    #[test]
    fn dont_interfere_with_internal_top_comments() {
        let ctx = ctx();
        let source = "
// this is a comment
fn foo() {
    // this is inner comment
}
";
        let rs = ctx.parse("a.rs", source);

        assert_n_children(rs, 1);
        let function = rs[0];
        // between the start of the comment, and the end of the function
        assert_eq!(
            function.source,
            "// this is a comment\nfn foo() {\n    // this is inner comment\n}"
        );
        assert_n_children(function, 5);
        let &[comment, _fn, _name, _params, body] = &function.children[..] else {
            unreachable!()
        };
        assert_eq!(comment.kind, "line_comment");
        assert_eq!(comment.source, "// this is a comment");
        assert_eq!(body.children.len(), 3, "\n{}", body.ascii_tree(None));
        let [_brace1, inner_comment, _brace2] = &body[..] else {
            unreachable!()
        };
        assert_eq!(inner_comment.kind, "line_comment");
        assert_eq!(inner_comment.source, "// this is inner comment");
    }

    #[test]
    fn multiple_line_comments_are_moved_together() {
        let ctx = ctx();
        let source = "
// line 1
// line 2
// line 3
fn foo() {}
";
        let rs = ctx.parse("a.rs", source);

        assert_n_children(rs, 1);
        let function = rs[0];
        // between the start of the first comment, and the end of the function
        assert_eq!(
            function.source,
            "// line 1\n// line 2\n// line 3\nfn foo() {}"
        );
        assert_n_children(function, 7);
        let &[line1, line2, line3, _paren1, _paren2, _brace1, _brace2] = &function.children[..]
        else {
            unreachable!()
        };
        assert_eq!(line1.kind, "line_comment");
        assert_eq!(line1.source, "// line 1");
        assert_eq!(line2.kind, "line_comment");
        assert_eq!(line2.source, "// line 2");
        assert_eq!(line2.kind, "line_comment");
        assert_eq!(line3.source, "// line 3");
    }

    #[test]
    fn line_and_block_comments_are_moved_together() {
        let ctx = ctx();
        let source = "
// line 1
/* line 2
 * continuation of line 2 */
// line 3
fn foo() {}
";
        let rs = ctx.parse("a.rs", source);

        assert_n_children(rs, 1);
        let function = rs[0];
        // between the start of the first comment, and the end of the function
        assert_eq!(
            function.source,
            "// line 1\n/* line 2\n * continuation of line 2 */\n// line 3\nfn foo() {}"
        );
        assert_n_children(function, 7);
        let &[line1, line2, line3, _paren1, _paren2, _brace1, _brace2] = &function.children[..]
        else {
            unreachable!()
        };
        assert_eq!(line1.kind, "line_comment");
        assert_eq!(line1.source, "// line 1");
        assert_eq!(line2.kind, "block_comment");
        assert_eq!(line2.source, "/* line 2\n * continuation of line 2 */");
        assert_eq!(line3.kind, "line_comment");
        assert_eq!(line3.source, "// line 3");
    }

    // doubles as a test for everything for comments bundled from below
    #[test]
    fn below_after_above() {
        let ctx = ctx();
        let source = "
// line 1 above
// line 2 above
fn foo() {
    let _ = 0;
}
// line 1 below
/* line 2 below
 * continuation of line 2 below */
// line 3 below
";
        let rs = ctx.parse("a.rs", source);

        assert_n_children(rs, 1);
        let function = rs[0];
        // between the start of the first comment, and the end of the last comment
        assert_eq!(
            function.source,
            "// line 1 above\n// line 2 above\nfn foo() {\n    let _ = 0;\n}\n// line 1 below\n/* line 2 below\n * continuation of line 2 below */\n// line 3 below"
        );
        assert_n_children(function, 9);
        let &[
            comment1_above,
            comment2_above,
            _fn,
            _name,
            _params,
            _body,
            comment1_below,
            comment2_below,
            comment3_below,
        ] = &function.children[..]
        else {
            unreachable!()
        };
        assert_eq!(comment1_above.kind, "line_comment");
        assert_eq!(comment1_above.source, "// line 1 above");
        assert_eq!(comment2_above.kind, "line_comment");
        assert_eq!(comment2_above.source, "// line 2 above");

        assert_eq!(comment1_below.kind, "line_comment");
        assert_eq!(comment1_below.source, "// line 1 below");
        assert_eq!(comment2_below.kind, "block_comment");
        #[rustfmt::skip]
        assert_eq!(comment2_below.source, "/* line 2 below\n * continuation of line 2 below */");
        assert_eq!(comment3_below.kind, "line_comment");
        assert_eq!(comment3_below.source, "// line 3 below");
    }

    mod affinities {
        use super::*;

        #[test]
        fn both_too_far() {
            let ctx = ctx();
            let source = "\
fn foo() {}

// lonely comment :(

fn bar() {}";
            let rs = ctx.parse("a.rs", source);

            assert_n_children(rs, 3);
            let &[foo, comment, bar] = &rs.children[..] else {
                unreachable!()
            };
            assert_eq!(foo.kind, "function_item");
            assert_n_children(foo, 4); // regular number of children of a `function_item`
            assert_eq!(comment.kind, "line_comment");
            assert_eq!(bar.kind, "function_item");
            assert_n_children(bar, 4); // regular number of children of a `function_item`
        }

        #[test]
        fn bundle_with_above() {
            let ctx = ctx();
            let source = "\
fn foo() {}
// comment below

fn bar() {}";
            let rs = ctx.parse("a.rs", source);

            assert_n_children(rs, 2);
            let &[foo, bar] = &rs.children[..] else {
                unreachable!()
            };
            assert_n_children(foo, 5);
            let &[_fn, _name, _params, _body, comment] = &foo.children[..] else {
                unreachable!()
            };
            assert_eq!(foo.kind, "function_item");
            assert_eq!(comment.kind, "line_comment");
            assert_eq!(bar.kind, "function_item");
            assert_n_children(bar, 4); // regular number of children of a `function_item`
        }

        #[test]
        fn bundle_with_below() {
            let ctx = ctx();
            let source = "\
fn foo() {}

// comment above
fn bar() {}";

            let rs = ctx.parse("a.rs", source);

            assert_n_children(rs, 2);
            let &[foo, bar] = &rs.children[..] else {
                unreachable!()
            };
            assert_eq!(foo.kind, "function_item");
            assert_n_children(foo, 4); // regular number of children of a `function_item`

            assert_eq!(bar.kind, "function_item");
            assert_n_children(bar, 5);
            let &[comment, _fn, _name, _params, _body] = &bar.children[..] else {
                unreachable!()
            };
            assert_eq!(comment.kind, "line_comment");
        }

        #[test]
        fn both_too_close() {
            let ctx = ctx();
            let source = "\
fn foo() {}
// Buridan's comment
fn bar() {}";

            let rs = ctx.parse("a.rs", source);
            assert_n_children(rs, 3);
            let &[foo, comment, bar] = &rs.children[..] else {
                unreachable!()
            };
            assert_eq!(foo.kind, "function_item");
            assert_n_children(foo, 4); // regular number of children of a `function_item`
            assert_eq!(comment.kind, "line_comment");
            assert_eq!(bar.kind, "function_item");
            assert_n_children(bar, 4); // regular number of children of a `function_item`
        }

        #[test]
        fn bundle_with_above_same_line() {
            let ctx = ctx();
            let source = "\
fn foo() {} // comment below
fn bar() {}";
            let rs = ctx.parse("a.rs", source);

            assert_n_children(rs, 2);
            let &[foo, bar] = &rs.children[..] else {
                unreachable!()
            };
            assert_eq!(foo.kind, "function_item");
            assert_eq!(foo.source, "fn foo() {} // comment below");
            assert_n_children(foo, 5);
            let &[_fn, _name, _params, _body, comment] = &foo.children[..] else {
                unreachable!()
            };
            assert_eq!(comment.kind, "line_comment");
            assert_eq!(bar.kind, "function_item");
            assert_n_children(bar, 4); // regular number of children of a `function_item`
        }

        #[test]
        fn bundle_with_below_same_line() {
            let ctx = ctx();
            let source = "\
fn foo() {}
/* comment above */ fn bar() {}";

            let rs = ctx.parse("a.rs", source);

            assert_n_children(rs, 2);
            let &[foo, bar] = &rs.children[..] else {
                unreachable!()
            };
            assert_eq!(foo.kind, "function_item");
            assert_n_children(foo, 4); // regular number of children of a `function_item`

            assert_eq!(bar.kind, "function_item");
            assert_n_children(bar, 5);
            let &[comment, _fn, _name, _params, _body] = &bar.children[..] else {
                unreachable!()
            };
            assert_eq!(comment.kind, "block_comment");
        }

        #[test]
        fn both_too_close_same_line() {
            let ctx = ctx();
            let source = "\
fn foo() {} /* Buridan's comment */ fn bar() {}";

            let rs = ctx.parse("a.rs", source);
            assert_n_children(rs, 3);
            let &[foo, comment, bar] = &rs.children[..] else {
                unreachable!()
            };
            assert_eq!(foo.kind, "function_item");
            assert_n_children(foo, 4); // regular number of children of a `function_item`
            assert_eq!(comment.kind, "block_comment");
            assert_eq!(bar.kind, "function_item");
            assert_n_children(bar, 4); // regular number of children of a `function_item`
        }
    }

    // integration-y tests

    #[test]
    fn comment_node_newline_comment() {
        let ctx = ctx();
        let source = "\
// comment above
fn foo() {}

// another comment";
        let rs = ctx.parse("a.rs", source);

        assert_n_children(rs, 2);
        let &[foo_w_comment, another_comment] = &rs.children[..] else {
            unreachable!()
        };
        assert_n_children(foo_w_comment, 5);
        let &[comment_above, _fn, _name, _params, _body] = &foo_w_comment[..] else {
            unreachable!()
        };
        assert_eq!(comment_above.kind, "line_comment");
        assert_eq!(comment_above.source, "// comment above");
        assert_eq!(foo_w_comment.kind, "function_item");

        assert_eq!(another_comment.kind, "line_comment");
        assert_eq!(another_comment.source, "// another comment");
    }

    #[test]
    fn node_comment_newline_comment() {
        let ctx = ctx();
        let source = "\
fn foo() {}
// comment below

// another comment";
        let rs = ctx.parse("a.rs", source);

        assert_n_children(rs, 2);
        let &[foo_w_comment, another_comment] = &rs.children[..] else {
            unreachable!()
        };
        assert_n_children(foo_w_comment, 5);
        let &[_fn, _name, _params, _body, comment_below] = &foo_w_comment[..] else {
            unreachable!()
        };
        assert_eq!(foo_w_comment.kind, "function_item");
        assert_eq!(comment_below.kind, "line_comment");
        assert_eq!(comment_below.source, "// comment below");

        assert_eq!(another_comment.kind, "line_comment");
        assert_eq!(another_comment.source, "// another comment");
    }

    #[test]
    fn comment_node_comment_newline_comment() {
        let ctx = ctx();
        let source = "\
// comment above
fn foo() {}
// comment below

// another comment";
        let rs = ctx.parse("a.rs", source);

        assert_n_children(rs, 2);
        let &[foo_w_comments, another_comment] = &rs.children[..] else {
            unreachable!()
        };
        assert_n_children(foo_w_comments, 6);
        let &[comment_above, _fn, _name, _params, _body, comment_below] = &foo_w_comments[..]
        else {
            unreachable!()
        };
        assert_eq!(comment_above.kind, "line_comment");
        assert_eq!(comment_above.source, "// comment above");
        assert_eq!(foo_w_comments.kind, "function_item");
        assert_eq!(comment_below.kind, "line_comment");
        assert_eq!(comment_below.source, "// comment below");

        assert_eq!(another_comment.kind, "line_comment");
        assert_eq!(another_comment.source, "// another comment");
    }

    #[test]
    fn comment_node_comment_newline_node() {
        let ctx = ctx();
        let source = "\
// comment above
fn foo() {}
// comment below

fn bar() {}";
        let rs = ctx.parse("a.rs", source);

        assert_n_children(rs, 2);
        let &[foo_w_comments, bar] = &rs.children[..] else {
            unreachable!()
        };
        assert_n_children(foo_w_comments, 6);
        let &[comment_above, _fn, _name, _params, _body, comment_below] = &foo_w_comments[..]
        else {
            unreachable!()
        };
        assert_eq!(comment_above.kind, "line_comment");
        assert_eq!(comment_above.source, "// comment above");
        assert_eq!(foo_w_comments.kind, "function_item");
        assert_eq!(comment_below.kind, "line_comment");
        assert_eq!(comment_below.source, "// comment below");

        assert_eq!(bar.kind, "function_item");
    }
}
