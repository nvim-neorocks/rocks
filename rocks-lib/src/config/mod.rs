use directories::ProjectDirs;
use mlua::{ExternalResult, FromLua};
use std::{collections::HashMap, fmt::Display, io, path::PathBuf, str::FromStr, time::Duration};
use thiserror::Error;

use crate::{
    build::variables::{self, HasVariables},
    package::{PackageVersion, PackageVersionReq},
    project::{Project, ProjectError},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LuaVersion {
    Lua51,
    Lua52,
    Lua53,
    Lua54,
    LuaJIT,
    LuaJIT52,
    // TODO(vhyrro): Support luau?
    // LuaU,
}

#[derive(Debug, Error)]
pub enum LuaVersionError {
    #[error("unsupported Lua version: {0}")]
    UnsupportedLuaVersion(PackageVersion),
}

impl LuaVersion {
    pub fn as_version(&self) -> PackageVersion {
        match self {
            LuaVersion::Lua51 => "5.1.0".parse().unwrap(),
            LuaVersion::Lua52 => "5.2.0".parse().unwrap(),
            LuaVersion::Lua53 => "5.3.0".parse().unwrap(),
            LuaVersion::Lua54 => "5.4.0".parse().unwrap(),
            LuaVersion::LuaJIT => "5.1.0".parse().unwrap(),
            LuaVersion::LuaJIT52 => "5.2.0".parse().unwrap(),
        }
    }
    pub fn version_compatibility_str(&self) -> String {
        match self {
            LuaVersion::Lua51 | LuaVersion::LuaJIT => "5.1".into(),
            LuaVersion::Lua52 | LuaVersion::LuaJIT52 => "5.2".into(),
            LuaVersion::Lua53 => "5.3".into(),
            LuaVersion::Lua54 => "5.4".into(),
        }
    }
    pub fn as_version_req(&self) -> PackageVersionReq {
        format!("~> {}", self.version_compatibility_str())
            .parse()
            .unwrap()
    }

    /// Get the LuaVersion from a version that has been parsed from the `lua -v` output
    pub fn from_version(version: PackageVersion) -> Result<LuaVersion, LuaVersionError> {
        // NOTE: Special case. luajit -v outputs 2.x.y as a version
        let luajit_version_req: PackageVersionReq = "~> 2".parse().unwrap();
        if luajit_version_req.matches(&version) {
            Ok(LuaVersion::LuaJIT)
        } else if LuaVersion::Lua51.as_version_req().matches(&version) {
            Ok(LuaVersion::Lua51)
        } else if LuaVersion::Lua52.as_version_req().matches(&version) {
            Ok(LuaVersion::Lua52)
        } else if LuaVersion::Lua53.as_version_req().matches(&version) {
            Ok(LuaVersion::Lua53)
        } else if LuaVersion::Lua54.as_version_req().matches(&version) {
            Ok(LuaVersion::Lua54)
        } else {
            Err(LuaVersionError::UnsupportedLuaVersion(version))
        }
    }
}

pub trait DefaultFromConfig {
    type Err: std::error::Error;

    fn or_default_from(self, config: &Config) -> Result<LuaVersion, Self::Err>;
}

impl DefaultFromConfig for Option<LuaVersion> {
    type Err = LuaVersionUnset;

    fn or_default_from(self, config: &Config) -> Result<LuaVersion, Self::Err> {
        match self {
            Some(value) => Ok(value),
            None => LuaVersion::from(config),
        }
    }
}

#[derive(Error, Debug)]
#[error("lua version not set! Please provide a version through `--lua-version <ver>`")]
pub struct LuaVersionUnset;

impl LuaVersion {
    pub fn from(config: &Config) -> Result<Self, LuaVersionUnset> {
        config.lua_version().ok_or(LuaVersionUnset).cloned()
    }
}

impl FromStr for LuaVersion {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "5.1" | "51" => Ok(LuaVersion::Lua51),
            "5.2" | "52" => Ok(LuaVersion::Lua52),
            "5.3" | "53" => Ok(LuaVersion::Lua53),
            "5.4" | "54" => Ok(LuaVersion::Lua54),
            "jit" | "luajit" => Ok(LuaVersion::LuaJIT),
            "jit52" | "luajit52" => Ok(LuaVersion::LuaJIT52),
            _ => Err(
                "unrecognized Lua version. Allowed versions: '5.1', '5.2', '5.3', '5.4', 'jit', 'jit52'."
                    .into(),
            ),
        }
    }
}

