use eyre::eyre;
use std::path::PathBuf;

use serde::{de, Deserialize, Deserializer};

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

impl<'de> Deserialize<'de> for TestSpec {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let internal = TestSpecInternal::deserialize(deserializer)?;
        let test_spec = match internal.test_type {
            Some(TestType::Busted) => Ok(Self::Busted(BustedTestSpec {
                flags: internal.flags,
            })),
            Some(TestType::Command) => match (internal.command, internal.script) {
                (None, None) => Err(eyre!(
                    "'command' test type must specify 'command' or 'script' field"
                )),
                (None, Some(script)) => Ok(Self::Script(ScriptTestSpec {
                    script,
                    flags: internal.flags,
                })),
                (Some(command), None) => Ok(Self::Command(CommandTestSpec {
                    command,
                    flags: internal.flags,
                })),
                (Some(_), Some(_)) => Err(eyre!(
                    "'command' test type cannot have both 'command' and 'script' fields"
                )),
            },
            None => Ok(Self::default()),
        }
        .map_err(de::Error::custom)?;
        Ok(test_spec)
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

#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
enum TestType {
    Busted,
    Command,
}

#[derive(Debug, PartialEq, Deserialize)]
struct TestSpecInternal {
    #[serde(default, rename = "type")]
    test_type: Option<TestType>,
    #[serde(default)]
    flags: Vec<String>,
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    script: Option<PathBuf>,
}

#[cfg(test)]
mod tests {

    use mlua::{Error, Lua, LuaSerdeExt};

    use super::*;

    #[tokio::test]
    pub async fn test_spec_from_lua() {
        let lua_content = "
        test = {\n
        }\n
        ";
        let lua = Lua::new();
        lua.load(lua_content).exec().unwrap();
        let test_spec: TestSpec = lua.from_value(lua.globals().get("test").unwrap()).unwrap();
        assert!(matches!(test_spec, TestSpec::AutoDetect { .. }));
        let lua_content = "
        test = {\n
            type = 'busted',\n
        }\n
        ";
        let lua = Lua::new();
        lua.load(lua_content).exec().unwrap();
        let test_spec: TestSpec = lua.from_value(lua.globals().get("test").unwrap()).unwrap();
        assert_eq!(test_spec, TestSpec::Busted(BustedTestSpec::default()));
        let lua_content = "
        test = {\n
            type = 'busted',\n
            flags = { 'foo', 'bar' },\n
        }\n
        ";
        let lua = Lua::new();
        lua.load(lua_content).exec().unwrap();
        let test_spec: TestSpec = lua.from_value(lua.globals().get("test").unwrap()).unwrap();
        assert_eq!(
            test_spec,
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
        let result: Result<TestSpec, Error> = lua.from_value(lua.globals().get("test").unwrap());
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
        let result: Result<TestSpec, Error> = lua.from_value(lua.globals().get("test").unwrap());
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
        let test_spec: TestSpec = lua.from_value(lua.globals().get("test").unwrap()).unwrap();
        assert_eq!(
            test_spec,
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
        let test_spec: TestSpec = lua.from_value(lua.globals().get("test").unwrap()).unwrap();
        assert_eq!(
            test_spec,
            TestSpec::Script(ScriptTestSpec {
                script: PathBuf::from("test.lua"),
                flags: vec!["foo".into(), "bar".into()],
            })
        );
    }
}
