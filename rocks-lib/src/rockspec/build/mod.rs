mod builtin;
mod cmake;
mod make;

pub use builtin::*;
pub use cmake::*;
pub use make::*;

use eyre::eyre;
use std::{collections::HashMap, path::PathBuf};

use serde::{de, de::IntoDeserializer, Deserialize, Deserializer};

#[derive(Debug, PartialEq, Default)]
pub struct BuildSpec {
    pub build_backend: Option<BuildBackendSpec>,
    pub install: InstallSpec,
    pub copy_directories: Vec<PathBuf>,
    pub patches: HashMap<PathBuf, String>,
}

impl<'de> Deserialize<'de> for BuildSpec {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let internal = BuildSpecInternal::deserialize(deserializer).map_err(de::Error::custom)?;
        let build_backend = match internal.build_type {
            BuildType::Builtin => Some(BuildBackendSpec::Builtin(
                internal.builtin_spec.unwrap_or_default(),
            )),
            BuildType::Make => {
                let default = MakeBuildSpec::default();
                Some(BuildBackendSpec::Make(MakeBuildSpec {
                    makefile: internal.makefile.unwrap_or(default.makefile),
                    build_target: internal.make_build_target,
                    build_pass: internal.make_build_pass.unwrap_or(default.build_pass),
                    install_target: internal
                        .make_install_target
                        .unwrap_or(default.install_target),
                    install_pass: internal.make_install_pass.unwrap_or(default.install_pass),
                    build_variables: internal.make_build_variables,
                    install_variables: internal.make_install_variables,
                    variables: internal.variables,
                }))
            }
            BuildType::CMake => Some(BuildBackendSpec::CMake(CMakeBuildSpec {
                cmake_lists_content: internal.cmake_lists_content,
                variables: internal.variables,
            })),
            BuildType::Command => todo!(),
            BuildType::None => None,
            BuildType::LuaRock(s) => Some(BuildBackendSpec::LuaRock(s)),
        };
        Ok(Self {
            build_backend,
            install: internal.install,
            copy_directories: internal.copy_directories,
            patches: internal.patches,
        })
    }
}

impl Default for BuildBackendSpec {
    fn default() -> Self {
        Self::Builtin(BuiltinBuildSpec::default())
    }
}

#[derive(Debug, PartialEq)]
pub enum BuildBackendSpec {
    Builtin(BuiltinBuildSpec),
    Make(MakeBuildSpec),
    CMake(CMakeBuildSpec),
    Command,
    LuaRock(String),
    // TODO: /// "cargo" (rust)?
    // Cargo,
}

/// For packages which don't provide means to install modules
/// and expect the user to copy the .lua or library files by hand to the proper locations.
/// This struct contains categories of files. Each category is itself a table,
/// where the array part is a list of filenames to be copied.
/// For module directories only, in the hash part, other keys are identifiers in Lua module format,
/// to indicate which subdirectory the file should be copied to.
/// For example, build.install.lua = {["foo.bar"] = {"src/bar.lua"}} will copy src/bar.lua
/// to the foo directory under the rock's Lua files directory.
#[derive(Debug, PartialEq, Default, Deserialize)]
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

fn deserialize_copy_directories<'de, D>(deserializer: D) -> Result<Vec<PathBuf>, D::Error>
where
    D: Deserializer<'de>,
{
    let copy_directories: Vec<String> = Vec::deserialize(deserializer)?;
    let special_directories: Vec<String> = vec!["lua".into(), "lib".into(), "rock_manifest".into()];
    match special_directories
        .into_iter()
        .find(|dir| copy_directories.contains(&dir))
    {
        // NOTE(mrcjkb): There also shouldn't be a directory named the same as the rockspec,
        // but I'm not sure how to (or if it makes sense to) enforce this here.
        Some(d) => Err(eyre!(
            "Directory '{}' in copy_directories clashes with the .rock format",
            d
        )),
        _ => Ok(copy_directories.into_iter().map(PathBuf::from).collect()),
    }
    .map_err(de::Error::custom)
}

#[derive(Debug, PartialEq, Deserialize, Default)]
struct BuildSpecInternal {
    #[serde(rename = "type", default)]
    build_type: BuildType,
    #[serde(rename = "modules", default)]
    builtin_spec: Option<BuiltinBuildSpec>,
    #[serde(default)]
    makefile: Option<PathBuf>,
    #[serde(rename = "build_target", default)]
    make_build_target: String,
    #[serde(default)]
    make_build_pass: Option<bool>,
    #[serde(rename = "install_target", default)]
    make_install_target: Option<String>,
    #[serde(default)]
    make_install_pass: Option<bool>,
    #[serde(rename = "build_variables", default)]
    make_build_variables: HashMap<String, String>,
    #[serde(rename = "install_variables", default)]
    make_install_variables: HashMap<String, String>,
    #[serde(default)]
    variables: HashMap<String, String>,
    #[serde(rename = "cmake", default)]
    cmake_lists_content: Option<String>,
    #[serde(default)]
    install: InstallSpec,
    #[serde(default, deserialize_with = "deserialize_copy_directories")]
    copy_directories: Vec<PathBuf>,
    #[serde(default)]
    patches: HashMap<PathBuf, String>,
}

#[derive(Debug, PartialEq, Deserialize)]
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

#[cfg(test)]
mod tests {

    use super::*;

    #[tokio::test]
    pub async fn deserialize_build_type() {
        let build_type: BuildType = serde_json::from_str("\"builtin\"".into()).unwrap();
        assert_eq!(build_type, BuildType::Builtin);
        let build_type: BuildType = serde_json::from_str("\"module\"".into()).unwrap();
        assert_eq!(build_type, BuildType::Builtin);
        let build_type: BuildType = serde_json::from_str("\"make\"".into()).unwrap();
        assert_eq!(build_type, BuildType::Make);
        let build_type: BuildType =
            serde_json::from_str("\"luarocks_build_rust_mlua\"".into()).unwrap();
        assert_eq!(
            build_type,
            BuildType::LuaRock("luarocks_build_rust_mlua".into())
        );
    }
}