impl Display for LuaVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            LuaVersion::Lua51 => "5.1",
            LuaVersion::Lua52 => "5.2",
            LuaVersion::Lua53 => "5.3",
            LuaVersion::Lua54 => "5.4",
            LuaVersion::LuaJIT => "jit",
            LuaVersion::LuaJIT52 => "jit52",
        })
    }
}

#[derive(Error, Debug)]
#[error("could not find a valid home directory")]
pub struct NoValidHomeDirectory;

#[derive(Clone, Debug, FromLua)]
pub struct Config {
    enable_development_rockspecs: bool,
    server: String,
    only_server: Option<String>,
    only_sources: Option<String>,
    namespace: String,
    lua_dir: PathBuf,
    lua_version: Option<LuaVersion>,
    tree: PathBuf,
    luarocks_tree: PathBuf,
    no_project: bool,
    verbose: bool,
    timeout: Duration,
    make: String,
    variables: HashMap<String, String>,

    cache_dir: PathBuf,
    data_dir: PathBuf,
}

impl mlua::UserData for Config {
    fn add_fields<F: mlua::prelude::LuaUserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("enable_development_rockspecs", |_lua, this| Ok(this.enable_development_rockspecs));
        fields.add_field_method_get("server", |_lua, this| Ok(this.server.clone()));
        fields.add_field_method_get("only_server", |_lua, this| Ok(this.only_server.clone()));
        fields.add_field_method_get("only_sources", |_lua, this| Ok(this.only_sources.clone()));
        fields.add_field_method_get("namespace", |_lua, this| Ok(this.namespace.clone()));
        fields.add_field_method_get("lua_dir", |_lua, this| Ok(this.lua_dir.clone()));
        fields.add_field_method_get("lua_version", |_lua, this| Ok(this.lua_version.clone().map(|v| v.to_string())));
        fields.add_field_method_get("tree", |_lua, this| Ok(this.tree.clone()));
        fields.add_field_method_get("no_project", |_lua, this| Ok(this.no_project));
        fields.add_field_method_get("timeout", |_lua, this| Ok(this.timeout.as_secs()));
        fields.add_field_method_get("make", |_lua, this| Ok(this.make.clone()));
        fields.add_field_method_get("variables", |_lua, this| Ok(this.variables.clone()));
        fields.add_field_method_get("cache_dir", |_lua, this| Ok(this.cache_dir.clone()));
        fields.add_field_method_get("data_dir", |_lua, this| Ok(this.data_dir.clone()));
    }
}

impl Config {
    pub fn get_project_dirs() -> Result<ProjectDirs, NoValidHomeDirectory> {
        directories::ProjectDirs::from("org", "neorocks", "rocks").ok_or(NoValidHomeDirectory)
    }

    pub fn get_default_cache_path() -> Result<PathBuf, NoValidHomeDirectory> {
        let project_dirs = Config::get_project_dirs()?;
        Ok(project_dirs.cache_dir().to_path_buf())
    }

    pub fn get_default_data_path() -> Result<PathBuf, NoValidHomeDirectory> {
        let project_dirs = Config::get_project_dirs()?;
        Ok(project_dirs.data_local_dir().to_path_buf())
    }

    pub fn with_lua_version(self, lua_version: LuaVersion) -> Self {
        Self {
            lua_version: Some(lua_version),
            ..self
        }
    }

    pub fn with_tree(self, tree: PathBuf) -> Self {
        Self { tree, ..self }
    }
}

impl Config {
    pub fn dev(&self) -> bool {
        self.enable_development_rockspecs
    }

    pub fn server(&self) -> &String {
        &self.server
    }

    pub fn only_server(&self) -> Option<&String> {
        self.only_server.as_ref()
    }

    pub fn only_sources(&self) -> Option<&String> {
        self.only_sources.as_ref()
    }

