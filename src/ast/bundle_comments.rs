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
        let source = "(/* this is a comment */)";
        let rs = ctx.parse("a.rs", source);

        let tup = rs[0][0];
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
