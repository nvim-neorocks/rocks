mod builtin;
mod cmake;
mod make;
mod rust_mlua;

pub use builtin::{BuiltinBuildSpec, LuaModule, ModulePaths, ModuleSpec};
pub use cmake::*;
pub use make::*;
pub use rust_mlua::*;

use builtin::{
    ModulePathsMissingSources, ModuleSpecAmbiguousPlatformOverride, ModuleSpecInternal,
    ParseLuaModuleError,
};

use itertools::Itertools;

use mlua::{FromLua, Lua, LuaSerdeExt, Value};
use std::{
    collections::HashMap,
    env::consts::DLL_EXTENSION,
    fmt::Display,
    future::Future,
    path::{Path, PathBuf},
    str::FromStr,
};
use thiserror::Error;

use serde::{de, de::IntoDeserializer, Deserialize, Deserializer};

use crate::{
    config::Config,
    lua_installation::LuaInstallation,
    progress::{Progress, ProgressBar},
    tree::RockLayout,
};

use super::{
    mlua_json_value_to_vec, DisplayAsLuaKV, DisplayAsLuaValue, DisplayLuaKV, DisplayLuaValue,
    LuaTableKey, PartialOverride, PerPlatform, PlatformIdentifier,
};

/// The build specification for a given rock, serialized from `rockspec.build = { ... }`.
///
/// See [the rockspec format](https://github.com/luarocks/luarocks/wiki/Rockspec-format) for more
/// info.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct BuildSpec {
    /// Determines the build backend to use.
    pub build_backend: Option<BuildBackendSpec>,
    /// A set of instructions on how/where to copy files from the project.
    // TODO(vhyrro): While we may want to support this, we also may want to supercede this in our
    // new Lua project rewrite.
    pub install: InstallSpec,
    /// A list of directories that should be copied as-is into the resulting rock.
    pub copy_directories: Vec<PathBuf>,
    /// A list of patches to apply to the project before packaging it.
    // NOTE: This cannot be a diffy::Patch<'a, str>
    // because Lua::from_value requires a DeserializeOwned
    pub patches: HashMap<PathBuf, String>,
}