    pub fn namespace(&self) -> &String {
        &self.namespace
    }

    pub fn lua_dir(&self) -> &PathBuf {
        &self.lua_dir
    }

    pub fn lua_version(&self) -> Option<&LuaVersion> {
        self.lua_version.as_ref()
    }

    // TODO(vhyrro): Return `&Tree` instead
    pub fn tree(&self) -> &PathBuf {
        &self.tree
    }

    /// The tree in which to install luarocks for use as a compatibility layer
    pub fn luarocks_tree(&self) -> &PathBuf {
        &self.luarocks_tree
    }

    pub fn no_project(&self) -> bool {
        self.no_project
    }

    pub fn verbose(&self) -> bool {
        self.verbose
    }

    pub fn timeout(&self) -> &Duration {
        &self.timeout
    }

    pub fn make_cmd(&self) -> &String {
        &self.make
    }

    pub fn variables(&self) -> &HashMap<String, String> {
        &self.variables
    }

    pub fn cache_dir(&self) -> &PathBuf {
        &self.cache_dir
    }

    pub fn data_dir(&self) -> &PathBuf {
        &self.data_dir
    }
}

impl HasVariables for Config {
    fn substitute_variables(&self, input: String) -> String {
        variables::substitute(|var_name| self.variables().get(var_name).cloned(), input)
    }
}

