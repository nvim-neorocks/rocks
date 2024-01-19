use std::{path::PathBuf, time::Duration};
use eyre::{eyre, Result};
use directories::ProjectDirs;

pub struct Config {
    pub enable_development_rockspecs: bool,
    pub server: String,
    pub only_server: Option<String>,
    pub only_sources: Option<String>,
    pub namespace: String,
    // TODO(vhyrro): Make both of these non-options and autodetect
    // this in Config::default()
    pub lua_dir: Option<PathBuf>,
    pub lua_version: Option<String>,
    pub tree: PathBuf,
    pub local: bool,
    pub global: bool,
    pub no_project: bool,
    pub verbose: bool,
    pub timeout: Duration,

    // Non-luarocks configs
    pub qualifier: String,
    pub org_name: String,
    pub app_name: String,
}

impl Config {
    pub fn get_project_dirs(&self) -> Result<ProjectDirs> {
        directories::ProjectDirs::from(&self.qualifier, &self.org_name, &self.app_name)
            .ok_or(eyre!("Could not find a valid home directory"))
    }

    pub fn get_default_cache_path(&self) -> Result<PathBuf> {
        let project_dirs = self.get_project_dirs()?;
        Ok(project_dirs.cache_dir().to_path_buf())
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
            server: server.unwrap_or_else(|| self.server),
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

    pub fn lua_version(self, lua_version: Option<String>) -> Config {
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

    pub fn local(self, local: bool) -> Config {
        Config { local, ..self }
    }

    pub fn global(self, global: bool) -> Config {
        Config { global, ..self }
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

    pub fn cache_path(self, cache_path: Option<PathBuf>) -> Config {
        Config {
            cache_path: cache_path.unwrap_or_else(|| Config::default().cache_path),
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
            tree: "/usr".into(),
            local: false,
            global: false,
            no_project: false,
            verbose: false,
            timeout: Duration::from_secs(30),
            qualifier: "org".into(),
            org_name: "neorocks".into(),
            app_name: "rocks".into(),
        }
    }
}