#[derive(Error, Debug)]
pub enum BuildSpecInternalError {
    #[error("'builtin' modules should not have list elements")]
    ModulesHaveListElements,
    #[error("no 'build_command' specified for the 'command' build backend")]
    NoBuildCommand,
    #[error("no 'install_command' specified for the 'command' build backend")]
    NoInstallCommand,
    #[error("no 'modules' specified for the 'rust-mlua' build backend")]
    NoModulesSpecified,
    #[error("invalid 'rust-mlua' modules format")]
    InvalidRustMLuaFormat,
    #[error(transparent)]
    ModulePathsMissingSources(#[from] ModulePathsMissingSources),
    #[error(transparent)]
    ParseLuaModuleError(#[from] ParseLuaModuleError),
}

impl BuildSpec {
    pub(crate) fn from_internal_spec(
        internal: BuildSpecInternal,
    ) -> Result<Self, BuildSpecInternalError> {
        let build_backend = match internal.build_type.unwrap_or_default() {
            BuildType::Builtin => Some(BuildBackendSpec::Builtin(BuiltinBuildSpec {
                modules: internal
                    .builtin_spec
                    .unwrap_or_default()
                    .into_iter()
                    .map(|(key, module_spec_internal)| {
                        let key_str = match key {
                            LuaTableKey::IntKey(_) => {
                                Err(BuildSpecInternalError::ModulesHaveListElements)
                            }
                            LuaTableKey::StringKey(str) => Ok(LuaModule::from_str(str.as_str())?),
                        }?;
                        match ModuleSpec::from_internal(module_spec_internal) {
                            Ok(module_spec) => Ok((key_str, module_spec)),
                            Err(err) => Err(err.into()),
                        }
                    })
                    .collect::<Result<HashMap<LuaModule, ModuleSpec>, BuildSpecInternalError>>()?,
            })),
            BuildType::Make => {
                let default = MakeBuildSpec::default();
                Some(BuildBackendSpec::Make(MakeBuildSpec {
                    makefile: internal.makefile.unwrap_or(default.makefile),
                    build_target: internal.make_build_target.unwrap_or_default(),
                    build_pass: internal.build_pass.unwrap_or(default.build_pass),
                    install_target: internal
                        .make_install_target
                        .unwrap_or(default.install_target),
                    install_pass: internal.install_pass.unwrap_or(default.install_pass),
                    build_variables: internal.make_build_variables.unwrap_or_default(),
                    install_variables: internal.make_install_variables.unwrap_or_default(),
                    variables: internal.variables.unwrap_or_default(),
                }))
            }
            BuildType::CMake => {
                let default = CMakeBuildSpec::default();
                Some(BuildBackendSpec::CMake(CMakeBuildSpec {
                    cmake_lists_content: internal.cmake_lists_content,
                    build_pass: internal.build_pass.unwrap_or(default.build_pass),
                    install_pass: internal.install_pass.unwrap_or(default.install_pass),
                    variables: internal.variables.unwrap_or_default(),
                }))
            }
            BuildType::Command => {
                let build_command = internal
                    .build_command
                    .ok_or(BuildSpecInternalError::NoBuildCommand)?;
                let install_command = internal
                    .install_command
                    .ok_or(BuildSpecInternalError::NoInstallCommand)?;
                Some(BuildBackendSpec::Command(CommandBuildSpec {
                    build_command,
                    install_command,
                }))
            }
            BuildType::None => None,
            BuildType::LuaRock(s) => Some(BuildBackendSpec::LuaRock(s)),
            BuildType::RustMlua => Some(BuildBackendSpec::RustMlua(RustMluaBuildSpec {
                modules: internal
                    .builtin_spec
                    .ok_or(BuildSpecInternalError::NoModulesSpecified)?
                    .into_iter()
                    .map(|(key, value)| match (key, value) {
                        (LuaTableKey::IntKey(_), ModuleSpecInternal::SourcePath(module)) => {
                            let mut rust_lib: PathBuf = format!("lib{}", module.display()).into();
                            rust_lib.set_extension(DLL_EXTENSION);
                            Ok((module.to_string_lossy().to_string(), rust_lib))
                        }
                        (
                            LuaTableKey::StringKey(module_name),
                            ModuleSpecInternal::SourcePath(module),
                        ) => {
                            let mut rust_lib: PathBuf = format!("lib{}", module.display()).into();
                            rust_lib.set_extension(DLL_EXTENSION);
                            Ok((module_name, rust_lib))
                        }
                        _ => Err(BuildSpecInternalError::InvalidRustMLuaFormat),
                    })
                    .try_collect()?,
                target_path: internal.target_path.unwrap_or("target".into()),
                default_features: internal.default_features.unwrap_or(true),
                include: internal
                    .include
                    .unwrap_or_default()
                    .into_iter()
                    .map(|(key, dest)| match key {
                        LuaTableKey::IntKey(_) => (dest.clone(), dest),
                        LuaTableKey::StringKey(src) => (src.into(), dest),
                    })
                    .collect(),
                features: internal.features.unwrap_or_default(),
            })),
        };
        Ok(Self {
            build_backend,
            install: internal.install.unwrap_or_default(),
            copy_directories: internal.copy_directories.unwrap_or_default(),
            patches: internal.patches.unwrap_or_default(),
        })
    }
}

impl<'de> Deserialize<'de> for BuildSpec {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let internal = BuildSpecInternal::deserialize(deserializer)?;
        BuildSpec::from_internal_spec(internal).map_err(de::Error::custom)
    }
}

impl FromLua for PerPlatform<BuildSpec> {
    fn from_lua(value: Value, lua: &Lua) -> mlua::Result<Self> {
        let internal = PerPlatform::from_lua(value, lua)?;
        let mut per_platform = HashMap::new();
        for (platform, internal_override) in internal.per_platform {
            let override_spec = BuildSpec::from_internal_spec(internal_override)
                .map_err(|err| mlua::Error::DeserializeError(err.to_string()))?;
            per_platform.insert(platform, override_spec);
        }
        let result = PerPlatform {
            default: BuildSpec::from_internal_spec(internal.default)
                .map_err(|err| mlua::Error::DeserializeError(err.to_string()))?,
            per_platform,
        };
        Ok(result)
    }
}

impl Default for BuildBackendSpec {
    fn default() -> Self {
        Self::Builtin(BuiltinBuildSpec::default())
    }
}

/// Encodes extra information about each backend.
/// When selecting a backend, one may provide extra parameters
/// to `build = { ... }` in order to further customize the behaviour of the build step.
///
/// Luarocks provides several default build types, these are also reflected in `rocks`
/// for compatibility.
#[derive(Debug, PartialEq, Clone)]
pub enum BuildBackendSpec {
    Builtin(BuiltinBuildSpec),
    Make(MakeBuildSpec),
    CMake(CMakeBuildSpec),
    Command(CommandBuildSpec),
    LuaRock(String),
    RustMlua(RustMluaBuildSpec),
}

#[derive(Debug, PartialEq, Clone)]
pub struct CommandBuildSpec {
    pub build_command: String,
    pub install_command: String,
}

/// For packages which don't provide means to install modules
/// and expect the user to copy the .lua or library files by hand to the proper locations.
/// This struct contains categories of files. Each category is itself a table,
/// where the array part is a list of filenames to be copied.
/// For module directories only, in the hash part, other keys are identifiers in Lua module format,
/// to indicate which subdirectory the file should be copied to.
/// For example, build.install.lua = {["foo.bar"] = {"src/bar.lua"}} will copy src/bar.lua
/// to the foo directory under the rock's Lua files directory.
#[derive(Debug, PartialEq, Default, Deserialize, Clone)]
pub struct InstallSpec {
    /// Lua modules written in Lua.
    #[serde(default)]
    pub lua: HashMap<LuaModule, PathBuf>,
    /// Dynamic libraries implemented compiled Lua modules.
    #[serde(default)]
    pub lib: HashMap<LuaModule, PathBuf>,
    /// Configuration files.
    #[serde(default)]
    pub conf: HashMap<String, PathBuf>,
    /// Lua command-line scripts.
    // TODO(vhyrro): The String component should be checked to ensure that it consists of a single
    // path component, such that targets like `my.binary` are not allowed.
    #[serde(default)]
    pub bin: HashMap<String, PathBuf>,
}

fn deserialize_copy_directories<'de, D>(deserializer: D) -> Result<Option<Vec<PathBuf>>, D::Error>
where
    D: Deserializer<'de>,
{
    let value: Option<serde_json::Value> = Option::deserialize(deserializer)?;
    let copy_directories: Option<Vec<String>> = match value {
        Some(json_value) => Some(mlua_json_value_to_vec(json_value).map_err(de::Error::custom)?),
        None => None,
    };
    let special_directories: Vec<String> = vec!["lua".into(), "lib".into(), "rock_manifest".into()];
    match special_directories
        .into_iter()
        .find(|dir| copy_directories.clone().unwrap_or_default().contains(dir))
    {
        // NOTE(mrcjkb): There also shouldn't be a directory named the same as the rockspec,
        // but I'm not sure how to (or if it makes sense to) enforce this here.
        Some(d) => Err(format!(
            "directory '{}' in copy_directories clashes with the .rock format", // TODO(vhyrro): More informative error message.
            d
        )),
        _ => Ok(copy_directories.map(|vec| vec.into_iter().map(PathBuf::from).collect())),
    }
    .map_err(de::Error::custom)
}

impl DisplayAsLuaKV for InstallSpec {
    fn display_lua(&self) -> DisplayLuaKV {
        let mut result = Vec::new();

        self.lua
            .iter()
            .chain(self.lib.iter())
            .for_each(|(key, value)| {
                result.push(DisplayLuaKV {
                    key: key.to_string(),
                    value: DisplayLuaValue::String(value.to_string_lossy().to_string()),
                });
            });

        self.conf
            .iter()
            .chain(self.bin.iter())
            .for_each(|(key, value)| {
                result.push(DisplayLuaKV {
                    key: key.clone(),
                    value: DisplayLuaValue::String(value.to_string_lossy().to_string()),
                });
            });

        DisplayLuaKV {
            key: "install".to_string(),
            value: DisplayLuaValue::Table(result),
        }
    }
}

#[derive(Debug, PartialEq, Deserialize, Default, Clone)]
pub(crate) struct BuildSpecInternal {
    #[serde(rename = "type", default)]
    pub(crate) build_type: Option<BuildType>,
    #[serde(rename = "modules", default)]
    pub(crate) builtin_spec: Option<HashMap<LuaTableKey, ModuleSpecInternal>>,
    #[serde(default)]
    pub(crate) makefile: Option<PathBuf>,
    #[serde(rename = "build_target", default)]
    pub(crate) make_build_target: Option<String>,
    #[serde(default)]
    pub(crate) build_pass: Option<bool>,
    #[serde(rename = "install_target", default)]
    pub(crate) make_install_target: Option<String>,
    #[serde(default)]
    pub(crate) install_pass: Option<bool>,
    #[serde(rename = "build_variables", default)]
    pub(crate) make_build_variables: Option<HashMap<String, String>>,
    #[serde(rename = "install_variables", default)]
    pub(crate) make_install_variables: Option<HashMap<String, String>>,
    #[serde(default)]
    pub(crate) variables: Option<HashMap<String, String>>,
    #[serde(rename = "cmake", default)]
    pub(crate) cmake_lists_content: Option<String>,
    #[serde(default)]
    pub(crate) build_command: Option<String>,
    #[serde(default)]
    pub(crate) install_command: Option<String>,
    #[serde(default)]
    pub(crate) install: Option<InstallSpec>,
    #[serde(default, deserialize_with = "deserialize_copy_directories")]
    pub(crate) copy_directories: Option<Vec<PathBuf>>,
    #[serde(default)]
    pub(crate) patches: Option<HashMap<PathBuf, String>>,
    // rust-mlua fields
    #[serde(default)]
    pub(crate) target_path: Option<PathBuf>,
    #[serde(default)]
    pub(crate) default_features: Option<bool>,
    #[serde(default)]
    pub(crate) include: Option<HashMap<LuaTableKey, PathBuf>>,
    #[serde(default)]
    pub(crate) features: Option<Vec<String>>,
}

impl FromLua for PerPlatform<BuildSpecInternal> {
    fn from_lua(value: Value, lua: &Lua) -> mlua::Result<Self> {
        match &value {
            list @ Value::Table(tbl) => {
                let mut per_platform = match tbl.get("platforms")? {
                    Value::Table(overrides) => Ok(lua.from_value(Value::Table(overrides))?),
                    Value::Nil => Ok(HashMap::default()),
                    val => Err(mlua::Error::DeserializeError(format!(
                        "Expected rockspec 'build' to be table or nil, but got {}",
                        val.type_name()
                    ))),
                }?;
                let _ = tbl.raw_remove("platforms");
                let default = lua.from_value(list.clone())?;
                override_platform_specs(&mut per_platform, &default)
                    .map_err(|err| mlua::Error::DeserializeError(err.to_string()))?;
                Ok(PerPlatform {
                    default,
                    per_platform,
                })
            }
            Value::Nil => Ok(PerPlatform::default()),
            val => Err(mlua::Error::DeserializeError(format!(
                "Expected rockspec 'build' to be a table or nil, but got {}",
                val.type_name()
            ))),
        }
    }
}

/// For each platform in `per_platform`, add the base specs,
/// and apply overrides to the extended platforms of each platform override.
fn override_platform_specs(
    per_platform: &mut HashMap<PlatformIdentifier, BuildSpecInternal>,
    base: &BuildSpecInternal,
) -> Result<(), ModuleSpecAmbiguousPlatformOverride> {
    let per_platform_raw = per_platform.clone();
    for (platform, build_spec) in per_platform.clone() {
        // Add base dependencies for each platform
        per_platform.insert(platform, override_build_spec_internal(base, &build_spec)?);
    }
    for (platform, build_spec) in per_platform_raw {
        for extended_platform in &platform.get_extended_platforms() {
            let extended_spec = per_platform
                .get(extended_platform)
                .unwrap_or(&base.to_owned())
                .to_owned();
            per_platform.insert(
                extended_platform.to_owned(),
                override_build_spec_internal(&extended_spec, &build_spec)?,
            );
        }
    }
    Ok(())
}

fn override_build_spec_internal(
    base: &BuildSpecInternal,
    override_spec: &BuildSpecInternal,
) -> Result<BuildSpecInternal, ModuleSpecAmbiguousPlatformOverride> {
    Ok(BuildSpecInternal {
        build_type: override_opt(&override_spec.build_type, &base.build_type),
        builtin_spec: match (
            override_spec.builtin_spec.clone(),
            base.builtin_spec.clone(),
        ) {
            (Some(override_val), Some(base_spec_map)) => {
                Some(base_spec_map.into_iter().chain(override_val).try_fold(
                    HashMap::default(),
                    |mut acc: HashMap<LuaTableKey, ModuleSpecInternal>,
                     (k, module_spec_override)|
                     -> Result<
                        HashMap<LuaTableKey, ModuleSpecInternal>,
                        ModuleSpecAmbiguousPlatformOverride,
                    > {
                        let overridden = match acc.get(&k) {
                            None => module_spec_override,
                            Some(base_module_spec) => {
                                base_module_spec.apply_overrides(&module_spec_override)?
                            }
                        };
                        acc.insert(k, overridden);
                        Ok(acc)
                    },
                )?)
            }
            (override_val @ Some(_), _) => override_val,
            (_, base_val @ Some(_)) => base_val,
            _ => None,
        },
        makefile: override_opt(&override_spec.makefile, &base.makefile),
        make_build_target: override_opt(&override_spec.make_build_target, &base.make_build_target),
        build_pass: override_opt(&override_spec.build_pass, &base.build_pass),
        make_install_target: override_opt(
            &override_spec.make_install_target,
            &base.make_install_target,
        ),
        install_pass: override_opt(&override_spec.install_pass, &base.install_pass),
        make_build_variables: merge_map_opts(
            &override_spec.make_build_variables,
            &base.make_build_variables,
        ),
        make_install_variables: merge_map_opts(
            &override_spec.make_install_variables,
            &base.make_build_variables,
        ),
        variables: merge_map_opts(&override_spec.variables, &base.variables),
        cmake_lists_content: override_opt(
            &override_spec.cmake_lists_content,
            &base.cmake_lists_content,
        ),
        build_command: override_opt(&override_spec.build_command, &base.build_command),
        install_command: override_opt(&override_spec.install_command, &base.install_command),
        install: override_opt(&override_spec.install, &base.install),
        copy_directories: match (
            override_spec.copy_directories.clone(),
            base.copy_directories.clone(),
        ) {
            (Some(override_vec), Some(base_vec)) => {
                let merged: Vec<PathBuf> =
                    base_vec.into_iter().chain(override_vec).unique().collect();
                Some(merged)
            }
            (None, base_vec @ Some(_)) => base_vec,
            (override_vec @ Some(_), None) => override_vec,
            _ => None,
        },
        patches: override_opt(&override_spec.patches, &base.patches),
        target_path: override_opt(&override_spec.target_path, &base.target_path),
        default_features: override_opt(&override_spec.default_features, &base.default_features),
        features: override_opt(&override_spec.features, &base.features),
        include: merge_map_opts(&override_spec.include, &base.include),
    })
}

fn override_opt<T: Clone>(override_opt: &Option<T>, base: &Option<T>) -> Option<T> {
    match override_opt.clone() {
        override_val @ Some(_) => override_val,
        None => base.clone(),
    }
}

fn merge_map_opts<K, V>(
    override_map: &Option<HashMap<K, V>>,
    base_map: &Option<HashMap<K, V>>,
) -> Option<HashMap<K, V>>
where
    K: Clone,
    K: Eq,
    K: std::hash::Hash,
    V: Clone,
{
    match (override_map.clone(), base_map.clone()) {
        (Some(override_map), Some(base_map)) => {
            Some(base_map.into_iter().chain(override_map).collect())
        }
        (_, base_map @ Some(_)) => base_map,
        (override_map @ Some(_), _) => override_map,
        _ => None,
    }
}

impl DisplayAsLuaKV for BuildSpecInternal {
    fn display_lua(&self) -> DisplayLuaKV {
        let mut result = Vec::new();

        if let Some(build_type) = &self.build_type {
            result.push(DisplayLuaKV {
                key: "type".to_string(),
                value: DisplayLuaValue::String(build_type.to_string()),
            });
        }
        if let Some(builtin_spec) = &self.builtin_spec {
            result.push(DisplayLuaKV {
                key: "modules".to_string(),
                value: DisplayLuaValue::Table(
                    builtin_spec
                        .iter()
                        .map(|(key, value)| DisplayLuaKV {
                            key: match key {
                                LuaTableKey::StringKey(s) => s.clone(),
                                LuaTableKey::IntKey(_) => unreachable!("integer key in modules"),
                            },
                            value: value.display_lua_value(),
                        })
                        .collect(),
                ),
            });
        }
        if let Some(makefile) = &self.makefile {
            result.push(DisplayLuaKV {
                key: "makefile".to_string(),
                value: DisplayLuaValue::String(makefile.to_string_lossy().to_string()),
            });
        }
        if let Some(make_build_target) = &self.make_build_target {
            result.push(DisplayLuaKV {
                key: "build_target".to_string(),
                value: DisplayLuaValue::String(make_build_target.clone()),
            });
        }
        if let Some(build_pass) = &self.build_pass {
            result.push(DisplayLuaKV {
                key: "build_pass".to_string(),
                value: DisplayLuaValue::Boolean(*build_pass),
            });
        }
        if let Some(make_install_target) = &self.make_install_target {
            result.push(DisplayLuaKV {
                key: "install_target".to_string(),
                value: DisplayLuaValue::String(make_install_target.clone()),
            });
        }
        if let Some(install_pass) = &self.install_pass {
            result.push(DisplayLuaKV {
                key: "install_pass".to_string(),
                value: DisplayLuaValue::Boolean(*install_pass),
            });
        }
        if let Some(make_build_variables) = &self.make_build_variables {
            result.push(DisplayLuaKV {
                key: "build_variables".to_string(),
                value: DisplayLuaValue::Table(
                    make_build_variables
                        .iter()
                        .map(|(key, value)| DisplayLuaKV {
                            key: key.clone(),
                            value: DisplayLuaValue::String(value.clone()),
                        })
                        .collect(),
                ),
            });
        }
        if let Some(make_install_variables) = &self.make_install_variables {
            result.push(DisplayLuaKV {
                key: "install_variables".to_string(),
                value: DisplayLuaValue::Table(
                    make_install_variables
                        .iter()
                        .map(|(key, value)| DisplayLuaKV {
                            key: key.clone(),
                            value: DisplayLuaValue::String(value.clone()),
                        })
                        .collect(),
                ),
            });
        }
        if let Some(variables) = &self.variables {
            result.push(DisplayLuaKV {
                key: "variables".to_string(),
                value: DisplayLuaValue::Table(
                    variables
                        .iter()
                        .map(|(key, value)| DisplayLuaKV {
                            key: key.clone(),
                            value: DisplayLuaValue::String(value.clone()),
                        })
                        .collect(),
                ),
            });
        }
        if let Some(cmake_lists_content) = &self.cmake_lists_content {
            result.push(DisplayLuaKV {
                key: "cmake".to_string(),
                value: DisplayLuaValue::String(cmake_lists_content.clone()),
            });
        }
        if let Some(build_command) = &self.build_command {
            result.push(DisplayLuaKV {
                key: "build_command".to_string(),
                value: DisplayLuaValue::String(build_command.clone()),
            });
        }
        if let Some(install_command) = &self.install_command {
            result.push(DisplayLuaKV {
                key: "install_command".to_string(),
                value: DisplayLuaValue::String(install_command.clone()),
            });
        }
        if let Some(install) = &self.install {
            result.push(install.display_lua());
        }
        if let Some(copy_directories) = &self.copy_directories {
            result.push(DisplayLuaKV {
                key: "copy_directories".to_string(),
                value: DisplayLuaValue::List(
                    copy_directories
                        .iter()
                        .map(|path_buf| {
                            DisplayLuaValue::String(path_buf.to_string_lossy().to_string())
                        })
                        .collect(),
                ),
            });
        }
        if let Some(patches) = &self.patches {
            result.push(DisplayLuaKV {
                key: "patches".to_string(),
                value: DisplayLuaValue::Table(
                    patches
                        .iter()
                        .map(|(key, value)| DisplayLuaKV {
                            key: key.to_string_lossy().to_string(),
                            value: DisplayLuaValue::String(value.clone()),
                        })
                        .collect(),
                ),
            });
        }
        if let Some(target_path) = &self.target_path {
            result.push(DisplayLuaKV {
                key: "target_path".to_string(),
                value: DisplayLuaValue::String(target_path.to_string_lossy().to_string()),
            });
        }
        if let Some(default_features) = &self.default_features {
            result.push(DisplayLuaKV {
                key: "default_features".to_string(),
                value: DisplayLuaValue::Boolean(*default_features),
            });
        }
        if let Some(include) = &self.include {
            result.push(DisplayLuaKV {
                key: "include".to_string(),
                value: DisplayLuaValue::Table(
                    include
                        .iter()
                        .map(|(key, value)| DisplayLuaKV {
                            key: match key {
                                LuaTableKey::StringKey(s) => s.clone(),
                                LuaTableKey::IntKey(_) => unreachable!("integer key in include"),
                            },
                            value: DisplayLuaValue::String(value.to_string_lossy().to_string()),
                        })
                        .collect(),
                ),
            });
        }
        if let Some(features) = &self.features {
            result.push(DisplayLuaKV {
                key: "features".to_string(),
                value: DisplayLuaValue::List(
                    features
                        .iter()
                        .map(|feature| DisplayLuaValue::String(feature.clone()))
                        .collect(),
                ),
            });
        }

        DisplayLuaKV {
            key: "build".to_string(),
            value: DisplayLuaValue::Table(result),
        }
    }
}

/// Maps `build.type` to an enum.
#[derive(Debug, PartialEq, Deserialize, Clone)]
#[serde(rename_all = "lowercase", remote = "BuildType")]
pub(crate) enum BuildType {
    /// "builtin" or "module"
    Builtin,
    /// "make"
    Make,
    /// "cmake"
    CMake,
    /// "command"
    Command,
    /// "none"
    None,
    /// external Lua rock
    LuaRock(String),
    #[serde(rename = "rust-mlua")]
    RustMlua,
}

// Special Deserialize case for BuildType:
// Both "module" and "builtin" map to `Builtin`
impl<'de> Deserialize<'de> for BuildType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        if s == "builtin" || s == "module" {
            Ok(Self::Builtin)
        } else {
            match Self::deserialize(s.clone().into_deserializer()) {
                Err(_) => Ok(Self::LuaRock(s)),
                ok => ok,
            }
        }
    }
}

