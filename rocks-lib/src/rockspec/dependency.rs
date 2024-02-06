use std::{collections::HashMap, str::FromStr};

use eyre::{eyre, Result};
use html_escape::decode_html_entities;
use mlua::{FromLua, Lua, LuaSerdeExt, Value};
use semver::{Version, VersionReq};
use serde::{de, Deserialize, Deserializer};

use super::{PerPlatform, PlatformIdentifier};

#[derive(Debug, Clone, PartialEq)]
pub struct LuaDependency {
    rock_name: String,
    rock_version_req: VersionReq,
}

impl FromStr for LuaDependency {
    type Err = eyre::Error;

    fn from_str(str: &str) -> Result<Self> {
        let rock_name = str
            .split_whitespace()
            .next()
            .map(|str| str.to_string())
            .ok_or(eyre!(
                "Could not parse dependency name from {}",
                str.to_string()
            ))?;
        let constraints = str.trim_start_matches(&rock_name);
        let rock_version_req = match constraints {
            "" => VersionReq::default(),
            constraints => parse_version_req(constraints.trim_start())?,
        };
        Ok(Self {
            rock_name,
            rock_version_req,
        })
    }
}

impl LuaDependency {
    pub fn parse(pkg_constraints: &String) -> Result<Self> {
        Self::from_str(&pkg_constraints.to_string())
    }

