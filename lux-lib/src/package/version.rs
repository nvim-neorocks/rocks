use std::{
    cmp::{self, Ordering},
    fmt::Display,
    str::FromStr,
};

use html_escape::decode_html_entities;
use itertools::Itertools;
use mlua::{ExternalResult, FromLua, IntoLua};
use semver::{Comparator, Error, Op, Version, VersionReq};
use serde::{de, Deserialize, Deserializer, Serialize};
use thiserror::Error;

/// **SemVer version** as defined by <https://semver.org>.
/// or a **Dev** version, which can be one of "dev", "scm", or "git"
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub enum PackageVersion {
    SemVer(SemVer),
    DevVer(DevVer),
}

impl IntoLua for PackageVersion {
    fn into_lua(self, lua: &mlua::Lua) -> mlua::Result<mlua::Value> {
        self.to_string().into_lua(lua)
    }
}

impl PackageVersion {
    pub fn parse(text: &str) -> Result<Self, PackageVersionParseError> {
        PackageVersion::from_str(text)
    }
    /// Note that this loses the specrev information.
    pub fn into_version_req(&self) -> PackageVersionReq {
        match self {
            PackageVersion::DevVer(DevVer { modrev, .. }) => {
                PackageVersionReq::Dev(modrev.to_owned())
            }
            PackageVersion::SemVer(SemVer { version, .. }) => {
                let version = version.to_owned();
                PackageVersionReq::SemVer(VersionReq {
                    comparators: vec![Comparator {
                        op: Op::Exact,
                        major: version.major,
                        minor: Some(version.minor),
                        patch: Some(version.patch),
                        pre: version.pre,
                    }],
                })
            }
        }
    }
}

#[derive(Error, Debug)]
pub enum PackageVersionParseError {
    #[error(transparent)]
    Specrev(#[from] SpecrevParseError),
    #[error("failed to parse version: {0}")]
    Version(#[from] Error),
}

impl Serialize for PackageVersion {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            PackageVersion::SemVer(version) => version.serialize(serializer),
            PackageVersion::DevVer(version) => version.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for PackageVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::from_str(&s).map_err(de::Error::custom)
    }
}

impl FromLua for PackageVersion {
    fn from_lua(
        value: mlua::prelude::LuaValue,
        lua: &mlua::prelude::Lua,
    ) -> mlua::prelude::LuaResult<Self> {
        let s = String::from_lua(value, lua)?;
        Self::from_str(&s).map_err(|err| mlua::Error::DeserializeError(err.to_string()))
    }
}

impl Display for PackageVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PackageVersion::SemVer(version) => version.fmt(f),
            PackageVersion::DevVer(version) => version.fmt(f),
        }
    }
}

impl PartialOrd for PackageVersion {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PackageVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (PackageVersion::SemVer(a), PackageVersion::SemVer(b)) => a.cmp(b),
            (PackageVersion::SemVer(..), PackageVersion::DevVer(..)) => Ordering::Less,
            (PackageVersion::DevVer(..), PackageVersion::SemVer(..)) => Ordering::Greater,
            (PackageVersion::DevVer(a), PackageVersion::DevVer(b)) => a.cmp(b),
        }
    }
}

impl FromStr for PackageVersion {
    type Err = PackageVersionParseError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let (modrev, specrev) = split_specrev(text)?;
        if is_dev_version_str(modrev) {
            return Ok(PackageVersion::DevVer(DevVer {
                modrev: modrev.into(),
                specrev,
            }));
        }

        Ok(PackageVersion::SemVer(SemVer {
            component_count: cmp::min(text.chars().filter(|c| *c == '.').count() + 1, 3),
            version: parse_version(modrev)?,
            specrev,
        }))
    }
}

// TODO: Stop deriving Eq here
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct SemVer {
    version: Version,
    component_count: usize,
    specrev: u16,
}

impl Display for SemVer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = format!(
            "{}-{}",
            self.version
                .to_string()
                .split('.')
                .take(self.component_count)
                .join("."),
            self.specrev
        );
        str.fmt(f)
    }
}

