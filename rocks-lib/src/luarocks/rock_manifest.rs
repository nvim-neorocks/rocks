use mlua::{FromLua, Lua, LuaSerdeExt as _, Table, Value};
/// Compatibility layer/adapter for the luarocks client
use std::{collections::HashMap, path::PathBuf};
use thiserror::Error;

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct RockManifest {
    pub lib: HashMap<PathBuf, String>,
    pub lua: HashMap<PathBuf, String>,
    pub bin: HashMap<PathBuf, String>,
    pub doc: HashMap<PathBuf, String>,
    pub root: HashMap<PathBuf, String>,
}

#[derive(Error, Debug)]
pub enum RockManifestError {
    #[error("could not parse rock_manifest: {0}")]
    MLua(#[from] mlua::Error),
}

impl RockManifest {
    pub fn new(rock_manifest_content: &str) -> Result<Self, RockManifestError> {
        let lua = Lua::new();
        lua.load(rock_manifest_content).exec()?;
        let globals = lua.globals();
        let value = globals.get("rock_manifest")?;
        Ok(Self::from_lua(value, &lua)?)
    }
}

impl FromLua for RockManifest {
    fn from_lua(value: Value, lua: &Lua) -> mlua::Result<Self> {
        match &value {
            Value::Table(rock_manifest) => {
                let lib = rock_manifest_entry_from_lua(rock_manifest, lua, "lib")?;
                let lua_entry = rock_manifest_entry_from_lua(rock_manifest, lua, "lua")?;
                let bin = rock_manifest_entry_from_lua(rock_manifest, lua, "bin")?;
                let doc = rock_manifest_entry_from_lua(rock_manifest, lua, "doc")?;
                let mut root = HashMap::new();
                rock_manifest.for_each(|key: String, value: Value| {
                    if let val @ Value::String(_) = value {
                        root.insert(PathBuf::from(key), String::from_lua(val, lua)?);
                    }
                    Ok(())
                })?;
                Ok(Self {
                    lib,
                    lua: lua_entry,
                    bin,
                    doc,
                    root,
                })
            }
            Value::Nil => Ok(Self::default()),
            val => Err(mlua::Error::DeserializeError(format!(
                "Expected rock_manifest to be a table or nil, but got {}",
                val.type_name()
            ))),
        }
    }
}

fn rock_manifest_entry_from_lua(
    rock_manifest: &Table,
    lua: &Lua,
    key: &str,
) -> mlua::Result<HashMap<PathBuf, String>> {
    if rock_manifest.contains_key(key)? {
        lua.from_value(rock_manifest.get(key)?)
    } else {
        Ok(HashMap::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    pub async fn rock_manifest_from_lua() {
        let rock_manifest_content = "
rock_manifest = {
   doc = {
      ['CHANGELOG.md'] = 'adbf3f997070946a5e61955d70bfadb2',
      LICENSE = '6bcb3636a93bdb8304439a4ff57e979c',
      ['README.md'] = '842bd0b364e36d982f02e22abee7742d'
   },
   lib = {
      ['toml_edit.so'] = '504d63aea7bb341a688ef28f1232fa9b'
   },
   ['toml-edit-0.6.1-1.rockspec'] = 'fcdd3b0066632dec36cd5510e00bc55e'
}
        ";
        let rock_manifest = RockManifest::new(rock_manifest_content).unwrap();
        assert_eq!(
            rock_manifest,
            RockManifest {
                lib: HashMap::from_iter(vec![(
                    "toml_edit.so".into(),
                    "504d63aea7bb341a688ef28f1232fa9b".into()
                )]),
                lua: HashMap::default(),
                bin: HashMap::default(),
                doc: HashMap::from_iter(vec![
                    (
                        "CHANGELOG.md".into(),
                        "adbf3f997070946a5e61955d70bfadb2".into()
                    ),
                    ("LICENSE".into(), "6bcb3636a93bdb8304439a4ff57e979c".into()),
                    (
                        "README.md".into(),
                        "842bd0b364e36d982f02e22abee7742d".into()
                    ),
                ]),
                root: HashMap::from_iter(vec![(
                    "toml-edit-0.6.1-1.rockspec".into(),
                    "fcdd3b0066632dec36cd5510e00bc55e".into()
                ),]),
            }
        );
    }
}
