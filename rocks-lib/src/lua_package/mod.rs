use eyre::{eyre, Result};
use itertools::Itertools;
use mlua::FromLua;
use serde::{de, Deserialize, Deserializer, Serialize};
use std::{fmt::Display, str::FromStr};
pub use version::{PackageVersion, PackageVersionReq};

mod outdated;
mod version;

pub struct LuaPackage {
    name: PackageName,
    version: PackageVersion,
}

impl LuaPackage {
    pub fn new(name: PackageName, version: PackageVersion) -> Self {
        Self { name, version }
    }
    pub fn parse(name: String, version: String) -> Result<Self> {
        Ok(Self::new(
            PackageName::new(name),
            PackageVersion::parse(&version)?,
        ))
    }
    pub fn name(&self) -> &PackageName {
        &self.name
    }
    pub fn version(&self) -> &PackageVersion {
        &self.version
    }
    pub fn as_package_req(&self) -> LuaPackageReq {
        LuaPackageReq {
            name: self.name.clone(),
            version_req: self.version.clone().into(),
        }
    }
}

/// A lua package requirement with a name and an optional version requirement.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
pub struct LuaPackageReq {
    /// The name of the package.
    name: PackageName,
    /// The version requirement, for example "1.0.0" or ">=1.0.0".
    #[cfg_attr(feature = "clap", clap(default_value_t = PackageVersionReq::default()))]
    version_req: PackageVersionReq,
}

impl LuaPackageReq {
    pub fn new(name: String, version: Option<String>) -> Result<Self> {
        Ok(Self {
            name: PackageName::new(name),
            version_req: match version {
                Some(version_req_str) => PackageVersionReq::parse(version_req_str.as_str())?,
                None => PackageVersionReq::default(),
            },
        })
    }
    pub fn parse(pkg_constraints: &String) -> Result<Self> {
        Self::from_str(&pkg_constraints.to_string())
    }
    pub fn name(&self) -> &PackageName {
        &self.name
    }
    pub fn version_req(&self) -> &PackageVersionReq {
        &self.version_req
    }
    /// Evaluate whether the given package satisfies the package requirement
    /// given by `self`.
    pub fn matches(&self, package: &LuaPackage) -> bool {
        self.name == package.name && self.version_req.matches(&package.version)
    }
}

impl FromStr for LuaPackageReq {
    type Err = eyre::Error;

    fn from_str(str: &str) -> Result<Self> {
        let rock_name_str = str
            .chars()
            .peeking_take_while(|t| t.is_alphanumeric() || matches!(t, '-' | '_' | '.'))
            .collect::<String>();

        if rock_name_str.is_empty() {
            return Err(eyre!(
                "Could not parse dependency name from {}",
                str.to_string()
            ));
        }

        let constraints = str.trim_start_matches(&rock_name_str).trim();
        let version_req = match constraints {
            "" => PackageVersionReq::default(),
            constraints => PackageVersionReq::parse(constraints.trim_start())?,
        };
        Ok(Self {
            name: PackageName::new(rock_name_str),
            version_req,
        })
    }
}