    pub fn matches(&self, rock: &LuaRock) -> bool {
        if self.rock_name != rock.name {
            return false;
        }
        self.rock_version_req.matches(&rock.version)
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

impl<'lua> FromLua<'lua> for PerPlatform<Vec<LuaDependency>> {
    fn from_lua(value: Value<'lua>, lua: &'lua Lua) -> mlua::Result<Self> {
        match &value {
            list @ Value::Table(tbl) => {
                let mut per_platform = match tbl.get("platforms")? {
                    val @ Value::Table(_) => Ok(lua.from_value(val)?),
                    Value::Nil => Ok(HashMap::default()),
                    val => Err(mlua::Error::DeserializeError(format!(
                        "Expected dependencies to be a table or nil, but got {}",
                        val.type_name()
                    ))),
                }?;
                let _ = tbl.raw_remove("platforms");
                let default = lua.from_value(list.clone())?;
                // TODO: Extract this to a trait?
                override_platform_deps(&mut per_platform, &default);
                Ok(PerPlatform {
                    default,
                    per_platform,
                })
            }
            Value::Nil => Ok(PerPlatform::default()),
            val => Err(mlua::Error::DeserializeError(format!(
                "Expected dependencies to be a table or nil, but got {}",
                val.type_name()
            ))),
        }
    }
}

/// For each platform in `per_platform`, add the base dependencies,
/// and apply overrides to the extended platforms of each platform override.
fn override_platform_deps(
    per_platform: &mut HashMap<PlatformIdentifier, Vec<LuaDependency>>,
    base: &Vec<LuaDependency>,
) {
    let per_platform_raw = per_platform.clone();
    for (platform, dependencies) in per_platform.clone() {
        // Add base dependencies for each platform
        per_platform.insert(platform, override_deps(&base, &dependencies));
    }
    for (platform, dependencies) in per_platform_raw {
        // Add extended platform dependencies (without base deps) for each platform
        for extended_platform in &platform.get_extended_platforms() {
            let extended_dependencies = per_platform
                .get(extended_platform)
                .map(Vec::clone)
                .unwrap_or_default();
            per_platform.insert(
                *extended_platform,
                override_deps(&extended_dependencies, &dependencies),
            );
        }
    }
}

/// Override `base_deps` with `override_deps`
/// - Adds missing dependencies
/// - Replaces dependencies with the same name
fn override_deps(
    base_vec: &Vec<LuaDependency>,
    override_vec: &Vec<LuaDependency>,
) -> Vec<LuaDependency> {
    let mut result_map: HashMap<String, LuaDependency> = base_vec
        .into_iter()
        .map(|dep| (dep.rock_name.clone(), dep.clone()))
        .collect();
    for override_dep in override_vec {
        result_map.insert(override_dep.rock_name.clone(), override_dep.clone());
    }
    result_map.into_values().collect()
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

impl<'lua> FromLua<'lua> for PerPlatform<HashMap<String, ExternalDependency>> {
    // TODO: Extract shared logic between ExtendalDependency map and LuaDependency list?
    fn from_lua(value: Value<'lua>, lua: &'lua Lua) -> mlua::Result<Self> {
        match &value {
            list @ Value::Table(tbl) => {
                let mut per_platform = match tbl.get("platforms")? {
                    val @ Value::Table(_) => Ok(lua.from_value(val)?),
                    Value::Nil => Ok(HashMap::default()),
                    val => Err(mlua::Error::DeserializeError(format!(
                        "Expected external dependencies to be a table or nil, but got {}",
                        val.type_name()
                    ))),
                }?;
                let _ = tbl.raw_remove("platforms");
                let default = lua.from_value(list.clone())?;
                override_platform_external_deps(&mut per_platform, &default);
                Ok(PerPlatform {
                    default,
                    per_platform,
                })
            }
            Value::Nil => Ok(PerPlatform::default()),
            val => Err(mlua::Error::DeserializeError(format!(
                "Expected rockspec external dependencies to be a table or nil, but got {}",
                val.type_name()
            ))),
        }
    }
}

fn override_platform_external_deps(
    per_platform: &mut HashMap<PlatformIdentifier, HashMap<String, ExternalDependency>>,
    base: &HashMap<String, ExternalDependency>,
) {
    let per_platform_raw = per_platform.clone();
    for (platform, dependencies) in per_platform.clone() {
        // Add base dependencies for each platform
        per_platform.insert(platform, override_external_deps(&base, &dependencies));
    }
    for (platform, dependencies) in per_platform_raw {
        // Add extended platform dependencies (without base deps) for each platform
        for extended_platform in &platform.get_extended_platforms() {
            let extended_dependencies = per_platform
                .get(extended_platform)
                .map(HashMap::clone)
                .unwrap_or_default();
            per_platform.insert(
                *extended_platform,
                override_external_deps(&extended_dependencies, &dependencies),
            );
        }
    }
}

fn override_external_deps(
    base_map: &HashMap<String, ExternalDependency>,
    override_map: &HashMap<String, ExternalDependency>,
) -> HashMap<String, ExternalDependency> {
    let mut result = HashMap::new();
    for (key, value) in base_map {
        result.insert(key.clone(), value.clone());
    }
    for (key, value) in override_map {
        result.insert(key.clone(), value.clone());
    }
    result
}

// TODO(mrcjkb): Move this somewhere more suitable
pub struct LuaRock {
    pub name: String,
    pub version: Version,
}

impl LuaRock {
    pub fn new(name: String, version: String) -> Result<Self> {
        Ok(Self {
            name,
            version: Version::parse(append_minor_patch_if_missing(version).as_str())?,
        })
    }
}

/// Transform LuaRocks constraints into constraints that can be parsed by the semver crate.
fn parse_version_req(version_constraints: &str) -> Result<VersionReq> {
    let unescaped = decode_html_entities(version_constraints)
        .to_string()
        .as_str()
        .to_owned();
    let transformed = match unescaped {
        s if s.starts_with("~>") => parse_pessimistic_version_constraint(s)?,
        s => s,
    };
    let version_req = VersionReq::parse(&transformed)?;
    Ok(version_req)
}

fn parse_pessimistic_version_constraint(version_constraint: String) -> Result<String> {
    // pessimistic operator
    let min_version_str = &version_constraint[2..].trim();
    let min_version =
        Version::parse(append_minor_patch_if_missing(min_version_str.to_string()).as_str())?;
    let max_version = match min_version_str.matches(".").count() {
        0 => Version {
            major: &min_version.major + 1,
            ..min_version.clone()
        },
        1 => Version {
            minor: &min_version.minor + 1,
            ..min_version.clone()
        },
        _ => Version {
            patch: &min_version.patch + 1,
            ..min_version.clone()
        },
    };
    Ok(">= ".to_string()
        + &min_version.to_string()
        + &", < ".to_string()
        + &max_version.to_string())
}

/// Recursively append .0 until the version string has a minor or patch version
fn append_minor_patch_if_missing(version: String) -> String {
    if version.matches(".").count() < 2 {
        return append_minor_patch_if_missing(version + ".0");
    }
    version
}

#[cfg(test)]
mod tests {

    use super::*;

    #[tokio::test]
    async fn parse_luarock() {
        let neorg = LuaRock::new("neorg".into(), "1.0.0".into()).unwrap();
        let expected_version = Version::parse("1.0.0").unwrap();
        assert_eq!(neorg.name, "neorg");
        assert_eq!(neorg.version, expected_version);
        let neorg = LuaRock::new("neorg".into(), "1.0".into()).unwrap();
        assert_eq!(neorg.version, expected_version);
        let neorg = LuaRock::new("neorg".into(), "1".into()).unwrap();
        assert_eq!(neorg.version, expected_version);
    }

    #[tokio::test]
    async fn parse_dependency() {
        let dep: LuaDependency = "lfs".parse().unwrap();
        assert_eq!(dep.rock_name, "lfs");
        let dep: LuaDependency = "neorg 1.0.0".parse().unwrap();
        assert_eq!(dep.rock_name, "neorg");
        let neorg = LuaRock::new("neorg".into(), "1.0.0".into()).unwrap();
        assert!(dep.matches(&neorg));
        let neorg = LuaRock::new("neorg".into(), "2.0.0".into()).unwrap();
        assert!(!dep.matches(&neorg));
        let dep: LuaDependency = "neorg 2.0.0".parse().unwrap();
        assert!(dep.matches(&neorg));
        let dep: LuaDependency = "neorg >= 1.0, &lt; 2.0".parse().unwrap();
        let neorg = LuaRock::new("neorg".into(), "1.5".into()).unwrap();
        assert!(dep.matches(&neorg));
        let dep: LuaDependency = "neorg &gt; 1.0, &lt; 2.0".parse().unwrap();
        let neorg = LuaRock::new("neorg".into(), "1.11.0".into()).unwrap();
        assert!(dep.matches(&neorg));
        let neorg = LuaRock::new("neorg".into(), "3.0.0".into()).unwrap();
        assert!(!dep.matches(&neorg));
        let neorg = LuaRock::new("neorg".into(), "0.5".into()).unwrap();
        assert!(!dep.matches(&neorg));
        let dep: LuaDependency = "neorg ~> 1".parse().unwrap();
        assert!(!dep.matches(&neorg));
        let neorg = LuaRock::new("neorg".into(), "3".into()).unwrap();
        assert!(!dep.matches(&neorg));
        let neorg = LuaRock::new("neorg".into(), "1.5".into()).unwrap();
        assert!(dep.matches(&neorg));
        let dep: LuaDependency = "neorg ~> 1.4".parse().unwrap();
        let neorg = LuaRock::new("neorg".into(), "1.3".into()).unwrap();
        assert!(!dep.matches(&neorg));
        let neorg = LuaRock::new("neorg".into(), "1.5".into()).unwrap();
        assert!(!dep.matches(&neorg));
        let neorg = LuaRock::new("neorg".into(), "1.4.10".into()).unwrap();
        assert!(dep.matches(&neorg));
        let neorg = LuaRock::new("neorg".into(), "1.4".into()).unwrap();
        assert!(dep.matches(&neorg));
        let dep: LuaDependency = "neorg ~> 1.0.5".parse().unwrap();
        let neorg = LuaRock::new("neorg".into(), "1.0.4".into()).unwrap();
        assert!(!dep.matches(&neorg));
        let neorg = LuaRock::new("neorg".into(), "1.0.5".into()).unwrap();
        assert!(dep.matches(&neorg));
        let neorg = LuaRock::new("neorg".into(), "1.0.6".into()).unwrap();
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
        let result = override_deps(&base_vec, &override_vec);
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
