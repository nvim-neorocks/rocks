use eyre::eyre;
use std::{collections::HashMap, path::PathBuf};

use serde::{de, de::IntoDeserializer, Deserialize, Deserializer};

#[derive(Debug, PartialEq, Deserialize, Default)]
pub struct BuildSpec {
    #[serde(rename = "type", default)]
    pub build_type: BuildType,
    #[serde(default)]
    pub install: InstallSpec,
    #[serde(default, deserialize_with = "deserialize_copy_directories")]
    pub copy_directories: Vec<PathBuf>,
    #[serde(default)]
    pub patches: HashMap<PathBuf, String>,
}

#[derive(Debug, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase", remote = "BuildType")]
pub enum BuildType {
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
    /// "cargo" (rust)
    Cargo,
    /// external Lua rock
    LuaRock(String),
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
