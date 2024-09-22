use eyre::{eyre, Result};
use itertools::Itertools;
use serde::{de, Deserialize, Deserializer};
use std::{collections::HashMap, path::PathBuf};

use crate::rockspec::{FromPlatformOverridable, PartialOverride, PerPlatform, PlatformOverridable};

#[derive(Debug, PartialEq, Deserialize, Default, Clone)]
pub struct BuiltinBuildSpec {
    /// Keys are module names in the format normally used by the `require()` function
    pub modules: HashMap<String, ModuleSpec>,
}

#[derive(Debug, PartialEq, Clone)]
pub enum ModuleSpec {
    /// Pathnames of Lua files or C sources, for modules based on a single source file.
    SourcePath(PathBuf),
    /// Pathnames of C sources of a simple module written in C composed of multiple files.
    SourcePaths(Vec<PathBuf>),
    ModulePaths(ModulePaths),
}

impl ModuleSpec {
    pub fn from_internal(internal: ModuleSpecInternal) -> Result<ModuleSpec> {
        match internal {
            ModuleSpecInternal::SourcePath(path) => Ok(ModuleSpec::SourcePath(path)),
            ModuleSpecInternal::SourcePaths(paths) => Ok(ModuleSpec::SourcePaths(paths)),
            ModuleSpecInternal::ModulePaths(module_paths) => Ok(ModuleSpec::ModulePaths(
                ModulePaths::from_internal(module_paths)?,
            )),
        }
    }
}

impl<'de> Deserialize<'de> for ModuleSpec {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::from_internal(ModuleSpecInternal::deserialize(deserializer)?)
            .map_err(de::Error::custom)
    }
}

impl FromPlatformOverridable<ModuleSpecInternal, Self> for ModuleSpec {
    fn from_platform_overridable(internal: ModuleSpecInternal) -> Result<Self> {
        Self::from_internal(internal)
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum ModuleSpecInternal {
    SourcePath(PathBuf),
    SourcePaths(Vec<PathBuf>),
    ModulePaths(ModulePathsInternal),
}

impl<'de> Deserialize<'de> for ModuleSpecInternal {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        if value.is_string() {
            let src_path = serde_json::from_value(value).map_err(de::Error::custom)?;
            Ok(Self::SourcePath(src_path))
        } else if value.is_array() {
            let src_paths = serde_json::from_value(value).map_err(de::Error::custom)?;
            Ok(Self::SourcePaths(src_paths))
        } else {
            let module_paths = serde_json::from_value(value).map_err(de::Error::custom)?;
            Ok(Self::ModulePaths(module_paths))
        }
    }
}

fn deserialize_definitions<'de, D>(
    deserializer: D,
) -> Result<Vec<(String, Option<String>)>, D::Error>
where
    D: Deserializer<'de>,
{
    let values = serde_json::Value::deserialize(deserializer)?;

    // If `defines` is an empty Lua table, it's treated as a dictionary.
    // This case is handled here.
    if let Some(values_as_obj) = values.as_object() {
        if values_as_obj.is_empty() {
            return Ok(Vec::default());
        }
    }

    values
        .as_array()
        .ok_or_else(|| de::Error::custom("expected `defines` to be a list of strings"))?
        .iter()
        .map(|val| {
            if let Some((key, value)) = val
                .as_str()
                .ok_or_else(|| de::Error::custom("expected item in `defines` to be a string"))?
                .split_once('=')
            {
                Ok((key.into(), Some(value.into())))
            } else {
                Ok((val.as_str().unwrap().into(), None))
            }
        })
        .try_collect()
}

impl PartialOverride for ModuleSpecInternal {
    fn apply_overrides(&self, override_spec: &Self) -> Result<Self> {
        match (override_spec, self) {
            (ModuleSpecInternal::SourcePath(_), b @ ModuleSpecInternal::SourcePath(_)) => {
                Ok(b.to_owned())
            }
            (ModuleSpecInternal::SourcePaths(_), b @ ModuleSpecInternal::SourcePaths(_)) => {
                Ok(b.to_owned())
            }
            (ModuleSpecInternal::ModulePaths(a), ModuleSpecInternal::ModulePaths(b)) => {
                Ok(ModuleSpecInternal::ModulePaths(a.apply_overrides(b)?))
            }
            _ => Err(eyre!(
                "cannot resolve ambiguous platform override for `build.modules`."
            )),
        }
    }
}

