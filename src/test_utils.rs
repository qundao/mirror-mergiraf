use std::hash::{DefaultHasher, Hash, Hasher};

use tree_sitter::Parser;
use typed_arena::Arena;

use crate::{
    lang_profile::LangProfile,
    tree::{Ast, AstNode},
};

/// Provides a set of utilities to help write concise tests
pub struct TestContext<'a> {
    pub(crate) arena: Arena<AstNode<'a>>,
}

pub fn ctx<'a>() -> TestContext<'a> {
    TestContext {
        arena: Arena::new(),
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
        Ast::new(tree, source, &lang_profile, &self.arena).expect("syntax error in source")
    }

    pub fn parse_rust(&'a self, source: &'a str) -> Ast<'a> {
        self.parse_internal(".rs", source)
    }

    pub fn parse_json(&'a self, source: &'a str) -> Ast<'a> {
        self.parse_internal(".json", source)
    }

    pub fn parse_java(&'a self, source: &'a str) -> Ast<'a> {
        self.parse_internal(".java", source)
    }

    pub fn parse_go(&'a self, source: &'a str) -> Ast<'a> {
        self.parse_internal(".go", source)
    }

    pub fn parse_yaml(&'a self, source: &'a str) -> Ast<'a> {
        self.parse_internal(".yaml", source)
    }
}

pub fn hash<T: Hash>(node: &T) -> u64 {
    let mut hasher = DefaultHasher::new();
    node.hash(&mut hasher);
    hasher.finish()
}
