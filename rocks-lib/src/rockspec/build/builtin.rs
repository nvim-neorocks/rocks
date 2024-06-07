use std::{collections::HashMap, path::PathBuf};

use eyre::{OptionExt, Result};
use itertools::Itertools;
use serde::{de, Deserialize, Deserializer};
use walkdir::WalkDir;

use crate::{rockspec::Rockspec, tree::TreeLayout};

use super::Build;

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

impl<'de> Deserialize<'de> for ModuleSpec {
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

#[derive(Debug, PartialEq, Deserialize, Clone)]
pub struct ModulePaths {
    /// Path names of C sources, mandatory field
    pub sources: Vec<PathBuf>,
    /// External libraries to be linked
    #[serde(default)]
    pub libraries: Vec<PathBuf>,
    /// C defines, e.g. { "FOO=bar", "USE_BLA" }
    #[serde(default, deserialize_with = "deserialize_definitions")]
    pub defines: Vec<(String, Option<String>)>,
    /// Directories to be added to the compiler's headers lookup directory list.
    #[serde(default)]
    pub incdirs: Vec<PathBuf>,
    /// Directories to be added to the linker's library lookup directory list.
    #[serde(default)]
    pub libdirs: Vec<PathBuf>,
}

impl Build for BuiltinBuildSpec {
    fn run(self, _rockspec: Rockspec, output_paths: TreeLayout, _no_install: bool) -> Result<()> {
        // Detect all Lua modules
        let modules = autodetect_modules()?
            .into_iter()
            .chain(self.modules)
            .collect::<HashMap<_, _>>();

        fn lua_module_to_pathbuf(path: &str, extension: &str) -> PathBuf {
            PathBuf::from(path.replace('.', std::path::MAIN_SEPARATOR_STR) + extension)
        }

        for (destination_path, module_type) in &modules {
            match module_type {
                ModuleSpec::SourcePath(source) => {
                    let destination_path = lua_module_to_pathbuf(destination_path, ".lua");

                    let target = output_paths.src.join(destination_path);

                    std::fs::create_dir_all(target.parent().unwrap())?;

                    std::fs::copy(source, target)?;
                }
                ModuleSpec::SourcePaths(files) => {
                    let destination_path =
                        lua_module_to_pathbuf(destination_path, std::env::consts::DLL_SUFFIX);
                    let target = output_paths.lib.join(destination_path);

                    let parent = target.parent().expect("TODO");
                    let file = target.file_name().expect("TODO");

                    std::fs::create_dir_all(parent)?;

                    cc::Build::new()
                        .cargo_metadata(false)
                        .debug(false)
                        .files(files)
                        .host(std::env::consts::OS)
                        .opt_level(3)
                        .out_dir(parent)
                        .shared_flag(true)
                        .target(std::env::consts::ARCH)
                        .try_compile(file.to_str().unwrap())?;
                }
                ModuleSpec::ModulePaths(data) => {
                    let destination_path =
                        lua_module_to_pathbuf(destination_path, std::env::consts::DLL_SUFFIX);
                    let target = output_paths.lib.join(destination_path);

                    std::fs::create_dir_all(target.parent().unwrap())?;

                    // TODO: Defines, libraries
                    let mut build = cc::Build::new();
                    let build = build
                        .cargo_metadata(false)
                        .debug(false)
                        .host(std::env::consts::OS)
                        .opt_level(3)
                        .out_dir(std::env::current_dir()?)
                        .target(std::env::consts::ARCH)
                        .shared_flag(true)
                        .files(&data.sources)
                        .includes(&data.incdirs);

                    // `cc::Build` has no `defines()` function, so we manually feed in the
                    // definitions in a verbose loop
                    for (name, value) in &data.defines {
                        build.define(name, value.as_deref());
                    }

                    for libdir in &data.libdirs {
                        build.flag(&("-L".to_string() + libdir.to_str().unwrap()));
                    }

                    for library in &data.libraries {
                        build.flag(&("-l".to_string() + library.to_str().unwrap()));
                    }

                    build.try_compile(target.to_str().unwrap())?;
                }
            }
        }

        Ok(())
    }
}

fn autodetect_modules() -> Result<HashMap<String, ModuleSpec>> {
    WalkDir::new("src")
        .into_iter()
        .chain(WalkDir::new("lua"))
        .chain(WalkDir::new("lib"))
        .filter_map(|file| {
            file.ok().and_then(|file| {
                if PathBuf::from(file.file_name())
                    .extension()
                    .map(|ext| ext == "lua")
                    .unwrap_or(false)
                    && !matches!(
                        file.file_name().to_string_lossy().as_bytes(),
                        b"spec" | b".luarocks" | b"lua_modules" | b"test.lua" | b"tests.lua"
                    )
                {
                    Some(file)
                } else {
                    None
                }
            })
        })
        .map(|file| {
            let cwd = std::env::current_dir().unwrap();
            let diff: PathBuf = pathdiff::diff_paths(cwd.join(file.into_path()), cwd)
                .ok_or_eyre("unable to autodetect modules")?;

            // NOTE(vhyrro): You may ask why we convert all paths to Lua module paths
            // just to convert them back later in the `run()` stage.
            //
            // The rockspec requires the format to be like this, and representing our
            // data in this form allows us to respect any overrides made by the user (which follow
            // the `module.name` format, not our internal one).
            let lua_module_path = diff
                .components()
                .skip(1)
                .collect::<PathBuf>()
                .to_string_lossy()
                .trim_end_matches(".lua")
                .replace(std::path::MAIN_SEPARATOR_STR, ".");

            Ok((lua_module_path, ModuleSpec::SourcePath(diff)))
        })
        .try_collect()
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
