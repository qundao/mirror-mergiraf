use std::hash::{Hash, Hasher};
use tree_sitter::Parser;
use typed_arena::Arena;

use crate::{
    ast::{Ast, AstNode},
    lang_profile::LangProfile,
    tree_matcher::TreeMatcher,
};

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
    fn parse_internal(&'a self, extension: &str, source: &'a str) -> Ast<'a> {
        let lang_profile =
            LangProfile::detect_from_filename(extension).expect("could not load language profile");
        let mut parser = Parser::new();
        parser
            .set_language(&lang_profile.language)
            .expect("Error loading language grammar");
        let tree = parser
            .parse(source, None)
            .expect("Parsing example source code failed");
        Ast::new(&tree, source, lang_profile, &self.arena, &self.ref_arena)
            .expect("syntax error in source")
    }

    pub fn parse_rust(&'a self, source: &'a str) -> Ast<'a> {
        self.parse_internal("a.rs", source)
    }

    pub fn parse_json(&'a self, source: &'a str) -> Ast<'a> {
        self.parse_internal("a.json", source)
    }

    pub fn parse_java(&'a self, source: &'a str) -> Ast<'a> {
        self.parse_internal("a.java", source)
    }

    pub fn parse_go(&'a self, source: &'a str) -> Ast<'a> {
        self.parse_internal("a.go", source)
    }

    pub fn parse_yaml(&'a self, source: &'a str) -> Ast<'a> {
        self.parse_internal("a.yaml", source)
    }

    pub fn parse_toml(&'a self, source: &'a str) -> Ast<'a> {
        self.parse_internal("a.toml", source)
    }

    pub fn parse_nix(&'a self, source: &'a str) -> Ast<'a> {
        self.parse_internal("a.nix", source)
    }
}

pub(crate) fn json_matchers() -> (TreeMatcher<'static>, TreeMatcher<'static>) {
    let lang_profile = LangProfile::json();
    let primary_matcher = TreeMatcher {
        min_height: 0,
        sim_threshold: 0.5,
        max_recovery_size: 100,
        use_rted: true,
        lang_profile,
    };
    let auxiliary_matcher = TreeMatcher {
        min_height: 1,
        sim_threshold: 0.5,
        max_recovery_size: 100,
        use_rted: false,
        lang_profile,
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
