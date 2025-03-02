use itertools::Itertools;
use mlua::{FromLua, IntoLua, UserData, UserDataFields};
use serde_enum_str::Serialize_enum_str;
use std::{convert::Infallible, path::PathBuf};
use thiserror::Error;

use serde::{Deserialize, Deserializer};

use super::{
    DisplayAsLuaKV, DisplayLuaKV, DisplayLuaValue, FromPlatformOverridable, PartialOverride,
    PerPlatform, PerPlatformWrapper, PlatformOverridable,
};

#[derive(Clone, Debug, PartialEq)]
pub enum TestSpec {
    AutoDetect,
    Busted(BustedTestSpec),
    Command(CommandTestSpec),
    Script(ScriptTestSpec),
}

impl Default for TestSpec {
    fn default() -> Self {
        Self::AutoDetect
    }
}

impl IntoLua for TestSpec {
    fn into_lua(self, lua: &mlua::Lua) -> mlua::Result<mlua::Value> {
        let table = lua.create_table()?;
        match self {
            TestSpec::AutoDetect => table.set("auto_detect", true)?,
            TestSpec::Busted(busted_test_spec) => table.set("busted", busted_test_spec)?,
            TestSpec::Command(command_test_spec) => table.set("command", command_test_spec)?,
            TestSpec::Script(script_test_spec) => table.set("script", script_test_spec)?,
        }
        Ok(mlua::Value::Table(table))
    }
}

#[derive(Error, Debug)]
pub enum TestSpecError {
    #[error("'command' test type must specify 'command' or 'script' field")]
    NoCommandOrScript,
    #[error("'command' test type cannot have both 'command' and 'script' fields")]
    CommandAndScript,
}

impl FromPlatformOverridable<TestSpecInternal, Self> for TestSpec {
    type Err = TestSpecError;

    fn from_platform_overridable(internal: TestSpecInternal) -> Result<Self, Self::Err> {
        let test_spec = match internal.test_type {
            Some(TestType::Busted) => Ok(Self::Busted(BustedTestSpec {
                flags: internal.flags.unwrap_or_default(),
            })),
            Some(TestType::Command) => match (internal.command, internal.script) {
                (None, None) => Err(TestSpecError::NoCommandOrScript),
                (None, Some(script)) => Ok(Self::Script(ScriptTestSpec {
                    script,
                    flags: internal.flags.unwrap_or_default(),
                })),
                (Some(command), None) => Ok(Self::Command(CommandTestSpec {
                    command,
                    flags: internal.flags.unwrap_or_default(),
                })),
                (Some(_), Some(_)) => Err(TestSpecError::CommandAndScript),
            },
            None => Ok(Self::default()),
        }?;
        Ok(test_spec)
    }
}

impl FromLua for PerPlatform<TestSpec> {
    fn from_lua(
        value: mlua::prelude::LuaValue,
        lua: &mlua::prelude::Lua,
    ) -> mlua::prelude::LuaResult<Self> {
        let wrapper = PerPlatformWrapper::from_lua(value, lua)?;
        Ok(wrapper.un_per_platform)
    }
}

