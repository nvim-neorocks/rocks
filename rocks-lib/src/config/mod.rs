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
    pub lua_dir: Option<String>,
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
