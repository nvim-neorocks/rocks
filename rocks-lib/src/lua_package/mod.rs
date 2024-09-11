use eyre::Result;
use semver::Version;
use serde::{Deserialize, Serialize};
use std::fmt::Display;

mod version;

pub use version::{parse_version, parse_version_req};

// TODO: We probably need a better name for this
pub struct LuaPackage {
    name: PackageName,
    version: Version,
}

impl LuaPackage {
    pub fn new(name: String, version: String) -> Result<Self> {
        Ok(Self {
            name: PackageName::new(name),
            version: parse_version(&version)?,
        })
    }
    pub fn name(&self) -> &PackageName {
        &self.name
    }
    pub fn version(&self) -> &Version {
        &self.version
    }
}

/// A luarocks package name, which is always lowercase
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Hash)]
pub struct PackageName {
    name: String,
}

impl PackageName {
    pub fn new(name: String) -> Self {
        Self {
            // TODO: validations?
            name: name.to_lowercase(),
        }
    }
}

impl<'de> Deserialize<'de> for PackageName {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(PackageName::new(String::deserialize(deserializer)?))
    }
}

impl Serialize for PackageName {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.name.serialize(serializer)
    }
}

impl From<&str> for PackageName {
    fn from(value: &str) -> Self {
        Self::new(value.into())
    }
}

impl Display for PackageName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn parse_name() {
        let mut package_name: PackageName = "neorg".into();
        assert_eq!(package_name.to_string(), "neorg");
        package_name = "LuaFileSystem".into();
        assert_eq!(package_name.to_string(), "luafilesystem");
    }
}
