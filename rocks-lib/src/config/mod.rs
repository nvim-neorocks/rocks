use directories::ProjectDirs;
use eyre::{eyre, OptionExt as _, Result};
use std::{fmt::Display, path::PathBuf, str::FromStr, time::Duration};

use crate::{project::Project, rockspec::Rockspec};

#[derive(Debug, Clone)]
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

impl FromStr for LuaVersion {
    type Err = String;

    fn from_str(s: &str) -> std::prelude::v1::Result<Self, Self::Err> {
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

impl TryFrom<&Rockspec> for LuaVersion {
    type Error = eyre::Report;

    fn try_from(rockspec: &Rockspec) -> std::result::Result<Self, Self::Error> {
        let lua = rockspec
            .dependencies
            .current_platform()
            .iter()
            .find(|val| *val.name() == "lua".into())
            .ok_or_eyre("no `lua` dependency found!")?;

        for (possibility, version) in [
            ("5.4.0", LuaVersion::Lua54),
            ("5.3.0", LuaVersion::Lua53),
            ("5.2.0", LuaVersion::Lua52),
            ("5.1.0", LuaVersion::Lua51),
        ] {
            if lua.version_req().matches(&possibility.parse().unwrap()) {
                return Ok(version);
            }
        }

        Err(eyre!("no valid matches for `lua` found!"))
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

// TODO: Make all fields private and add getters that return references to the data - they needn't be modified at runtime.
pub struct Config {
    pub enable_development_rockspecs: bool,
    pub server: String,
    pub only_server: Option<String>,
    pub only_sources: Option<String>,
    pub namespace: String,
    pub lua_dir: PathBuf,
    pub lua_version: Option<LuaVersion>,
    pub tree: PathBuf,
    pub no_project: bool,
    pub verbose: bool,
    pub timeout: Duration,
}

impl Config {
    pub fn get_project_dirs() -> Result<ProjectDirs> {
        directories::ProjectDirs::from("org", "neorocks", "rocks")
            .ok_or(eyre!("Could not find a valid home directory"))
    }

    pub fn get_default_cache_path() -> Result<PathBuf> {
        let project_dirs = Config::get_project_dirs()?;
        Ok(project_dirs.cache_dir().to_path_buf())
    }

    pub fn get_default_data_path() -> Result<PathBuf> {
        let project_dirs = Config::get_project_dirs()?;
        Ok(project_dirs.data_local_dir().to_path_buf())
    }
}

impl Config {
    pub fn new() -> Config {
        Config::default()
    }

    pub fn dev(self, dev: bool) -> Config {
        Config {
            enable_development_rockspecs: dev,
            ..self
        }
    }

    pub fn server(self, server: String) -> Config {
        Config { server, ..self }
    }

    pub fn only_server(self, server: String) -> Config {
        Config {
            only_server: Some(server.clone()),
            server,
            ..self
        }
    }

    pub fn only_sources(self, sources: String) -> Config {
        Config {
            only_sources: Some(sources),
            ..self
        }
    }

    pub fn namespace(self, namespace: String) -> Config {
        Config { namespace, ..self }
    }

    pub fn lua_dir(self, lua_dir: PathBuf) -> Config {
        Config { lua_dir, ..self }
    }

    pub fn lua_version(self, lua_version: LuaVersion) -> Config {
        Config {
            lua_version: Some(lua_version),
            ..self
        }
    }

    pub fn tree(self, tree: PathBuf) -> Config {
        Config { tree, ..self }
    }

    pub fn no_project(self, no_project: bool) -> Config {
        Config { no_project, ..self }
    }

    pub fn verbose(self, verbose: bool) -> Config {
        Config { verbose, ..self }
    }

    pub fn timeout(self, timeout: Option<Duration>) -> Config {
        Config {
            timeout: timeout.unwrap_or_else(|| Config::default().timeout),
            ..self
        }
    }
}

impl Default for Config {
    fn default() -> Config {
        // TODO: Remove these unwraps
        let data_path = Config::get_default_data_path().unwrap();
        let current_project = Project::current().unwrap();

        Config {
            enable_development_rockspecs: false,
            server: "https://luarocks.org/".into(),
            only_server: None,
            only_sources: None,
            namespace: "".into(),
            lua_dir: data_path.join("lua"),
            lua_version: current_project
                .as_ref()
                .map(|project| LuaVersion::try_from(project.rockspec()))
                .transpose()
                .unwrap(),
            tree: current_project
                .as_ref()
                .map(|project| project.root().join("tree"))
                .unwrap_or(data_path)
                .to_path_buf(),
            no_project: false,
            verbose: false,
            timeout: Duration::from_secs(30),
        }
    }
}
