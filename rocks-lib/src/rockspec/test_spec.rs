use eyre::{eyre, Result};
use itertools::Itertools;
use mlua::{FromLua, Lua, Value};
use std::{collections::HashMap, path::PathBuf};

use serde::Deserialize;

use super::{PartialOverride, PerPlatform, PlatformOverridable};

#[derive(Debug, PartialEq)]
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

impl TestSpec {
    fn from_internal_spec(internal: TestSpecInternal) -> Result<Self> {
        let test_spec = match internal.test_type {
            Some(TestType::Busted) => Ok(Self::Busted(BustedTestSpec {
                flags: internal.flags.unwrap_or_default(),
            })),
            Some(TestType::Command) => match (internal.command, internal.script) {
                (None, None) => Err(eyre!(
                    "'command' test type must specify 'command' or 'script' field"
                )),
                (None, Some(script)) => Ok(Self::Script(ScriptTestSpec {
                    script,
                    flags: internal.flags.unwrap_or_default(),
                })),
                (Some(command), None) => Ok(Self::Command(CommandTestSpec {
                    command,
                    flags: internal.flags.unwrap_or_default(),
                })),
                (Some(_), Some(_)) => Err(eyre!(
                    "'command' test type cannot have both 'command' and 'script' fields"
                )),
            },
            None => Ok(Self::default()),
        }?;
        Ok(test_spec)
    }
}

impl<'lua> FromLua<'lua> for PerPlatform<TestSpec> {
    fn from_lua(value: Value<'lua>, lua: &'lua Lua) -> mlua::Result<Self> {
        let internal = PerPlatform::from_lua(value, lua)?;
        let mut per_platform = HashMap::new();
        for (platform, internal_override) in internal.per_platform {
            let override_spec = TestSpec::from_internal_spec(internal_override)
                .map_err(|err| mlua::Error::DeserializeError(err.to_string()))?;
            per_platform.insert(platform, override_spec);
        }
        let result = PerPlatform {
            default: TestSpec::from_internal_spec(internal.default)
                .map_err(|err| mlua::Error::DeserializeError(err.to_string()))?,
            per_platform,
        };
        Ok(result)
    }
}

#[derive(Debug, PartialEq, Default)]
pub struct BustedTestSpec {
    flags: Vec<String>,
}

#[derive(Debug, PartialEq)]
pub struct CommandTestSpec {
    command: String,
    flags: Vec<String>,
}

#[derive(Debug, PartialEq)]
pub struct ScriptTestSpec {
    script: PathBuf,
    flags: Vec<String>,
}

#[derive(Debug, Deserialize, PartialEq, Clone)]
#[serde(rename_all = "lowercase")]
enum TestType {
    Busted,
    Command,
}

#[derive(Debug, PartialEq, Deserialize, Default, Clone)]
struct TestSpecInternal {
    #[serde(default, rename = "type")]
    test_type: Option<TestType>,
    #[serde(default)]
    flags: Option<Vec<String>>,
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    script: Option<PathBuf>,
}

impl PartialOverride for TestSpecInternal {
    fn apply_overrides(&self, override_spec: &Self) -> Self {
        TestSpecInternal {
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
        }
    }
}

impl PlatformOverridable for TestSpecInternal {
    fn on_nil<T>() -> Result<PerPlatform<T>>
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

#[cfg(test)]
mod tests {

    use mlua::{Error, Lua};

    use crate::rockspec::PlatformIdentifier;

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
