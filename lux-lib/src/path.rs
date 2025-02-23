use itertools::Itertools;
use serde::Serialize;
use std::{env, fmt::Display, io, path::PathBuf, str::FromStr};

use crate::{
    build::utils::lua_lib_extension,
    config::{Config, LuaVersion},
    tree::Tree,
};

const LUA_PATH_SEPARATOR: &str = ";";
const LUA_INIT: &str = "require('lux').loader()";

#[derive(PartialEq, Eq, Debug, Serialize)]
pub struct Paths {
    /// Paths for Lua libraries
    src: PackagePath,
    /// Paths for native Lua libraries
    lib: PackagePath,
    /// Paths for executables
    bin: BinPath,

    version: LuaVersion,
}

impl Paths {
    fn default(tree: &Tree) -> Self {
        Self {
            src: <_>::default(),
            lib: <_>::default(),
            bin: <_>::default(),
            version: tree.version().clone(),
        }
    }

    pub fn new(tree: Tree) -> io::Result<Self> {
        let mut paths = tree
            .list()?
            .into_iter()
            .flat_map(|(_, packages)| {
                packages
                    .into_iter()
                    .map(|package| tree.rock_layout(&package))
                    .collect_vec()
            })
            .fold(Self::default(&tree), |mut paths, package| {
                paths.src.0.push(package.src.join("?.lua"));
                paths.src.0.push(package.src.join("?").join("init.lua"));
                paths
                    .lib
                    .0
                    .push(package.lib.join(format!("?.{}", lua_lib_extension())));
                paths.bin.0.push(package.bin);
                paths
            });

        let lib_path = option_env!("LUX_LIB_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| Config::get_default_data_path().unwrap())
            .join(tree.version().to_string())
            .join("?.so");

        paths.prepend(&Paths {
            version: tree.version().clone(),
            src: <_>::default(),
            bin: <_>::default(),
            lib: PackagePath(vec![lib_path]),
        });

        Ok(paths)
    }

    /// Get the `package.path`
    pub fn package_path(&self) -> &PackagePath {
        &self.src
    }

    /// Get the `package.cpath`
    pub fn package_cpath(&self) -> &PackagePath {
        &self.lib
    }

    /// Get the `$PATH`
    pub fn path(&self) -> &BinPath {
        &self.bin
    }

    /// Get `$LUA_INIT`
    pub fn init(&self) -> String {
        format!("if _VERSION:find('{}') then {LUA_INIT} end", self.version)
    }

    /// Get the `$PATH`, prepended to the existing `$PATH` environment.
    pub fn path_prepended(&self) -> BinPath {
        let mut path = BinPath::from_env();
        path.prepend(self.path());
        path
    }

    pub fn prepend(&mut self, other: &Self) {
        self.src.prepend(&other.src);
        self.lib.prepend(&other.lib);
        self.bin.prepend(&other.bin);
    }
}

#[derive(PartialEq, Eq, Debug, Default, Serialize)]
pub struct PackagePath(Vec<PathBuf>);

impl PackagePath {
    pub fn prepend(&mut self, other: &Self) {
        let mut new_vec = other.0.to_owned();
        new_vec.append(&mut self.0);
        self.0 = new_vec;
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn joined(&self) -> String {
        self.0
            .iter()
            .unique()
            .map(|path| path.to_string_lossy())
            .join(LUA_PATH_SEPARATOR)
    }
}

impl FromStr for PackagePath {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let paths = s
            .trim_start_matches(LUA_PATH_SEPARATOR)
            .trim_end_matches(LUA_PATH_SEPARATOR)
            .split(LUA_PATH_SEPARATOR)
            .map(PathBuf::from)
            .collect();
        Ok(PackagePath(paths))
    }
}

impl Display for PackagePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.joined().fmt(f)
    }
}

#[derive(PartialEq, Eq, Debug, Default, Serialize)]
pub struct BinPath(Vec<PathBuf>);

impl BinPath {
    pub fn from_env() -> Self {
        Self::from_str(env::var("PATH").unwrap_or_default().as_str()).unwrap_or_default()
    }
    pub fn prepend(&mut self, other: &Self) {
        let mut new_vec = other.0.to_owned();
        new_vec.append(&mut self.0);
        self.0 = new_vec;
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn joined(&self) -> String {
        env::join_paths(self.0.iter().unique())
            .expect("Failed to join bin paths.")
            .to_string_lossy()
            .to_string()
    }
}

impl FromStr for BinPath {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let paths = env::split_paths(s).collect();
        Ok(BinPath(paths))
    }
}

impl Display for BinPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.joined().fmt(f)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn package_path_leading_trailing_delimiters() {
        let path = PackagePath::from_str(
            ";;/path/to/some/lib/lua/5.1/?.so;/path/to/another/lib/lua/5.1/?.so;;;",
        )
        .unwrap();
        assert_eq!(
            path,
            PackagePath(vec![
                "/path/to/some/lib/lua/5.1/?.so".into(),
                "/path/to/another/lib/lua/5.1/?.so".into(),
            ])
        );
        assert_eq!(
            format!("{}", path),
            "/path/to/some/lib/lua/5.1/?.so;/path/to/another/lib/lua/5.1/?.so"
        );
    }
}
