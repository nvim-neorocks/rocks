use std::{collections::HashMap, str::FromStr};

use eyre::{eyre, Result};
use itertools::Itertools;
use semver::VersionReq;
use serde::{de, Deserialize, Deserializer};

use crate::lua_package::{parse_version_req, LuaPackage, PackageName};

use super::{PartialOverride, PerPlatform, PlatformOverridable};

#[derive(Debug, Clone, PartialEq)]
pub struct LuaDependency {
    pub rock_name: PackageName,
    pub rock_version_req: VersionReq,
}

impl FromStr for LuaDependency {
    type Err = eyre::Error;

    fn from_str(str: &str) -> Result<Self> {
        let rock_name_str = str
            .chars()
            .peeking_take_while(|t| t.is_alphanumeric() || matches!(t, '-' | '_'))
            .collect::<String>();

        if rock_name_str.is_empty() {
            return Err(eyre!(
                "Could not parse dependency name from {}",
                str.to_string()
            ));
        }

        let constraints = str.trim_start_matches(&rock_name_str).trim();
        let rock_version_req = match constraints {
            "" => VersionReq::default(),
            constraints => parse_version_req(constraints.trim_start())?,
        };
        Ok(Self {
            rock_name: PackageName::new(rock_name_str),
            rock_version_req,
        })
    }
}

impl LuaDependency {
    pub fn parse(pkg_constraints: &String) -> Result<Self> {
        Self::from_str(&pkg_constraints.to_string())
    }

    pub fn matches(&self, rock: &LuaPackage) -> bool {
        if &self.rock_name == rock.name() {
            self.rock_version_req.matches(rock.version())
        } else {
            false
        }
    }
}

impl<'de> Deserialize<'de> for LuaDependency {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::from_str(&s).map_err(de::Error::custom)
    }
}

/// Override `base_deps` with `override_deps`
/// - Adds missing dependencies
/// - Replaces dependencies with the same name
impl PartialOverride for Vec<LuaDependency> {
    fn apply_overrides(&self, override_vec: &Self) -> Self {
        let mut result_map: HashMap<String, LuaDependency> = self
            .iter()
            .map(|dep| (dep.rock_name.clone().to_string(), dep.clone()))
            .collect();
        for override_dep in override_vec {
            result_map.insert(
                override_dep.rock_name.clone().to_string(),
                override_dep.clone(),
            );
        }
        result_map.into_values().collect()
    }
}

impl PlatformOverridable for Vec<LuaDependency> {
    fn on_nil<T>() -> Result<super::PerPlatform<T>>
    where
        T: PlatformOverridable,
        T: Default,
    {
        Ok(PerPlatform::default())
    }
}

/// Can be defined in a [platform-agnostic](https://github.com/luarocks/luarocks/wiki/platform-agnostic-external-dependencies) manner
#[derive(Debug, Deserialize, PartialEq, Clone)]
#[serde(rename_all = "lowercase")]
pub enum ExternalDependency {
    /// A header file, e.g. "foo.h"
    Header(String),
    /// A library file, e.g. "foo.lib"
    Library(String),
}

impl PartialOverride for HashMap<String, ExternalDependency> {
    fn apply_overrides(&self, override_map: &Self) -> Self {
        let mut result = Self::new();
        for (key, value) in self {
            result.insert(key.clone(), value.clone());
        }
        for (key, value) in override_map {
            result.insert(key.clone(), value.clone());
        }
        result
    }
}

