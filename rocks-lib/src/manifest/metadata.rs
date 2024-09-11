use eyre::Result;
use itertools::Itertools;
use mlua::{Lua, LuaSerdeExt};
use std::collections::HashMap;

use crate::{config::Config, lua_package::PackageName};

#[derive(serde::Deserialize)]
pub struct ManifestMetadata {
    /// The key of each package's HashMap is the version string
    pub repository: HashMap<PackageName, HashMap<String, Vec<ManifestRockEntry>>>,
}

#[derive(serde::Deserialize)]
pub struct ManifestRockEntry {
    /// e.g. "linux-x86_64", "rockspec", "src", ...
    pub arch: String,
}

impl ManifestMetadata {
    // TODO(vhyrro): Perhaps make these functions return a cached, in-memory version of the
    // manifest if it has already been parsed?
    pub fn new(manifest: &String) -> Result<Self> {
        let lua = Lua::new();

        lua.load(manifest).exec()?;

        let manifest = ManifestMetadata {
            repository: lua.from_value(lua.globals().get("repository")?)?,
        };

        Ok(manifest)
    }

    pub async fn from_config(config: &Config) -> Result<Self> {
        let manifest = crate::manifest::manifest_from_server(config.server.clone(), config).await?;

        Self::new(&manifest)
    }

    pub fn has_rock(&self, rock_name: &PackageName) -> bool {
        self.repository.contains_key(rock_name)
    }

    pub fn available_versions(&self, rock_name: &PackageName) -> Option<Vec<&String>> {
        if !self.has_rock(rock_name) {
            return None;
        }

        Some(self.repository[rock_name].keys().collect())
    }

    pub fn latest_version(&self, rock_name: &PackageName) -> Option<&String> {
        if !self.has_rock(rock_name) {
            return None;
        }

        self.repository[rock_name].keys().sorted().last()
    }
}
