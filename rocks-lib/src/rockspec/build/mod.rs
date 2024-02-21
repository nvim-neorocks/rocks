mod builtin;
mod cmake;
mod make;

pub use builtin::*;
pub use cmake::*;
use itertools::Itertools as _;
pub use make::*;

use eyre::{eyre, OptionExt as _, Result};
use mlua::{FromLua, Lua, LuaSerdeExt, Value};
use std::{collections::HashMap, path::PathBuf};

use serde::{de, de::IntoDeserializer, Deserialize, Deserializer};

use super::{PerPlatform, PlatformIdentifier, Rockspec};

#[derive(Debug, PartialEq, Default)]
pub struct BuildSpec {
    pub build_backend: Option<BuildBackendSpec>,
    pub install: InstallSpec,
    pub copy_directories: Vec<PathBuf>,
    pub patches: HashMap<PathBuf, String>,
}

impl BuildSpec {
    fn from_internal_spec(internal: BuildSpecInternal) -> Result<Self> {
        let build_backend = match internal.build_type.unwrap_or_default() {
            BuildType::Builtin => Some(BuildBackendSpec::Builtin(
                internal.builtin_spec.unwrap_or_default(),
            )),
            BuildType::Make => {
                let default = MakeBuildSpec::default();
                Some(BuildBackendSpec::Make(MakeBuildSpec {
                    makefile: internal.makefile.unwrap_or(default.makefile),
                    build_target: internal.make_build_target.unwrap_or_default(),
                    build_pass: internal.make_build_pass.unwrap_or(default.build_pass),
                    install_target: internal
                        .make_install_target
                        .unwrap_or(default.install_target),
                    install_pass: internal.make_install_pass.unwrap_or(default.install_pass),
                    build_variables: internal.make_build_variables.unwrap_or_default(),
                    install_variables: internal.make_install_variables.unwrap_or_default(),
                    variables: internal.variables.unwrap_or_default(),
                }))
            }
            BuildType::CMake => Some(BuildBackendSpec::CMake(CMakeBuildSpec {
                cmake_lists_content: internal.cmake_lists_content,
                variables: internal.variables.unwrap_or_default(),
            })),
            BuildType::Command => {
                let build_command = internal
                    .build_command
                    .ok_or_eyre("no 'build_command' specied")?;
                let install_command = internal
                    .install_command
                    .ok_or_eyre("no 'install_command' specied")?;
                Some(BuildBackendSpec::Command(CommandBuildSpec {
                    build_command,
                    install_command,
                }))
            }
            BuildType::None => None,
            BuildType::LuaRock(s) => Some(BuildBackendSpec::LuaRock(s)),
        };
        Ok(Self {
            build_backend,
            install: internal.install.unwrap_or_default(),
            copy_directories: internal.copy_directories.unwrap_or_default(),
            patches: internal.patches.unwrap_or_default(),
        })
    }
}

