use std::{collections::HashMap, path::PathBuf};

#[derive(Debug, PartialEq, Default, Clone)]
pub struct TreesitterParserBuildSpec {
    /// Name of the parser language, e.g. "haskell"
    pub(crate) lang: String,

    /// Won't build the parser if `false`
    /// (useful for packages that only include queries)
    pub(crate) parser: bool,

    /// Must the sources be generated?
    pub(crate) generate: bool,

    /// tree-sitter grammar's location (relative to the source root)
    pub(crate) location: Option<PathBuf>,

    /// Embedded queries to be installed in the `etc/queries` directory
    pub(crate) queries: HashMap<PathBuf, String>,
}
