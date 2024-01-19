use std::{path::PathBuf, time::Duration};

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

    // Rocks-specific options (unavailable in luarocks)
    pub cache_path: PathBuf,
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

            cache_path: directories::ProjectDirs::from("org", "neorocks", "rocks")
                .unwrap()
                .cache_dir()
                .to_path_buf(),
        }
    }
}
