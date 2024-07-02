use directories::ProjectDirs;
use eyre::{eyre, Result};
use std::{fmt::Display, path::PathBuf, time::Duration};

#[derive(Clone)]
pub enum LuaVersion {
    Lua51,
    Lua52,
    Lua53,
    Lua54,
    LuaJIT,
    // TODO(vhyrro): Support luau?
    // LuaU,
}

impl Display for LuaVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            LuaVersion::Lua51 | LuaVersion::LuaJIT => "5.1",
            LuaVersion::Lua52 => "5.2",
            LuaVersion::Lua53 => "5.3",
            LuaVersion::Lua54 => "5.4",
        })
    }
}

pub struct Config {
    pub enable_development_rockspecs: bool,
    pub server: String,
    pub only_server: Option<String>,
    pub only_sources: Option<String>,
    pub namespace: String,
    // TODO(vhyrro): Make both of these non-options and autodetect
    // this in Config::default()
    pub lua_dir: Option<PathBuf>,
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

    pub fn server(self, server: Option<String>) -> Config {
        if self.only_server.is_some() {
            self
        } else {
            Config {
                server: server.unwrap_or_else(|| Config::default().server),
                ..self
            }
        }
    }

    pub fn only_server(self, server: Option<String>) -> Config {
        Config {
            only_server: server.clone(),
            server: server.unwrap_or(self.server),
            ..self
        }
    }

    pub fn only_sources(self, sources: Option<String>) -> Config {
        Config {
            only_sources: sources,
            ..self
        }
    }

    pub fn namespace(self, namespace: Option<String>) -> Config {
        Config {
            namespace: namespace.unwrap_or_else(|| Config::default().namespace),
            ..self
        }
    }

    pub fn lua_dir(self, lua_dir: Option<PathBuf>) -> Config {
        Config { lua_dir, ..self }
    }

    pub fn lua_version(self, lua_version: Option<LuaVersion>) -> Config {
        Config {
            lua_version,
            ..self
        }
    }

    pub fn tree(self, tree: Option<PathBuf>) -> Config {
        Config {
            tree: tree.unwrap_or_else(|| Config::default().tree),
            ..self
        }
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
        Config {
            enable_development_rockspecs: false,
            server: "https://luarocks.org/".into(),
            only_server: None,
            only_sources: None,
            namespace: "".into(),
            lua_dir: None,
            lua_version: None,
            tree: Config::get_default_data_path().unwrap(), // TODO: Remove this unwrap
            no_project: false,
            verbose: false,
            timeout: Duration::from_secs(30),
        }
    }
}
