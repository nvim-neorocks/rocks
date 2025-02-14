use std::{
    collections::HashMap,
    ops::{Deref, DerefMut},
    path::PathBuf,
};

use itertools::Itertools;
use serde::{Deserialize, Serialize};

use crate::{
    config::{Config, LuaVersion},
    lua_rockspec::{
        BuildSpec, ExternalDependencySpec, LuaVersionError, PerPlatform, PlatformSupport,
        RockDescription, RockSource, RockspecFormat, TestSpec,
    },
    package::{PackageName, PackageReq, PackageVersion},
};

/// A trait for querying information about a project from either a rockspec or `lux.toml` file.
pub trait Rockspec {
    fn package(&self) -> &PackageName;
    fn version(&self) -> &PackageVersion;
    fn description(&self) -> &RockDescription;
    fn supported_platforms(&self) -> &PlatformSupport;
    fn dependencies(&self) -> &PerPlatform<Vec<PackageReq>>;
    fn build_dependencies(&self) -> &PerPlatform<Vec<PackageReq>>;
    fn external_dependencies(&self) -> &PerPlatform<HashMap<String, ExternalDependencySpec>>;
    fn test_dependencies(&self) -> &PerPlatform<Vec<PackageReq>>;

    fn source(&self) -> &PerPlatform<RockSource>;
    fn build(&self) -> &PerPlatform<BuildSpec>;
    fn test(&self) -> &PerPlatform<TestSpec>;

    fn source_mut(&mut self) -> &mut PerPlatform<RockSource>;
    fn build_mut(&mut self) -> &mut PerPlatform<BuildSpec>;
    fn test_mut(&mut self) -> &mut PerPlatform<TestSpec>;

    fn to_rockspec_str(&self) -> String;

    fn format(&self) -> &Option<RockspecFormat>;

    /// Shorthand to extract the binaries that are part of the rockspec.
    fn binaries(&self) -> RockBinaries {
        RockBinaries(
            self.build()
                .current_platform()
                .install
                .bin
                .keys()
                .map_into()
                .collect(),
        )
    }
}

pub trait LuaVersionCompatibility {
    /// Ensures that the rockspec is compatible with the lua version established in the config.
    /// Returns an error if the rockspec is not compatible.
    fn validate_lua_version(&self, config: &Config) -> Result<(), LuaVersionError>;

    /// Ensures that the rockspec is compatible with the lua version established in the config,
    /// and returns the lua version from the config if it is compatible.
    fn lua_version_matches(&self, config: &Config) -> Result<LuaVersion, LuaVersionError>;

    /// Checks if the rockspec supports the given lua version.
    fn supports_lua_version(&self, lua_version: &LuaVersion) -> bool;

    /// Returns the lua version required by the rockspec.
    fn lua_version(&self) -> Option<LuaVersion>;

    /// Returns the lua version required by the rockspec's test dependencies.
    fn test_lua_version(&self) -> Option<LuaVersion>;
}

impl<T: Rockspec> LuaVersionCompatibility for T {
    fn validate_lua_version(&self, config: &Config) -> Result<(), LuaVersionError> {
        let _ = self.lua_version_matches(config)?;
        Ok(())
    }

    fn lua_version_matches(&self, config: &Config) -> Result<LuaVersion, LuaVersionError> {
        let version = LuaVersion::from(config)?;
        if self.supports_lua_version(&version) {
            Ok(version)
        } else {
            Err(LuaVersionError::LuaVersionUnsupported(
                version,
                self.package().to_owned(),
                self.version().to_owned(),
            ))
        }
    }

    fn supports_lua_version(&self, lua_version: &LuaVersion) -> bool {
        let lua_version_reqs = self
            .dependencies()
            .current_platform()
            .iter()
            .filter(|val| *val.name() == "lua".into())
            .collect_vec();
        let lua_pkg_version = lua_version.as_version();
        lua_version_reqs.is_empty()
            || lua_version_reqs
                .into_iter()
                .any(|lua| lua.version_req().matches(&lua_pkg_version))
    }

    fn lua_version(&self) -> Option<LuaVersion> {
        latest_lua_version(self.dependencies())
    }

    fn test_lua_version(&self) -> Option<LuaVersion> {
        latest_lua_version(self.test_dependencies()).or(self.lua_version())
    }
}

pub(crate) fn latest_lua_version(
    dependencies: &PerPlatform<Vec<PackageReq>>,
) -> Option<LuaVersion> {
    dependencies
        .current_platform()
        .iter()
        .find(|val| *val.name() == "lua".into())
        .and_then(|lua| {
            for (possibility, version) in [
                ("5.4.0", LuaVersion::Lua54),
                ("5.3.0", LuaVersion::Lua53),
                ("5.2.0", LuaVersion::Lua52),
                ("5.1.0", LuaVersion::Lua51),
            ] {
                if lua.version_req().matches(&possibility.parse().unwrap()) {
                    return Some(version);
                }
            }

            None
        })
}

#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialOrd, Ord, Hash, PartialEq, Eq)]
pub struct RockBinaries(Vec<PathBuf>);

impl Deref for RockBinaries {
    type Target = Vec<PathBuf>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for RockBinaries {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
