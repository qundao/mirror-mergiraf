use std::sync::OnceLock;

use tree_sitter::{Language, Query};

/// Bundles the source of a query and a cached compiled version of it.
#[derive(Debug)]
pub struct CompiledQuery<'a> {
    pub source: &'a str,
    compiled: OnceLock<Result<Query, String>>,
}

impl<'a> CompiledQuery<'a> {
    /// Creates a new query, which isn't compiled yet
    pub fn new(source: &'a str) -> Self {
        Self {
            source,
            compiled: OnceLock::new(),
        }
    }

    /// Gets the compiled version of the query (cached)
    pub fn cached_compile(&self, language: &Language) -> Result<&Query, &String> {
        self.compiled
            .get_or_init(|| {
                Query::new(language, self.source)
                    .map_err(|err| format!("Invalid query: \"'{}\": {err}", self.source))
            })
            .as_ref()
    }
}

impl Clone for CompiledQuery<'_> {
    // cloning discards the compiled version of the query
    fn clone(&self) -> Self {
        Self::new(self.source)
    }
}

impl PartialEq for CompiledQuery<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.source == other.source
    }
}
