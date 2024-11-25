use itertools::Itertools;
use mlua::{Lua, LuaSerdeExt};
use std::collections::HashMap;
use thiserror::Error;

use crate::{
    config::Config,
    package::{PackageName, PackageReq, PackageVersion, RemotePackage},
};

use super::ManifestFromServerError;

#[derive(Clone)]
pub struct ManifestMetadata {
    pub repository: HashMap<PackageName, HashMap<PackageVersion, Vec<ManifestRockEntry>>>,
}

impl<'de> serde::Deserialize<'de> for ManifestMetadata {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let intermediate = IntermediateManifest::deserialize(deserializer)?;
        Ok(from_intermediate(intermediate))
    }
}

#[derive(Error, Debug)]
#[error("failed to parse manifest: {0}")]
pub struct ManifestLuaError(#[from] mlua::Error);

#[derive(Error, Debug)]
#[error("failed to parse manifest from configuration: {0}")]
pub enum ManifestError {
    Lua(#[from] ManifestLuaError),
    Server(#[from] ManifestFromServerError),
}

impl ManifestMetadata {
    // TODO(vhyrro): Perhaps make these functions return a cached, in-memory version of the
    // manifest if it has already been parsed?
    pub fn new(manifest: &String) -> Result<Self, ManifestLuaError> {
        let lua = Lua::new();

        lua.load(manifest).exec()?;

        let intermediate = IntermediateManifest {
            repository: lua.from_value(lua.globals().get("repository")?)?,
        };
        let manifest = from_intermediate(intermediate);

        Ok(manifest)
    }

    pub async fn from_config(config: &Config) -> Result<Self, ManifestError> {
        let manifest =
            crate::manifest::manifest_from_server(config.server().clone(), config).await?;

        Ok(Self::new(&manifest)?)
    }

    pub fn has_rock(&self, rock_name: &PackageName) -> bool {
        self.repository.contains_key(rock_name)
    }

    pub fn available_versions(&self, rock_name: &PackageName) -> Option<Vec<&PackageVersion>> {
        if !self.has_rock(rock_name) {
            return None;
        }

        Some(self.repository[rock_name].keys().collect())
    }

    pub fn latest_version(&self, rock_name: &PackageName) -> Option<&PackageVersion> {
        if !self.has_rock(rock_name) {
            return None;
        }

        self.repository[rock_name].keys().sorted().last()
    }

    pub fn latest_match(&self, lua_package_req: &PackageReq) -> Option<RemotePackage> {
        if !self.has_rock(lua_package_req.name()) {
            return None;
        }

        let version = self.repository[lua_package_req.name()]
            .keys()
            .sorted()
            .rev()
            .find(|version| lua_package_req.version_req().matches(version))?;

        Some(RemotePackage::new(
            lua_package_req.name().to_owned(),
            version.to_owned(),
        ))
    }
}

#[derive(Clone, serde::Deserialize)]
pub struct ManifestRockEntry {
    /// e.g. "linux-x86_64", "rockspec", "src", ...
    pub arch: String,
}

/// Intermediate implementation for deserializing
#[derive(serde::Deserialize)]
struct IntermediateManifest {
    /// The key of each package's HashMap is the version string
    repository: HashMap<PackageName, HashMap<String, Vec<ManifestRockEntry>>>,
}

/// Construct a `ManifestMetadata` from an intermediate representation,
/// silently skipping entries for versions we don't know how to parse.
fn from_intermediate(intermediate: IntermediateManifest) -> ManifestMetadata {
    let repository = intermediate
        .repository
        .into_iter()
        .map(|(name, package_map)| {
            (
                name,
                package_map
                    .into_iter()
                    .filter_map(|(version_str, entries)| {
                        let version = PackageVersion::parse(version_str.as_str()).ok()?;
                        Some((version, entries))
                    })
                    .collect(),
            )
        })
        .collect();
    ManifestMetadata { repository }
}
