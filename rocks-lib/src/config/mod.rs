use directories::ProjectDirs;
use external_deps::ExternalDependencySearchConfig;
use itertools::Itertools;
use serde::{Deserialize, Serialize, Serializer};
use std::{
    collections::HashMap, env, fmt::Display, io, path::PathBuf, str::FromStr, time::Duration,
};
use thiserror::Error;
use url::Url;

use crate::rockspec::LuaVersionCompatibility;
use crate::{
    build::{
        utils,
        variables::{self, HasVariables},
    },
    package::{PackageVersion, PackageVersionReq},
    project::{Project, ProjectError},
};

pub mod external_deps;

const DEV_PATH: &str = "dev/";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub enum LuaVersion {
    #[serde(rename = "5.1")]
    Lua51,
    #[serde(rename = "5.2")]
    Lua52,
    #[serde(rename = "5.3")]
    Lua53,
    #[serde(rename = "5.4")]
    Lua54,
    #[serde(rename = "jit")]
    LuaJIT,
    #[serde(rename = "jit5.2")]
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

#[derive(Debug, Clone)]
pub struct Config {
    enable_development_packages: bool,
    server: Url,
    extra_servers: Vec<Url>,
    only_sources: Option<String>,
    namespace: String,
    lua_dir: PathBuf,
    lua_version: Option<LuaVersion>,
    tree: PathBuf,
    luarocks_tree: PathBuf,
    no_project: bool,
    verbose: bool,
    timeout: Duration,
    variables: HashMap<String, String>,
    external_deps: ExternalDependencySearchConfig,

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
    pub fn server(&self) -> &Url {
        &self.server
    }

    pub fn extra_servers(&self) -> &Vec<Url> {
        self.extra_servers.as_ref()
    }

    pub fn enabled_dev_servers(&self) -> Result<Vec<Url>, ConfigError> {
        let mut enabled_dev_servers = Vec::new();
        if self.enable_development_packages {
            enabled_dev_servers.push(self.server().join(DEV_PATH)?);
            for server in self.extra_servers() {
                enabled_dev_servers.push(server.join(DEV_PATH)?);
            }
        }
        Ok(enabled_dev_servers)
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

    pub fn make_cmd(&self) -> String {
        match self.variables.get("MAKE") {
            Some(make) => make.clone(),
            None => "make".into(),
        }
    }

    pub fn cmake_cmd(&self) -> String {
        match self.variables.get("CMAKE") {
            Some(cmake) => cmake.clone(),
            None => "cmake".into(),
        }
    }

    pub fn variables(&self) -> &HashMap<String, String> {
        &self.variables
    }

    pub fn external_deps(&self) -> &ExternalDependencySearchConfig {
        &self.external_deps
    }

    pub fn cache_dir(&self) -> &PathBuf {
        &self.cache_dir
    }

    pub fn data_dir(&self) -> &PathBuf {
        &self.data_dir
    }
}

impl HasVariables for Config {
    fn substitute_variables(&self, input: &str) -> String {
        variables::substitute(|var_name| self.variables().get(var_name).cloned(), input)
    }
}

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    NoValidHomeDirectory(#[from] NoValidHomeDirectory),
    #[error(transparent)]
    Project(#[from] ProjectError),
    #[error("error deserializing rocks config: {0}")]
    Deserialize(#[from] toml::de::Error),
    #[error("error parsing URL: {0}")]
    UrlParseError(#[from] url::ParseError),
}

#[derive(Default, Deserialize, Serialize)]
pub struct ConfigBuilder {
    #[serde(
        default,
        deserialize_with = "deserialize_url",
        serialize_with = "serialize_url"
    )]
    server: Option<Url>,
    #[serde(
        default,
        deserialize_with = "deserialize_url_vec",
        serialize_with = "serialize_url_vec"
    )]
    extra_servers: Option<Vec<Url>>,
    only_sources: Option<String>,
    namespace: Option<String>,
    lua_version: Option<LuaVersion>,
    tree: Option<PathBuf>,
    lua_dir: Option<PathBuf>,
    luarocks_tree: Option<PathBuf>,
    cache_dir: Option<PathBuf>,
    data_dir: Option<PathBuf>,
    no_project: Option<bool>,
    enable_development_packages: Option<bool>,
    verbose: Option<bool>,
    timeout: Option<Duration>,
    variables: Option<HashMap<String, String>>,
    #[serde(default)]
    external_deps: ExternalDependencySearchConfig,
}

impl ConfigBuilder {
    /// Create a new `ConfigBuilder` from a config file by deserializing from a config file
    /// if present, or otherwise by instantiating the default config.
    pub fn new() -> Result<Self, ConfigError> {
        let config_file = Self::config_file()?;
        if config_file.is_file() {
            Ok(toml::from_str(&std::fs::read_to_string(&config_file)?)?)
        } else {
            Ok(Self::default())
        }
    }

    /// Get the path to the rocks config file.
    pub fn config_file() -> Result<PathBuf, NoValidHomeDirectory> {
        let project_dirs = directories::ProjectDirs::from("org", "neorocks", "rocks")
            .ok_or(NoValidHomeDirectory)?;
        Ok(project_dirs.config_dir().join("config.toml").to_path_buf())
    }

    pub fn dev(self, dev: Option<bool>) -> Self {
        Self {
            enable_development_packages: dev,
            ..self
        }
    }

    pub fn server(self, server: Option<Url>) -> Self {
        Self { server, ..self }
    }

    pub fn extra_servers(self, extra_servers: Option<Vec<Url>>) -> Self {
        Self {
            extra_servers,
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
                .and_then(|project| project.rocks().lua_version()))
            .or(crate::lua_installation::get_installed_lua_version("lua")
                .ok()
                .and_then(|version| LuaVersion::from_version(version).ok()));
        Ok(Config {
            enable_development_packages: self.enable_development_packages.unwrap_or(false),
            server: self
                .server
                .unwrap_or_else(|| Url::parse("https://luarocks.org/").unwrap()),
            extra_servers: self.extra_servers.unwrap_or_default(),
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
            variables: default_variables()
                .chain(self.variables.unwrap_or_default())
                .collect(),
            external_deps: self.external_deps,
            cache_dir,
            data_dir,
        })
    }
}

/// Useful for printing the current config
impl From<Config> for ConfigBuilder {
    fn from(value: Config) -> Self {
        ConfigBuilder {
            enable_development_packages: Some(value.enable_development_packages),
            server: Some(value.server),
            extra_servers: Some(value.extra_servers),
            only_sources: value.only_sources,
            namespace: Some(value.namespace),
            lua_dir: Some(value.lua_dir),
            lua_version: value.lua_version,
            tree: Some(value.tree),
            luarocks_tree: Some(value.luarocks_tree),
            no_project: Some(value.no_project),
            verbose: Some(value.verbose),
            timeout: Some(value.timeout),
            variables: Some(value.variables),
            cache_dir: Some(value.cache_dir),
            data_dir: Some(value.data_dir),
            external_deps: value.external_deps,
        }
    }
}

fn default_variables() -> impl Iterator<Item = (String, String)> {
    let cflags = env::var("CFLAGS").unwrap_or(utils::default_cflags().into());
    vec![
        ("LUA".into(), "lua".into()),
        ("MAKE".into(), "make".into()),
        ("CMAKE".into(), "cmake".into()),
        ("LIB_EXTENSION".into(), utils::lua_lib_extension().into()),
        ("OBJ_EXTENSION".into(), utils::lua_obj_extension().into()),
        ("CFLAGS".into(), cflags),
        ("LIBFLAG".into(), utils::default_libflag().into()),
    ]
    .into_iter()
}

fn deserialize_url<'de, D>(deserializer: D) -> Result<Option<Url>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = Option::<String>::deserialize(deserializer)?;
    s.map(|s| Url::parse(&s).map_err(serde::de::Error::custom))
        .transpose()
}

fn serialize_url<S>(url: &Option<Url>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match url {
        Some(url) => serializer.serialize_some(url.as_str()),
        None => serializer.serialize_none(),
    }
}

fn deserialize_url_vec<'de, D>(deserializer: D) -> Result<Option<Vec<Url>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = Option::<Vec<String>>::deserialize(deserializer)?;
    s.map(|v| {
        v.into_iter()
            .map(|s| Url::parse(&s).map_err(serde::de::Error::custom))
            .try_collect()
    })
    .transpose()
}

fn serialize_url_vec<S>(urls: &Option<Vec<Url>>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match urls {
        Some(urls) => {
            let url_strings: Vec<String> = urls.iter().map(|url| url.to_string()).collect();
            serializer.serialize_some(&url_strings)
        }
        None => serializer.serialize_none(),
    }
}