impl<'de> Deserialize<'de> for LuaPackageReq {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::from_str(&s).map_err(de::Error::custom)
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

impl<'lua> FromLua<'lua> for PackageName {
    fn from_lua(
        value: mlua::prelude::LuaValue<'lua>,
        lua: &'lua mlua::prelude::Lua,
    ) -> mlua::prelude::LuaResult<Self> {
        Ok(Self::new(String::from_lua(value, lua)?))
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

    #[tokio::test]
    async fn parse_lua_package() {
        let neorg = LuaPackage::parse("neorg".into(), "1.0.0".into()).unwrap();
        let expected_version = PackageVersion::parse("1.0.0").unwrap();
        assert_eq!(neorg.name().to_string(), "neorg");
        assert_eq!(*neorg.version(), expected_version);
        let neorg = LuaPackage::parse("neorg".into(), "1.0".into()).unwrap();
        assert_eq!(*neorg.version(), expected_version);
        let neorg = LuaPackage::parse("neorg".into(), "1".into()).unwrap();
        assert_eq!(*neorg.version(), expected_version);
    }

    #[tokio::test]
    async fn parse_lua_package_req() {
        let mut package_req = LuaPackageReq::new("foo".into(), Some("1.0.0".into())).unwrap();
        assert!(package_req.matches(&LuaPackage::parse("foo".into(), "1.0.0".into()).unwrap()));
        assert!(!package_req.matches(&LuaPackage::parse("bar".into(), "1.0.0".into()).unwrap()));
        assert!(!package_req.matches(&LuaPackage::parse("foo".into(), "2.0.0".into()).unwrap()));
        package_req = LuaPackageReq::new("foo".into(), Some(">= 1.0.0".into())).unwrap();
        assert!(package_req.matches(&LuaPackage::parse("foo".into(), "2.0.0".into()).unwrap()));
        let package_req: LuaPackageReq = "lua >= 5.1".parse().unwrap();
        assert_eq!(package_req.name.to_string(), "lua");
        let package_req: LuaPackageReq = "lua>=5.1".parse().unwrap();
        assert_eq!(package_req.name.to_string(), "lua");
        let package_req: LuaPackageReq = "toml-edit >= 0.1.0".parse().unwrap();
        assert_eq!(package_req.name.to_string(), "toml-edit");
        let package_req: LuaPackageReq = "plugin.nvim >= 0.1.0".parse().unwrap();
        assert_eq!(package_req.name.to_string(), "plugin.nvim");
        let package_req: LuaPackageReq = "lfs".parse().unwrap();
        assert_eq!(package_req.name.to_string(), "lfs");
        let package_req: LuaPackageReq = "neorg 1.0.0".parse().unwrap();
        assert_eq!(package_req.name.to_string(), "neorg");
        let neorg = LuaPackage::parse("neorg".into(), "1.0.0".into()).unwrap();
        assert!(package_req.matches(&neorg));
        let neorg = LuaPackage::parse("neorg".into(), "2.0.0".into()).unwrap();
        assert!(!package_req.matches(&neorg));
        let package_req: LuaPackageReq = "neorg 2.0.0".parse().unwrap();
        assert!(package_req.matches(&neorg));
        let package_req: LuaPackageReq = "neorg = 2.0.0".parse().unwrap();
        assert!(package_req.matches(&neorg));
        let package_req: LuaPackageReq = "neorg == 2.0.0".parse().unwrap();
        assert!(package_req.matches(&neorg));
        let package_req: LuaPackageReq = "neorg &equals; 2.0.0".parse().unwrap();
        assert!(package_req.matches(&neorg));
        let package_req: LuaPackageReq = "neorg >= 1.0, &lt; 2.0".parse().unwrap();
        let neorg = LuaPackage::parse("neorg".into(), "1.5".into()).unwrap();
        assert!(package_req.matches(&neorg));
        let package_req: LuaPackageReq = "neorg &gt; 1.0, &lt; 2.0".parse().unwrap();
        let neorg = LuaPackage::parse("neorg".into(), "1.11.0".into()).unwrap();
        assert!(package_req.matches(&neorg));
        let neorg = LuaPackage::parse("neorg".into(), "3.0.0".into()).unwrap();
        assert!(!package_req.matches(&neorg));
        let neorg = LuaPackage::parse("neorg".into(), "0.5".into()).unwrap();
        assert!(!package_req.matches(&neorg));
        let package_req: LuaPackageReq = "neorg ~> 1".parse().unwrap();
        assert!(!package_req.matches(&neorg));
        let neorg = LuaPackage::parse("neorg".into(), "3".into()).unwrap();
        assert!(!package_req.matches(&neorg));
        let neorg = LuaPackage::parse("neorg".into(), "1.5".into()).unwrap();
        assert!(package_req.matches(&neorg));
        let package_req: LuaPackageReq = "neorg ~> 1.4".parse().unwrap();
        let neorg = LuaPackage::parse("neorg".into(), "1.3".into()).unwrap();
        assert!(!package_req.matches(&neorg));
        let neorg = LuaPackage::parse("neorg".into(), "1.5".into()).unwrap();
        assert!(!package_req.matches(&neorg));
        let neorg = LuaPackage::parse("neorg".into(), "1.4.10".into()).unwrap();
        assert!(package_req.matches(&neorg));
        let neorg = LuaPackage::parse("neorg".into(), "1.4".into()).unwrap();
        assert!(package_req.matches(&neorg));
        let package_req: LuaPackageReq = "neorg ~> 1.0.5".parse().unwrap();
        let neorg = LuaPackage::parse("neorg".into(), "1.0.4".into()).unwrap();
        assert!(!package_req.matches(&neorg));
        let neorg = LuaPackage::parse("neorg".into(), "1.0.5".into()).unwrap();
        assert!(package_req.matches(&neorg));
        let neorg = LuaPackage::parse("neorg".into(), "1.0.6".into()).unwrap();
        assert!(!package_req.matches(&neorg));
    }
}
