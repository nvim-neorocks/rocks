use anyhow::Result;
use mlua::{Lua, LuaSerdeExt};
use std::collections::HashMap;

#[derive(serde::Deserialize)]
pub struct ManifestMetadata {
    pub repository: HashMap<String, HashMap<String, Vec<HashMap<String, String>>>>,
}

impl ManifestMetadata {
    pub fn new(manifest: &String) -> Result<Self> {
        let lua = Lua::new();

        lua.load(manifest).exec()?;

        let manifest = ManifestMetadata {
            repository: lua.from_value(lua.globals().get("repository")?)?,
        };

        Ok(manifest)
    }

    pub fn has_rock(&self, rock_name: &String) -> bool {
        self.repository.contains_key(rock_name)
    }

    pub fn available_versions(&self, rock_name: &String) -> Option<Vec<&String>> {
        if !self.has_rock(rock_name) {
            return None;
        }

        Some(self.repository[rock_name].keys().collect())
    }
}
