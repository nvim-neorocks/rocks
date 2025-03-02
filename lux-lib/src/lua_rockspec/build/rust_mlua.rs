use std::{collections::HashMap, path::PathBuf};

use mlua::UserData;

#[derive(Debug, PartialEq, Default, Clone)]
pub struct RustMluaBuildSpec {
    /// Keys are module names in the format normally used by the `require()` function.
    /// values are the library names in the target directory.
    pub modules: HashMap<String, PathBuf>,
    /// Set if the cargo `target` directory is not in the source root.
    pub target_path: PathBuf,
    /// If set to `false` pass `--no-default-features` to cargo.
    pub default_features: bool,
    /// Copy additional files to the `lua` directory.
    /// Keys are the sources, values the destinations (relative to the `lua` directory).
    pub include: HashMap<PathBuf, PathBuf>,
    /// Pass additional features
    pub features: Vec<String>,
}

impl UserData for RustMluaBuildSpec {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("modules", |_, this, _: ()| Ok(this.modules.clone()));
        methods.add_method("target_path", |_, this, _: ()| Ok(this.target_path.clone()));
        methods.add_method("default_features", |_, this, _: ()| {
            Ok(this.default_features)
        });
        methods.add_method("include", |_, this, _: ()| Ok(this.include.clone()));
        methods.add_method("features", |_, this, _: ()| Ok(this.features.clone()));
    }
}