impl Serialize for SemVer {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.to_string().serialize(serializer)
    }
}

impl Ord for SemVer {
    fn cmp(&self, other: &Self) -> Ordering {
        let result = self.version.cmp(&other.version);
        if result == Ordering::Equal {
            return self.specrev.cmp(&other.specrev);
        }
        result
    }
}

impl PartialOrd for SemVer {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct DevVer {
    modrev: String,
    specrev: u16,
}

impl Display for DevVer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = format!("{}-{}", self.modrev, self.specrev);
        str.fmt(f)
    }
}

impl Serialize for DevVer {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.to_string().serialize(serializer)
    }
}

impl Ord for DevVer {
    fn cmp(&self, other: &Self) -> Ordering {
        // NOTE: We compare specrevs first for dev versions
        let result = self.specrev.cmp(&other.specrev);
        if result == Ordering::Equal {
            return self.modrev.cmp(&other.modrev);
        }
        result
    }
}

impl PartialOrd for DevVer {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Error, Debug)]
#[error(transparent)]
pub struct PackageVersionReqError(#[from] Error);

/// **SemVer version** requirement as defined by <https://semver.org>.
/// or a **Dev** version requirement, which can be one of "dev", "scm", or "git"
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub enum PackageVersionReq {
    /// A PackageVersionReq that matches a SemVer version.
    SemVer(VersionReq),
    /// A PackageVersionReq that matches only dev versions.
    Dev(String),
    /// A PackageVersionReq that has no version constraint.
    Any,
}

impl FromLua for PackageVersionReq {
    fn from_lua(value: mlua::Value, lua: &mlua::Lua) -> mlua::Result<Self> {
        PackageVersionReq::parse(&String::from_lua(value, lua)?).into_lua_err()
    }
}

impl IntoLua for PackageVersionReq {
    fn into_lua(self, lua: &mlua::Lua) -> mlua::Result<mlua::Value> {
        let table = lua.create_table()?;

        match self {
            PackageVersionReq::SemVer(version_req) => {
                table.set("semver", version_req.to_string())?
            }
            PackageVersionReq::Dev(dev) => table.set("dev", dev)?,
            PackageVersionReq::Any => table.set("any", true)?,
        }

        Ok(mlua::Value::Table(table))
    }
}

impl PackageVersionReq {
    /// Returns a `PackageVersionReq` that matches any version.
    pub fn any() -> Self {
        PackageVersionReq::Any
    }

    pub fn parse(text: &str) -> Result<Self, PackageVersionReqError> {
        PackageVersionReq::from_str(text)
    }

    pub fn matches(&self, version: &PackageVersion) -> bool {
        match (self, version) {
            (PackageVersionReq::SemVer(version_req), PackageVersion::SemVer(semver)) => {
                version_req.matches(&semver.version)
            }
            (PackageVersionReq::SemVer(..), PackageVersion::DevVer(..)) => false,
            (PackageVersionReq::Dev(..), PackageVersion::SemVer(..)) => false,
            (PackageVersionReq::Dev(name_req), PackageVersion::DevVer(devver)) => {
                name_req.ends_with(&devver.modrev)
            }
            (PackageVersionReq::Any, _) => true,
        }
    }

    pub fn is_any(&self) -> bool {
        matches!(self, PackageVersionReq::Any)
    }
}

impl Display for PackageVersionReq {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PackageVersionReq::SemVer(version_req) => version_req.fmt(f),
            PackageVersionReq::Dev(name_req) => f.write_str(name_req.as_str()),
            PackageVersionReq::Any => f.write_str("any"),
        }
    }
}

impl<'de> Deserialize<'de> for PackageVersionReq {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

impl FromStr for PackageVersionReq {
    type Err = PackageVersionReqError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let text = text
            .split('-')
            .map(str::to_string)
            .coalesce(|version, rest| {
                Ok(format!(
                    "{version}{}",
                    rest.trim_start_matches(['0', '1', '2', '3', '4', '5', '6', '7', '8', '9'])
                ))
            })
            .collect::<String>();

