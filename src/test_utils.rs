use std::hash::{Hash, Hasher};
use typed_arena::Arena;

use crate::{ast::AstNode, lang_profile::LangProfile, tree_matcher::TreeMatcher};

/// Provides a set of utilities to help write concise tests
pub struct TestContext<'a> {
    pub(crate) arena: Arena<AstNode<'a>>,
    pub(crate) ref_arena: Arena<&'a AstNode<'a>>,
}

pub fn ctx<'a>() -> TestContext<'a> {
    TestContext {
        arena: Arena::new(),
        ref_arena: Arena::new(),
    }
}

impl<'a> TestContext<'a> {
    pub fn parse(&'a self, filename: &str, source: &'a str) -> &'a AstNode<'a> {
        let lang_profile =
            LangProfile::detect_from_filename(filename).expect("could not load language profile");
        AstNode::parse(source, lang_profile, &self.arena, &self.ref_arena)
            .expect("syntax error in source")
    }
}

pub(crate) fn json_matchers() -> (TreeMatcher, TreeMatcher) {
    let primary_matcher = TreeMatcher {
        min_height: 0,
        sim_threshold: 0.5,
        max_recovery_size: 100,
        use_rted: true,
    };
    let auxiliary_matcher = TreeMatcher {
        min_height: 1,
        sim_threshold: 0.5,
        max_recovery_size: 100,
        use_rted: false,
    };
    (primary_matcher, auxiliary_matcher)
}

impl LangProfile {
    pub fn rust() -> &'static Self {
        Self::detect_from_filename("a.rs").unwrap()
    }
    pub fn json() -> &'static Self {
        Self::detect_from_filename("a.json").unwrap()
    }
    pub fn java() -> &'static Self {
        Self::detect_from_filename("a.java").unwrap()
    }
    pub fn go() -> &'static Self {
        Self::detect_from_filename("a.go").unwrap()
    }
}

pub fn hash<T: Hash>(node: &T) -> u64 {
    let mut hasher = crate::fxhasher();
    node.hash(&mut hasher);
    hasher.finish()
}
