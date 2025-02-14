use itertools::Itertools;
use mlua::{FromLua, Lua, LuaSerdeExt, Table, Value};
/// Compatibility layer/adapter for the luarocks client
use std::{collections::HashMap, path::PathBuf};
use thiserror::Error;

use crate::lua_rockspec::{DisplayAsLuaKV, DisplayAsLuaValue, DisplayLuaKV, DisplayLuaValue};

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct RockManifest {
    pub lib: RockManifestLib,
    pub lua: RockManifestLua,
    pub bin: RockManifestBin,
    pub doc: RockManifestDoc,
    pub root: RockManifestRoot,
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

    pub fn to_lua_string(&self) -> String {
        self.display_lua().to_string()
    }
}

impl FromLua for RockManifest {
    fn from_lua(value: Value, lua: &Lua) -> mlua::Result<Self> {
        match &value {
            Value::Table(rock_manifest) => {
                let lib = RockManifestLib {
                    entries: rock_manifest_entry_from_lua(rock_manifest, lua, "lib")?,
                };
                let lua_entry = RockManifestLua {
                    entries: rock_manifest_entry_from_lua(rock_manifest, lua, "lua")?,
                };
                let bin = RockManifestBin {
                    entries: rock_manifest_entry_from_lua(rock_manifest, lua, "bin")?,
                };
                let doc = RockManifestDoc {
                    entries: rock_manifest_entry_from_lua(rock_manifest, lua, "doc")?,
                };
                let mut root_entry = HashMap::new();
                rock_manifest.for_each(|key: String, value: Value| {
                    if let val @ Value::String(_) = value {
                        root_entry.insert(PathBuf::from(key), String::from_lua(val, lua)?);
                    }
                    Ok(())
                })?;
                let root = RockManifestRoot {
                    entries: root_entry,
                };
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

impl DisplayAsLuaKV for RockManifest {
    fn display_lua(&self) -> DisplayLuaKV {
        DisplayLuaKV {
            key: "rock_manifest".to_string(),
            value: DisplayLuaValue::Table(
                vec![
                    self.lua.display_lua(),
                    self.lib.display_lua(),
                    self.doc.display_lua(),
                    self.bin.display_lua(),
                ]
                .into_iter()
                .chain(self.root.entries.iter().map(|entry| entry.display_lua()))
                .collect_vec(),
            ),
        }
    }
}

impl DisplayAsLuaKV for (&PathBuf, &String) {
    fn display_lua(&self) -> DisplayLuaKV {
        DisplayLuaKV {
            key: format!("[\"{}\"]", self.0.display()),
            value: DisplayLuaValue::String(self.1.clone()),
        }
    }
}

impl DisplayAsLuaValue for HashMap<PathBuf, String> {
    fn display_lua_value(&self) -> DisplayLuaValue {
        DisplayLuaValue::Table(self.iter().map(|it| it.display_lua()).collect_vec())
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct RockManifestLua {
    pub entries: HashMap<PathBuf, String>,
}

impl DisplayAsLuaKV for RockManifestLua {
    fn display_lua(&self) -> crate::lua_rockspec::DisplayLuaKV {
        DisplayLuaKV {
            key: "lua".to_string(),
            value: self.entries.display_lua_value(),
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct RockManifestLib {
    pub entries: HashMap<PathBuf, String>,
}

impl DisplayAsLuaKV for RockManifestLib {
    fn display_lua(&self) -> crate::lua_rockspec::DisplayLuaKV {
        DisplayLuaKV {
            key: "lib".to_string(),
            value: self.entries.display_lua_value(),
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct RockManifestBin {
    pub entries: HashMap<PathBuf, String>,
}

impl DisplayAsLuaKV for RockManifestBin {
    fn display_lua(&self) -> crate::lua_rockspec::DisplayLuaKV {
        DisplayLuaKV {
            key: "bin".to_string(),
            value: self.entries.display_lua_value(),
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct RockManifestDoc {
    pub entries: HashMap<PathBuf, String>,
}

impl DisplayAsLuaKV for RockManifestDoc {
    fn display_lua(&self) -> crate::lua_rockspec::DisplayLuaKV {
        DisplayLuaKV {
            key: "doc".to_string(),
            value: self.entries.display_lua_value(),
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct RockManifestRoot {
    pub entries: HashMap<PathBuf, String>,
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
                lib: RockManifestLib {
                    entries: HashMap::from_iter(vec![(
                        "toml_edit.so".into(),
                        "504d63aea7bb341a688ef28f1232fa9b".into()
                    )])
                },
                lua: RockManifestLua::default(),
                bin: RockManifestBin::default(),
                doc: RockManifestDoc {
                    entries: HashMap::from_iter(vec![
                        (
                            "CHANGELOG.md".into(),
                            "adbf3f997070946a5e61955d70bfadb2".into()
                        ),
                        ("LICENSE".into(), "6bcb3636a93bdb8304439a4ff57e979c".into()),
                        (
                            "README.md".into(),
                            "842bd0b364e36d982f02e22abee7742d".into()
                        ),
                    ])
                },
                root: RockManifestRoot {
                    entries: HashMap::from_iter(vec![(
                        "toml-edit-0.6.1-1.rockspec".into(),
                        "fcdd3b0066632dec36cd5510e00bc55e".into()
                    ),])
                },
            }
        );
    }
}