        if is_dev_version_str(text.trim_start_matches("==").trim()) {
            return Ok(PackageVersionReq::Dev(text));
        }

        Ok(PackageVersionReq::SemVer(parse_version_req(&text)?))
    }
}

#[derive(Error, Debug)]
pub enum SpecrevParseError {
    #[error("specrev {specrev} in version {full_version} contains non-numeric characters")]
    InvalidSpecrev {
        specrev: String,
        full_version: String,
    },
    #[error("could not parse specrev in version {0}")]
    InvalidVersion(String),
}

fn split_specrev(version_str: &str) -> Result<(&str, u16), SpecrevParseError> {
    if let Some(pos) = version_str.rfind('-') {
        if let Some(specrev_str) = version_str.get(pos + 1..) {
            if specrev_str.chars().all(|c| c.is_ascii_digit()) {
                let specrev =
                    specrev_str
                        .parse::<u16>()
                        .map_err(|_| SpecrevParseError::InvalidSpecrev {
                            specrev: specrev_str.into(),
                            full_version: version_str.into(),
                        })?;
                Ok((&version_str[..pos], specrev))
            } else {
                Err(SpecrevParseError::InvalidSpecrev {
                    specrev: specrev_str.into(),
                    full_version: version_str.into(),
                })
            }
        } else {
            Err(SpecrevParseError::InvalidVersion(version_str.into()))
        }
    } else {
        // We assume a specrev of 1 if none can be found.
        Ok((version_str, 1))
    }
}

fn is_dev_version_str(text: &str) -> bool {
    matches!(text, "dev" | "scm" | "git")
}

/// Parses a Version from a string, automatically supplying any missing details (i.e. missing
/// minor/patch sections).
fn parse_version(s: &str) -> Result<Version, Error> {
    let version_str = correct_version_string(s);
    Version::parse(&version_str)
}

/// Transform LuaRocks constraints into constraints that can be parsed by the semver crate.
fn parse_version_req(version_constraints: &str) -> Result<VersionReq, Error> {
    let unescaped = decode_html_entities(version_constraints)
        .to_string()
        .as_str()
        .to_owned();
    let transformed = match unescaped {
        s if s.starts_with("~>") => parse_pessimistic_version_constraint(s)?,
        s if s.starts_with("@") => format!("={}", &s[1..]),
        // The semver crate only understands "= version", unlike luarocks which understands "== version".
        s if s.starts_with("==") => s[1..].to_string(),
        s => s,
    };

    let version_req = VersionReq::parse(&transformed)?;
    Ok(version_req)
}