#[derive(Error, Debug)]
#[error(transparent)]
pub enum ConfigError {
    Io(#[from] io::Error),
    NoValidHomeDirectory(#[from] NoValidHomeDirectory),
    Project(#[from] ProjectError),
}

#[derive(Clone, Default)]
pub struct ConfigBuilder {
    enable_development_rockspecs: Option<bool>,
    server: Option<String>,
    only_server: Option<String>,
    only_sources: Option<String>,
    namespace: Option<String>,
    lua_dir: Option<PathBuf>,
    lua_version: Option<LuaVersion>,
    tree: Option<PathBuf>,
    luarocks_tree: Option<PathBuf>,
    no_project: Option<bool>,
    verbose: Option<bool>,
    timeout: Option<Duration>,
    make: Option<String>,
    variables: Option<HashMap<String, String>>,

    cache_dir: Option<PathBuf>,
    data_dir: Option<PathBuf>,
}

impl ConfigBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn dev(self, dev: Option<bool>) -> Self {
        Self {
            enable_development_rockspecs: dev,
            ..self
        }
    }

    pub fn server(self, server: Option<String>) -> Self {
        Self { server, ..self }
    }

    pub fn only_server(self, server: Option<String>) -> Self {
        Self {
            only_server: server.clone(),
            server,
            ..self
        }
    }

    pub fn only_sources(self, sources: Option<String>) -> Self {
        Self {
            only_sources: sources,
            ..self
        }
    }

    pub fn namespace(self, namespace: Option<String>) -> Self {
        Self { namespace, ..self }
    }

    pub fn lua_dir(self, lua_dir: Option<PathBuf>) -> Self {
        Self { lua_dir, ..self }
    }

    pub fn lua_version(self, lua_version: Option<LuaVersion>) -> Self {
        Self {
            lua_version,
            ..self
        }
    }

    pub fn tree(self, tree: Option<PathBuf>) -> Self {
        Self { tree, ..self }
    }

    pub fn luarocks_tree(self, luarocks_tree: Option<PathBuf>) -> Self {
        Self {
            luarocks_tree,
            ..self
        }
    }

    pub fn no_project(self, no_project: Option<bool>) -> Self {
        Self { no_project, ..self }
    }

    pub fn verbose(self, verbose: Option<bool>) -> Self {
        Self { verbose, ..self }
    }

    pub fn timeout(self, timeout: Option<Duration>) -> Self {
        Self { timeout, ..self }
    }

    pub fn make_cmd(self, make: Option<String>) -> Self {
        Self { make, ..self }
    }

    pub fn variables(self, variables: Option<HashMap<String, String>>) -> Self {
        Self { variables, ..self }
    }

    pub fn cache_dir(self, cache_dir: Option<PathBuf>) -> Self {
        Self { cache_dir, ..self }
    }

    pub fn data_dir(self, data_dir: Option<PathBuf>) -> Self {
        Self { data_dir, ..self }
    }

    pub fn build(self) -> Result<Config, ConfigError> {
        let data_dir = self.data_dir.unwrap_or(Config::get_default_data_path()?);
        let cache_dir = self.cache_dir.unwrap_or(Config::get_default_cache_path()?);
        let current_project = Project::current()?;
        let lua_version = self
            .lua_version
            .or(current_project
                .as_ref()
                .and_then(|project| project.rockspec().lua_version()))
            .or(crate::lua_installation::get_installed_lua_version("lua")
                .ok()
                .and_then(|version| LuaVersion::from_version(version).ok()));
        Ok(Config {
            enable_development_rockspecs: self.enable_development_rockspecs.unwrap_or(false),
            server: self
                .server
                .unwrap_or_else(|| "https://luarocks.org/".to_string()),
            only_server: self.only_server,
            only_sources: self.only_sources,
            namespace: self.namespace.unwrap_or_default(),
            lua_dir: self.lua_dir.unwrap_or_else(|| data_dir.join("lua")),
            lua_version,
            tree: self
                .tree
                .or_else(|| {
                    if self.no_project.unwrap_or(false) {
                        None
                    } else {
                        current_project
                            .as_ref()
                            .map(|project| project.root().join(".rocks"))
                    }
                })
                .unwrap_or_else(|| data_dir.join("tree")),
            luarocks_tree: self.luarocks_tree.unwrap_or(data_dir.join(".luarocks")),
            no_project: self.no_project.unwrap_or(false),
            verbose: self.verbose.unwrap_or(false),
            timeout: self.timeout.unwrap_or_else(|| Duration::from_secs(30)),
            make: self.make.unwrap_or("make".into()),
            variables: self.variables.unwrap_or_default(),
            cache_dir,
            data_dir,
        })
    }
}

impl mlua::UserData for ConfigBuilder {
    fn add_fields<F: mlua::prelude::LuaUserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_set("enable_development_rockspecs", |_lua, this, enable_development_rockspecs| {
            this.enable_development_rockspecs = enable_development_rockspecs;

            Ok(())
        });
        fields.add_field_method_set("server", |_lua, this, server| {
            this.server = server;

            Ok(())
        });
        fields.add_field_method_set("only_server", |_lua, this, only_server| {
            this.only_server = only_server;

            Ok(())
        });
        fields.add_field_method_set("only_sources", |_lua, this, only_sources| {
            this.only_sources = only_sources;

            Ok(())
        });
        fields.add_field_method_set("namespace", |_lua, this, namespace| {
            this.namespace = namespace;

            Ok(())
        });
        fields.add_field_method_set("lua_dir", |_lua, this, lua_dir| {
            this.lua_dir = lua_dir;

            Ok(())
        });
        fields.add_field_method_set("lua_version", |_lua, this, lua_version: Option<String>| {
            this.lua_version = lua_version.and_then(|v| v.parse().ok());

            Ok(())
        });
        fields.add_field_method_set("tree", |_lua, this, tree| {
            this.tree = tree;

            Ok(())
        });
        fields.add_field_method_set("no_project", |_lua, this, no_project| {
            this.no_project = no_project;

            Ok(())
        });
        fields.add_field_method_set("timeout", |_lua, this, timeout: Option<_>| {
            this.timeout = timeout.map(Duration::from_secs);

            Ok(())
        });
        fields.add_field_method_set("make", |_lua, this, make| {
            this.make = make;

            Ok(())
        });
        fields.add_field_method_set("variables", |_lua, this, variables| {
            this.variables = variables;

            Ok(())
        });
        fields.add_field_method_set("cache_dir", |_lua, this, cache_dir| {
            this.cache_dir = cache_dir;

            Ok(())
        });
        fields.add_field_method_set("data_dir", |_lua, this, data_dir| {
            this.data_dir = data_dir;

            Ok(())
        });
    }

    fn add_methods<M: mlua::prelude::LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("build", |_lua, this, ()| {
            this.clone().build().into_lua_err()
        })
    }
}