impl PlatformOverridable for HashMap<String, ExternalDependency> {
    fn on_nil<T>() -> Result<super::PerPlatform<T>>
    where
        T: PlatformOverridable,
        T: Default,
    {
        Ok(PerPlatform::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use semver::Version;

    #[tokio::test]
    async fn parse_luarock() {
        let neorg = LuaPackage::new("neorg".into(), "1.0.0".into()).unwrap();
        let expected_version = Version::parse("1.0.0").unwrap();
        assert_eq!(neorg.name().to_string(), "neorg");
        assert_eq!(*neorg.version(), expected_version);
        let neorg = LuaPackage::new("neorg".into(), "1.0".into()).unwrap();
        assert_eq!(*neorg.version(), expected_version);
        let neorg = LuaPackage::new("neorg".into(), "1".into()).unwrap();
        assert_eq!(*neorg.version(), expected_version);
    }

    #[tokio::test]
    async fn parse_dependency() {
        let dep: LuaDependency = "lua >= 5.1".parse().unwrap();
        assert_eq!(dep.rock_name.to_string(), "lua");
        let dep: LuaDependency = "lua>=5.1".parse().unwrap();
        assert_eq!(dep.rock_name.to_string(), "lua");
        let dep: LuaDependency = "toml-edit >= 0.1.0".parse().unwrap();
        assert_eq!(dep.rock_name.to_string(), "toml-edit");
        let dep: LuaDependency = "lfs".parse().unwrap();
        assert_eq!(dep.rock_name.to_string(), "lfs");
        let dep: LuaDependency = "neorg 1.0.0".parse().unwrap();
        assert_eq!(dep.rock_name.to_string(), "neorg");
        let neorg = LuaPackage::new("neorg".into(), "1.0.0".into()).unwrap();
        assert!(dep.matches(&neorg));
        let neorg = LuaPackage::new("neorg".into(), "2.0.0".into()).unwrap();
        assert!(!dep.matches(&neorg));
        let dep: LuaDependency = "neorg 2.0.0".parse().unwrap();
        assert!(dep.matches(&neorg));
        let dep: LuaDependency = "neorg = 2.0.0".parse().unwrap();
        assert!(dep.matches(&neorg));
        let dep: LuaDependency = "neorg == 2.0.0".parse().unwrap();
        assert!(dep.matches(&neorg));
        let dep: LuaDependency = "neorg &equals; 2.0.0".parse().unwrap();
        assert!(dep.matches(&neorg));
        let dep: LuaDependency = "neorg >= 1.0, &lt; 2.0".parse().unwrap();
        let neorg = LuaPackage::new("neorg".into(), "1.5".into()).unwrap();
        assert!(dep.matches(&neorg));
        let dep: LuaDependency = "neorg &gt; 1.0, &lt; 2.0".parse().unwrap();
        let neorg = LuaPackage::new("neorg".into(), "1.11.0".into()).unwrap();
        assert!(dep.matches(&neorg));
        let neorg = LuaPackage::new("neorg".into(), "3.0.0".into()).unwrap();
        assert!(!dep.matches(&neorg));
        let neorg = LuaPackage::new("neorg".into(), "0.5".into()).unwrap();
        assert!(!dep.matches(&neorg));
        let dep: LuaDependency = "neorg ~> 1".parse().unwrap();
        assert!(!dep.matches(&neorg));
        let neorg = LuaPackage::new("neorg".into(), "3".into()).unwrap();
        assert!(!dep.matches(&neorg));
        let neorg = LuaPackage::new("neorg".into(), "1.5".into()).unwrap();
        assert!(dep.matches(&neorg));
        let dep: LuaDependency = "neorg ~> 1.4".parse().unwrap();
        let neorg = LuaPackage::new("neorg".into(), "1.3".into()).unwrap();
        assert!(!dep.matches(&neorg));
        let neorg = LuaPackage::new("neorg".into(), "1.5".into()).unwrap();
        assert!(!dep.matches(&neorg));
        let neorg = LuaPackage::new("neorg".into(), "1.4.10".into()).unwrap();
        assert!(dep.matches(&neorg));
        let neorg = LuaPackage::new("neorg".into(), "1.4".into()).unwrap();
        assert!(dep.matches(&neorg));
        let dep: LuaDependency = "neorg ~> 1.0.5".parse().unwrap();
        let neorg = LuaPackage::new("neorg".into(), "1.0.4".into()).unwrap();
        assert!(!dep.matches(&neorg));
        let neorg = LuaPackage::new("neorg".into(), "1.0.5".into()).unwrap();
        assert!(dep.matches(&neorg));
        let neorg = LuaPackage::new("neorg".into(), "1.0.6".into()).unwrap();
        assert!(!dep.matches(&neorg));
    }

    #[tokio::test]
    async fn test_override_deps() {
        let neorg_a: LuaDependency = "neorg 1.0.0".parse().unwrap();
        let neorg_b: LuaDependency = "neorg 2.0.0".parse().unwrap();
        let foo: LuaDependency = "foo 1.0.0".parse().unwrap();
        let bar: LuaDependency = "bar 1.0.0".parse().unwrap();
        let base_vec = vec![neorg_a, foo.clone()];
        let override_vec = vec![neorg_b.clone(), bar.clone()];
        let result = base_vec.apply_overrides(&override_vec);
        assert_eq!(result.clone().len(), 3);
        assert_eq!(
            result
                .into_iter()
                .filter(|dep| *dep == neorg_b || *dep == foo || *dep == bar)
                .count(),
            3
        );
    }
}