fn parse_pessimistic_version_constraint(version_constraint: String) -> Result<String, Error> {
    // pessimistic operator
    let min_version_str = &version_constraint[2..].trim();
    let min_version = Version::parse(&correct_version_string(min_version_str))?;

    let max_version = match min_version_str.matches('.').count() {
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

    Ok(format!(">= {min_version}, < {max_version}"))
}

/// ┻━┻ ︵╰(°□°╰) Luarocks allows for an arbitrary number of version digits
/// This function attempts to correct a non-semver compliant version string,
/// by swapping the third '.' out with a '-', converting the non-semver
/// compliant digits to a pre-release identifier.
fn correct_version_string(version: &str) -> String {
    let version = append_minor_patch_if_missing(version);
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() > 3 {
        let corrected_version = format!(
            "{}.{}.{}-{}",
            parts[0],
            parts[1],
            parts[2],
            parts[3..].join(".")
        );
        corrected_version
    } else {
        version.to_string()
    }
}

/// Recursively append .0 until the version string has a minor or patch version
fn append_minor_patch_if_missing(version: &str) -> String {
    if version.matches('.').count() < 2 {
        append_minor_patch_if_missing(&format!("{}.0", version))
    } else {
        version.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn parse_semver_version() {
        assert_eq!(
            PackageVersion::parse("1-1").unwrap(),
            PackageVersion::SemVer(SemVer {
                version: "1.0.0".parse().unwrap(),
                component_count: 1,
                specrev: 1,
            })
        );
        assert_eq!(
            PackageVersion::parse("1.0-1").unwrap(),
            PackageVersion::SemVer(SemVer {
                version: "1.0.0".parse().unwrap(),
                component_count: 2,
                specrev: 1,
            })
        );
        assert_eq!(
            PackageVersion::parse("1.0.0-1").unwrap(),
            PackageVersion::SemVer(SemVer {
                version: "1.0.0".parse().unwrap(),
                component_count: 3,
                specrev: 1
            })
        );
        assert_eq!(
            PackageVersion::parse("1.0.0-1").unwrap(),
            PackageVersion::SemVer(SemVer {
                version: "1.0.0".parse().unwrap(),
                component_count: 3,
                specrev: 1
            })
        );
        assert_eq!(
            PackageVersion::parse("1.0.0-10-1").unwrap(),
            PackageVersion::SemVer(SemVer {
                version: "1.0.0-10".parse().unwrap(),
                component_count: 3,
                specrev: 1
            })
        );
        assert_eq!(
            PackageVersion::parse("1.0.0.10-1").unwrap(),
            PackageVersion::SemVer(SemVer {
                version: "1.0.0-10".parse().unwrap(),
                component_count: 3,
                specrev: 1
            })
        );
        assert_eq!(
            PackageVersion::parse("1.0.0.10.0-1").unwrap(),
            PackageVersion::SemVer(SemVer {
                version: "1.0.0-10.0".parse().unwrap(),
                component_count: 3,
                specrev: 1
            })
        );
    }

    #[tokio::test]
    async fn parse_dev_version() {
        assert_eq!(
            PackageVersion::parse("dev-1").unwrap(),
            PackageVersion::DevVer(DevVer {
                modrev: "dev".into(),
                specrev: 1
            })
        );
        assert_eq!(
            PackageVersion::parse("scm-1").unwrap(),
            PackageVersion::DevVer(DevVer {
                modrev: "scm".into(),
                specrev: 1
            })
        );
        assert_eq!(
            PackageVersion::parse("git-1").unwrap(),
            PackageVersion::DevVer(DevVer {
                modrev: "git".into(),
                specrev: 1
            })
        );
        assert_eq!(
            PackageVersion::parse("scm-1").unwrap(),
            PackageVersion::DevVer(DevVer {
                modrev: "scm".into(),
                specrev: 1
            })
        );
    }

    #[tokio::test]
    async fn parse_dev_version_req() {
        assert_eq!(
            PackageVersionReq::parse("dev").unwrap(),
            PackageVersionReq::Dev("dev".into())
        );
        assert_eq!(
            PackageVersionReq::parse("scm").unwrap(),
            PackageVersionReq::Dev("scm".into())
        );
        assert_eq!(
            PackageVersionReq::parse("git").unwrap(),
            PackageVersionReq::Dev("git".into())
        );
        assert_eq!(
            PackageVersionReq::parse("==dev").unwrap(),
            PackageVersionReq::Dev("==dev".into())
        );
        assert_eq!(
            PackageVersionReq::parse("==git").unwrap(),
            PackageVersionReq::Dev("==git".into())
        );
        assert_eq!(
            PackageVersionReq::parse("== dev").unwrap(),
            PackageVersionReq::Dev("== dev".into())
        );
        assert_eq!(
            PackageVersionReq::parse("== scm").unwrap(),
            PackageVersionReq::Dev("== scm".into())
        );
        assert_eq!(
            PackageVersionReq::parse(">1-1,<1.2-2").unwrap(),
            PackageVersionReq::SemVer(">1,<1.2".parse().unwrap())
        );
        assert_eq!(
            PackageVersionReq::parse("> 1-1, < 1.2-2").unwrap(),
            PackageVersionReq::SemVer("> 1, < 1.2".parse().unwrap())
        );
    }
}