impl<'lua> FromLua<'lua> for PerPlatform<BuildSpec> {
    fn from_lua(value: Value<'lua>, lua: &'lua Lua) -> mlua::Result<Self> {
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

#[derive(Debug, PartialEq, Clone)]
pub enum BuildBackendSpec {
    Builtin(BuiltinBuildSpec),
    Make(MakeBuildSpec),
    CMake(CMakeBuildSpec),
    Command(CommandBuildSpec),
    LuaRock(String),
    // TODO: /// "cargo" (rust)?
    // Cargo,
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
    pub lua: HashMap<String, PathBuf>,
    /// Dynamic libraries implemented compiled Lua modules.
    #[serde(default)]
    pub lib: HashMap<String, PathBuf>,
    /// Configuration files.
    #[serde(default)]
    pub conf: HashMap<String, PathBuf>,
    /// Lua command-line scripts.
    #[serde(default)]
    pub bin: HashMap<String, PathBuf>,
}

fn deserialize_copy_directories<'de, D>(deserializer: D) -> Result<Option<Vec<PathBuf>>, D::Error>
where
    D: Deserializer<'de>,
{
    let copy_directories: Option<Vec<String>> = Option::deserialize(deserializer)?;
    let special_directories: Vec<String> = vec!["lua".into(), "lib".into(), "rock_manifest".into()];
    match special_directories
        .into_iter()
        .find(|dir| copy_directories.clone().unwrap_or_default().contains(dir))
    {
        // NOTE(mrcjkb): There also shouldn't be a directory named the same as the rockspec,
        // but I'm not sure how to (or if it makes sense to) enforce this here.
        Some(d) => Err(eyre!(
            "Directory '{}' in copy_directories clashes with the .rock format",
            d
        )),
        _ => Ok(copy_directories.map(|vec| vec.into_iter().map(PathBuf::from).collect())),
    }
    .map_err(de::Error::custom)
}

#[derive(Debug, PartialEq, Deserialize, Default, Clone)]
struct BuildSpecInternal {
    #[serde(rename = "type", default)]
    build_type: Option<BuildType>,
    #[serde(rename = "modules", default)]
    builtin_spec: Option<BuiltinBuildSpec>,
    #[serde(default)]
    makefile: Option<PathBuf>,
    #[serde(rename = "build_target", default)]
    make_build_target: Option<String>,
    #[serde(default)]
    make_build_pass: Option<bool>,
    #[serde(rename = "install_target", default)]
    make_install_target: Option<String>,
    #[serde(default)]
    make_install_pass: Option<bool>,
    #[serde(rename = "build_variables", default)]
    make_build_variables: Option<HashMap<String, String>>,
    #[serde(rename = "install_variables", default)]
    make_install_variables: Option<HashMap<String, String>>,
    #[serde(default)]
    variables: Option<HashMap<String, String>>,
    #[serde(rename = "cmake", default)]
    cmake_lists_content: Option<String>,
    #[serde(default)]
    build_command: Option<String>,
    #[serde(default)]
    install_command: Option<String>,
    #[serde(default)]
    install: Option<InstallSpec>,
    #[serde(default, deserialize_with = "deserialize_copy_directories")]
    copy_directories: Option<Vec<PathBuf>>,
    #[serde(default)]
    patches: Option<HashMap<PathBuf, String>>,
}

impl<'lua> FromLua<'lua> for PerPlatform<BuildSpecInternal> {
    fn from_lua(value: Value<'lua>, lua: &'lua Lua) -> mlua::Result<Self> {
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
                override_platform_specs(&mut per_platform, &default);
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
) {
    let per_platform_raw = per_platform.clone();
    for (platform, build_spec) in per_platform.clone() {
        // Add base dependencies for each platform
        per_platform.insert(platform, override_build_spec_internal(base, &build_spec));
    }
    for (platform, build_spec) in per_platform_raw {
        for extended_platform in &platform.get_extended_platforms() {
            let extended_spec = per_platform
                .get(extended_platform)
                .map(BuildSpecInternal::clone)
                .unwrap_or_default();
            per_platform.insert(
                *extended_platform,
                override_build_spec_internal(&extended_spec, &build_spec),
            );
        }
    }
}

fn override_build_spec_internal(
    base: &BuildSpecInternal,
    override_spec: &BuildSpecInternal,
) -> BuildSpecInternal {
    BuildSpecInternal {
        build_type: override_opt(&override_spec.build_type, &base.build_type),
        builtin_spec: match (
            override_spec.builtin_spec.clone(),
            base.builtin_spec.clone(),
        ) {
            (Some(override_val), Some(base_val)) => Some(BuiltinBuildSpec {
                modules: base_val
                    .modules
                    .into_iter()
                    .chain(override_val.modules)
                    .collect(),
            }),
            (override_val @ Some(_), _) => override_val,
            (_, base_val @ Some(_)) => base_val,
            _ => None,
        },
        makefile: override_opt(&override_spec.makefile, &base.makefile),
        make_build_target: override_opt(&override_spec.make_build_target, &base.make_build_target),
        make_build_pass: override_opt(&override_spec.make_build_pass, &base.make_build_pass),
        make_install_target: override_opt(
            &override_spec.make_install_target,
            &base.make_install_target,
        ),
        make_install_pass: override_opt(&override_spec.make_install_pass, &base.make_install_pass),
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
    }
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

#[derive(Debug, PartialEq, Deserialize, Clone)]
#[serde(rename_all = "lowercase", remote = "BuildType")]
enum BuildType {
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
    // TODO: /// "cargo" (rust)?
    // Cargo,
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

impl Default for BuildType {
    fn default() -> Self {
        Self::Builtin
    }
}

// TODO(vhyrro): Move this to the dedicated build.rs module
pub trait Build {
    fn run(self, rockspec: Rockspec, no_install: bool) -> Result<()>;
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
        let build_type: BuildType = serde_json::from_str("\"luarocks_build_rust_mlua\"").unwrap();
        assert_eq!(
            build_type,
            BuildType::LuaRock("luarocks_build_rust_mlua".into())
        );
    }
}