impl Display for BuildType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildType::Builtin => write!(f, "builtin"),
            BuildType::Make => write!(f, "make"),
            BuildType::CMake => write!(f, "cmake"),
            BuildType::Command => write!(f, "command"),
            BuildType::None => write!(f, "none"),
            BuildType::LuaRock(s) => write!(f, "{}", s),
            BuildType::RustMlua => write!(f, "rust-mlua"),
        }
    }
}

impl Default for BuildType {
    fn default() -> Self {
        Self::Builtin
    }
}

#[derive(Default)]
pub struct BuildInfo {
    pub binaries: Vec<PathBuf>,
}

// TODO(vhyrro): Move this to the dedicated build.rs module
pub trait Build {
    type Err: std::error::Error;

    fn run(
        self,
        output_paths: &RockLayout,
        no_install: bool,
        lua: &LuaInstallation,
        config: &Config,
        build_dir: &Path,
        progress: &Progress<ProgressBar>,
    ) -> impl Future<Output = Result<BuildInfo, Self::Err>> + Send;
}

#[cfg(test)]
mod tests {

    use super::*;

    #[tokio::test]
    pub async fn deserialize_build_type() {
        let build_type: BuildType = serde_json::from_str("\"builtin\"").unwrap();
        assert_eq!(build_type, BuildType::Builtin);
        let build_type: BuildType = serde_json::from_str("\"module\"").unwrap();
        assert_eq!(build_type, BuildType::Builtin);
        let build_type: BuildType = serde_json::from_str("\"make\"").unwrap();
        assert_eq!(build_type, BuildType::Make);
        let build_type: BuildType = serde_json::from_str("\"custom_build_backend\"").unwrap();
        assert_eq!(
            build_type,
            BuildType::LuaRock("custom_build_backend".into())
        );
        let build_type: BuildType = serde_json::from_str("\"rust-mlua\"").unwrap();
        assert_eq!(build_type, BuildType::RustMlua);
    }
}
