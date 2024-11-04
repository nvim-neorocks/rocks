use directories::ProjectDirs;
use std::{collections::HashMap, fmt::Display, io, path::PathBuf, str::FromStr, time::Duration};
use thiserror::Error;

use crate::{
    build::variables::{self, HasVariables},
    package::PackageVersion,
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

impl LuaVersion {
    pub fn as_version(&self) -> PackageVersion {
        match self {
            LuaVersion::Lua51 | LuaVersion::LuaJIT => "5.1.0".parse().unwrap(),
            LuaVersion::Lua52 | LuaVersion::LuaJIT52 => "5.2.0".parse().unwrap(),
            LuaVersion::Lua53 => "5.3.0".parse().unwrap(),
            LuaVersion::Lua54 => "5.4.0".parse().unwrap(),,
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

// TODO: Make all fields private and add getters that return references to the data - they needn't be modified at runtime.
pub struct Config {
    enable_development_rockspecs: bool,
    server: String,
    only_server: Option<String>,
    only_sources: Option<String>,
    namespace: String,
    lua_dir: PathBuf,
    lua_version: Option<LuaVersion>,
    tree: PathBuf,
    no_project: bool,
    verbose: bool,
    timeout: Duration,
    make: String,
    variables: HashMap<String, String>,

    cache_dir: PathBuf,
    data_dir: PathBuf,
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

#[derive(Default)]
pub struct ConfigBuilder {
    enable_development_rockspecs: Option<bool>,
    server: Option<String>,
    only_server: Option<String>,
    only_sources: Option<String>,
    namespace: Option<String>,
    lua_dir: Option<PathBuf>,
    lua_version: Option<LuaVersion>,
    tree: Option<PathBuf>,
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
        let current_project = Project::current()?;

        Ok(Config {
            enable_development_rockspecs: self.enable_development_rockspecs.unwrap_or(false),
            server: self
                .server
                .unwrap_or_else(|| "https://luarocks.org/".to_string()),
            only_server: self.only_server,
            only_sources: self.only_sources,
            namespace: self.namespace.unwrap_or_default(),
            lua_dir: self.lua_dir.unwrap_or_else(|| data_dir.join("lua")),
            lua_version: self.lua_version.or(current_project
                .as_ref()
                .and_then(|project| project.rockspec().lua_version())),
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
                .unwrap_or_else(|| data_dir.clone()),
            no_project: self.no_project.unwrap_or(false),
            verbose: self.verbose.unwrap_or(false),
            timeout: self.timeout.unwrap_or_else(|| Duration::from_secs(30)),
            make: self.make.unwrap_or("make".into()),
            variables: self.variables.unwrap_or_default(),
            cache_dir: self.cache_dir.unwrap_or(Config::get_default_cache_path()?),
            data_dir,
        })
    }
}