impl PlatformOverridable for ModuleSpecInternal {
    fn on_nil<T>() -> Result<PerPlatform<T>>
    where
        T: PlatformOverridable,
    {
        Err(eyre!(
            "could not resolve platform override for `build.modules`. This is a bug!"
        ))
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct ModulePaths {
    /// Path names of C sources, mandatory field
    pub sources: Vec<PathBuf>,
    /// External libraries to be linked
    pub libraries: Vec<PathBuf>,
    /// C defines, e.g. { "FOO=bar", "USE_BLA" }
    pub defines: Vec<(String, Option<String>)>,
    /// Directories to be added to the compiler's headers lookup directory list.
    pub incdirs: Vec<PathBuf>,
    /// Directories to be added to the linker's library lookup directory list.
    pub libdirs: Vec<PathBuf>,
}

impl ModulePaths {
    fn from_internal(internal: ModulePathsInternal) -> Result<ModulePaths> {
        if internal.sources.is_empty() {
            return Err(eyre!("missing or empty field `sources`"));
        }
        Ok(ModulePaths {
            sources: internal.sources,
            libraries: internal.libraries,
            defines: internal.defines,
            incdirs: internal.incdirs,
            libdirs: internal.libdirs,
        })
    }
}

impl<'de> Deserialize<'de> for ModulePaths {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let internal = ModulePathsInternal::deserialize(deserializer)?;
        Self::from_internal(internal).map_err(de::Error::custom)
    }
}

#[derive(Debug, PartialEq, Deserialize, Clone, Default)]
pub struct ModulePathsInternal {
    #[serde(default)]
    pub sources: Vec<PathBuf>,
    #[serde(default)]
    pub libraries: Vec<PathBuf>,
    #[serde(default, deserialize_with = "deserialize_definitions")]
    pub defines: Vec<(String, Option<String>)>,
    #[serde(default)]
    pub incdirs: Vec<PathBuf>,
    #[serde(default)]
    pub libdirs: Vec<PathBuf>,
}

impl PartialOverride for ModulePathsInternal {
    fn apply_overrides(&self, override_spec: &Self) -> Result<Self> {
        Ok(Self {
            sources: override_vec(override_spec.sources.as_ref(), self.sources.as_ref()),
            libraries: override_vec(override_spec.libraries.as_ref(), self.libraries.as_ref()),
            defines: override_vec(override_spec.defines.as_ref(), self.defines.as_ref()),
            incdirs: override_vec(override_spec.incdirs.as_ref(), self.incdirs.as_ref()),
            libdirs: override_vec(override_spec.libdirs.as_ref(), self.libdirs.as_ref()),
        })
    }
}

impl PlatformOverridable for ModulePathsInternal {
    fn on_nil<T>() -> Result<PerPlatform<T>>
    where
        T: PlatformOverridable,
        T: Default,
    {
        Ok(PerPlatform::default())
    }
}

fn override_vec<T: Clone>(override_vec: &[T], base: &[T]) -> Vec<T> {
    if override_vec.is_empty() {
        return base.to_owned();
    }
    override_vec.to_owned()
}

#[cfg(test)]
mod tests {
    use mlua::{Lua, LuaSerdeExt};

    use super::*;

    #[tokio::test]
    pub async fn modules_spec_from_lua() {
        let lua_content = "
        build = {\n
            modules = {\n
                foo = 'lua/foo/init.lua',\n
                bar = {\n
                  'lua/bar.lua',\n
                  'lua/bar/internal.lua',\n
                },\n
                baz = {\n
                    sources = {\n
                        'lua/baz.lua',\n
                    },\n
                    defines = { 'USE_BAZ' },\n
                },\n
            },\n
        }\n
        ";
        let lua = Lua::new();
        lua.load(lua_content).exec().unwrap();
        let build_spec: BuiltinBuildSpec =
            lua.from_value(lua.globals().get("build").unwrap()).unwrap();
        let foo = build_spec.modules.get("foo").unwrap();
        assert_eq!(*foo, ModuleSpec::SourcePath("lua/foo/init.lua".into()));
        let bar = build_spec.modules.get("bar").unwrap();
        assert_eq!(
            *bar,
            ModuleSpec::SourcePaths(vec!["lua/bar.lua".into(), "lua/bar/internal.lua".into()])
        );
        let baz = build_spec.modules.get("baz").unwrap();
        assert!(matches!(baz, ModuleSpec::ModulePaths { .. }));
        let lua_content_no_sources = "
        build = {\n
            modules = {\n
                baz = {\n
                    defines = { 'USE_BAZ' },\n
                },\n
            },\n
        }\n
        ";
        lua.load(lua_content_no_sources).exec().unwrap();
        let result: mlua::Result<BuiltinBuildSpec> =
            lua.from_value(lua.globals().get("build").unwrap());
        let _err = result.unwrap_err();
        let lua_content_complex_defines = "
        build = {\n
            modules = {\n
                baz = {\n
                    sources = {\n
                        'lua/baz.lua',\n
                    },\n
                    defines = { 'USE_BAZ=1', 'ENABLE_LOGGING=true', 'LINK_STATIC' },\n
                },\n
            },\n
        }\n
        ";
        lua.load(lua_content_complex_defines).exec().unwrap();
        let build_spec: BuiltinBuildSpec =
            lua.from_value(lua.globals().get("build").unwrap()).unwrap();
        let baz = build_spec.modules.get("baz").unwrap();
        match baz {
            ModuleSpec::ModulePaths(paths) => assert_eq!(
                paths.defines,
                vec![
                    ("USE_BAZ".into(), Some("1".into())),
                    ("ENABLE_LOGGING".into(), Some("true".into())),
                    ("LINK_STATIC".into(), None)
                ]
            ),
            _ => panic!(),
        }
    }
}