impl<'de> Deserialize<'de> for TestSpec {
    fn deserialize<D>(deserializer: D) -> Result<TestSpec, D::Error>
    where
        D: Deserializer<'de>,
    {
        let internal = TestSpecInternal::deserialize(deserializer)?;
        let test_spec =
            TestSpec::from_platform_overridable(internal).map_err(serde::de::Error::custom)?;
        Ok(test_spec)
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct BustedTestSpec {
    flags: Vec<String>,
}

impl UserData for BustedTestSpec {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("flags", |_, this, _: ()| Ok(this.flags.clone()));
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CommandTestSpec {
    command: String,
    flags: Vec<String>,
}

impl UserData for CommandTestSpec {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("command", |_, this, _: ()| Ok(this.command.clone()));
        methods.add_method("flags", |_, this, _: ()| Ok(this.flags.clone()));
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScriptTestSpec {
    script: PathBuf,
    flags: Vec<String>,
}

impl UserData for ScriptTestSpec {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("script", |_, this, _: ()| Ok(this.script.clone()));
        methods.add_method("flags", |_, this, _: ()| Ok(this.flags.clone()));
    }
}

#[derive(Debug, Deserialize, Serialize_enum_str, PartialEq, Clone)]
#[serde(rename_all = "lowercase")]
pub(crate) enum TestType {
    Busted,
    Command,
}

#[derive(Debug, PartialEq, Deserialize, Default, Clone)]
pub(crate) struct TestSpecInternal {
    #[serde(default, rename = "type")]
    pub(crate) test_type: Option<TestType>,
    #[serde(default)]
    pub(crate) flags: Option<Vec<String>>,
    #[serde(default)]
    pub(crate) command: Option<String>,
    #[serde(default)]
    pub(crate) script: Option<PathBuf>,
}

impl PartialOverride for TestSpecInternal {
    type Err = Infallible;

    fn apply_overrides(&self, override_spec: &Self) -> Result<Self, Self::Err> {
        Ok(TestSpecInternal {
            test_type: override_opt(&override_spec.test_type, &self.test_type),
            flags: match (override_spec.flags.clone(), self.flags.clone()) {
                (Some(override_vec), Some(base_vec)) => {
                    let merged: Vec<String> =
                        base_vec.into_iter().chain(override_vec).unique().collect();
                    Some(merged)
                }
                (None, base_vec @ Some(_)) => base_vec,
                (override_vec @ Some(_), None) => override_vec,
                _ => None,
            },
            command: match override_spec.script.clone() {
                Some(_) => None,
                None => override_opt(&override_spec.command, &self.command),
            },
            script: match override_spec.command.clone() {
                Some(_) => None,
                None => override_opt(&override_spec.script, &self.script),
            },
        })
    }
}

impl PlatformOverridable for TestSpecInternal {
    type Err = Infallible;

    fn on_nil<T>() -> Result<PerPlatform<T>, <Self as PlatformOverridable>::Err>
    where
        T: PlatformOverridable,
        T: Default,
    {
        Ok(PerPlatform::default())
    }
}

fn override_opt<T: Clone>(override_opt: &Option<T>, base: &Option<T>) -> Option<T> {
    match override_opt.clone() {
        override_val @ Some(_) => override_val,
        None => base.clone(),
    }
}

impl DisplayAsLuaKV for TestSpecInternal {
    fn display_lua(&self) -> DisplayLuaKV {
        let mut result = Vec::new();

        if let Some(test_type) = &self.test_type {
            result.push(DisplayLuaKV {
                key: "type".to_string(),
                value: DisplayLuaValue::String(test_type.to_string()),
            });
        }
        if let Some(flags) = &self.flags {
            result.push(DisplayLuaKV {
                key: "flags".to_string(),
                value: DisplayLuaValue::List(
                    flags
                        .iter()
                        .map(|flag| DisplayLuaValue::String(flag.clone()))
                        .collect(),
                ),
            });
        }
        if let Some(command) = &self.command {
            result.push(DisplayLuaKV {
                key: "command".to_string(),
                value: DisplayLuaValue::String(command.clone()),
            });
        }
        if let Some(script) = &self.script {
            result.push(DisplayLuaKV {
                key: "script".to_string(),
                value: DisplayLuaValue::String(script.to_string_lossy().to_string()),
            });
        }

        DisplayLuaKV {
            key: "test".to_string(),
            value: DisplayLuaValue::Table(result),
        }
    }
}

#[cfg(test)]
mod tests {

    use mlua::{Error, FromLua, Lua};

    use crate::lua_rockspec::PlatformIdentifier;

    use super::*;

    #[tokio::test]
    pub async fn test_spec_from_lua() {
        let lua_content = "
        test = {\n
        }\n
        ";
        let lua = Lua::new();
        lua.load(lua_content).exec().unwrap();
        let test_spec = PerPlatform::from_lua(lua.globals().get("test").unwrap(), &lua).unwrap();
        assert!(matches!(test_spec.default, TestSpec::AutoDetect { .. }));
        let lua_content = "
        test = {\n
            type = 'busted',\n
        }\n
        ";
        let lua = Lua::new();
        lua.load(lua_content).exec().unwrap();
        let test_spec: PerPlatform<TestSpec> =
            PerPlatform::from_lua(lua.globals().get("test").unwrap(), &lua).unwrap();
        assert_eq!(
            test_spec.default,
            TestSpec::Busted(BustedTestSpec::default())
        );
        let lua_content = "
        test = {\n
            type = 'busted',\n
            flags = { 'foo', 'bar' },\n
        }\n
        ";
        let lua = Lua::new();
        lua.load(lua_content).exec().unwrap();
        let test_spec: PerPlatform<TestSpec> =
            PerPlatform::from_lua(lua.globals().get("test").unwrap(), &lua).unwrap();
        assert_eq!(
            test_spec.default,
            TestSpec::Busted(BustedTestSpec {
                flags: vec!["foo".into(), "bar".into()],
            })
        );
        let lua_content = "
        test = {\n
            type = 'command',\n
        }\n
        ";
        let lua = Lua::new();
        lua.load(lua_content).exec().unwrap();
        let result: Result<PerPlatform<TestSpec>, Error> =
            PerPlatform::from_lua(lua.globals().get("test").unwrap(), &lua);
        let _err = result.unwrap_err();
        let lua_content = "
        test = {\n
            type = 'command',\n
            command = 'foo',\n
            script = 'bar',\n
        }\n
        ";
        let lua = Lua::new();
        lua.load(lua_content).exec().unwrap();
        let result: Result<PerPlatform<TestSpec>, Error> =
            PerPlatform::from_lua(lua.globals().get("test").unwrap(), &lua);
        let _err = result.unwrap_err();
        let lua_content = "
        test = {\n
            type = 'command',\n
            command = 'baz',\n
            flags = { 'foo', 'bar' },\n
        }\n
        ";
        let lua = Lua::new();
        lua.load(lua_content).exec().unwrap();
        let test_spec: PerPlatform<TestSpec> =
            PerPlatform::from_lua(lua.globals().get("test").unwrap(), &lua).unwrap();
        assert_eq!(
            test_spec.default,
            TestSpec::Command(CommandTestSpec {
                command: "baz".into(),
                flags: vec!["foo".into(), "bar".into()],
            })
        );
        let lua_content = "
        test = {\n
            type = 'command',\n
            script = 'test.lua',\n
            flags = { 'foo', 'bar' },\n
        }\n
        ";
        let lua = Lua::new();
        lua.load(lua_content).exec().unwrap();
        let test_spec: PerPlatform<TestSpec> =
            PerPlatform::from_lua(lua.globals().get("test").unwrap(), &lua).unwrap();
        assert_eq!(
            test_spec.default,
            TestSpec::Script(ScriptTestSpec {
                script: PathBuf::from("test.lua"),
                flags: vec!["foo".into(), "bar".into()],
            })
        );
        let lua_content = "
        test = {\n
            type = 'command',\n
            command = 'baz',\n
            flags = { 'foo', 'bar' },\n
            platforms = {\n
                unix = { flags = { 'baz' }, },\n
                macosx = {\n
                    script = 'bat.lua',\n
                    flags = { 'bat' },\n
                },\n
                linux = { type = 'busted' },\n
            },\n
        }\n
        ";
        let lua = Lua::new();
        lua.load(lua_content).exec().unwrap();
        let test_spec: PerPlatform<TestSpec> =
            PerPlatform::from_lua(lua.globals().get("test").unwrap(), &lua).unwrap();
        assert_eq!(
            test_spec.default,
            TestSpec::Command(CommandTestSpec {
                command: "baz".into(),
                flags: vec!["foo".into(), "bar".into()],
            })
        );
        let unix = test_spec
            .per_platform
            .get(&PlatformIdentifier::Unix)
            .unwrap();
        assert_eq!(
            *unix,
            TestSpec::Command(CommandTestSpec {
                command: "baz".into(),
                flags: vec!["foo".into(), "bar".into(), "baz".into()],
            })
        );
        let macosx = test_spec
            .per_platform
            .get(&PlatformIdentifier::MacOSX)
            .unwrap();
        assert_eq!(
            *macosx,
            TestSpec::Script(ScriptTestSpec {
                script: "bat.lua".into(),
                flags: vec!["foo".into(), "bar".into(), "bat".into(), "baz".into()],
            })
        );
        let linux = test_spec
            .per_platform
            .get(&PlatformIdentifier::Linux)
            .unwrap();
        assert_eq!(
            *linux,
            TestSpec::Busted(BustedTestSpec {
                flags: vec!["foo".into(), "bar".into(), "baz".into()],
            })
        );
    }
}
