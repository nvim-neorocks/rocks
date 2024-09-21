use std::{cmp::Ordering, fmt::Display, str::FromStr};

use eyre::Result;
use html_escape::decode_html_entities;
use mlua::FromLua;
use semver::{Comparator, Error, Op, Version, VersionReq};
use serde::{de, Deserialize, Deserializer, Serialize};

/// **SemVer version** as defined by <https://semver.org>.
/// or a **Dev** version, which can be one of "dev", "scm", or "git"
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub enum PackageVersion {
    SemVer { version: Version },
    Dev { name: String },
}

impl PackageVersion {
    pub fn parse(text: &str) -> Result<Self> {
        PackageVersion::from_str(text)
    }
}

impl Serialize for PackageVersion {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            PackageVersion::SemVer { version } => version.to_string().serialize(serializer),
            PackageVersion::Dev { name } => name.serialize(serializer),
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

impl<'lua> FromLua<'lua> for PackageVersion {
    fn from_lua(
        value: mlua::prelude::LuaValue<'lua>,
        lua: &'lua mlua::prelude::Lua,
    ) -> mlua::prelude::LuaResult<Self> {
        let s = String::from_lua(value, lua)?;
        Self::from_str(&s).map_err(|err| mlua::Error::DeserializeError(err.to_string()))
    }
}

impl Display for PackageVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PackageVersion::SemVer { version } => version.fmt(f),
            PackageVersion::Dev { name } => f.write_str(name.as_str()),
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
            (PackageVersion::SemVer { version: a }, PackageVersion::SemVer { version: b }) => {
                a.cmp(b)
            }
            (PackageVersion::SemVer { .. }, PackageVersion::Dev { .. }) => Ordering::Less,
            (PackageVersion::Dev { .. }, PackageVersion::SemVer { .. }) => Ordering::Greater,
            (PackageVersion::Dev { name: a }, PackageVersion::Dev { name: b }) => a.cmp(b),
        }
    }
}

impl FromStr for PackageVersion {
    type Err = eyre::Error;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        if is_dev_version_str(text) {
            return Ok(PackageVersion::Dev { name: text.into() });
        }
        Ok(PackageVersion::SemVer {
            version: parse_version(text)?,
        })
    }
}

/// **SemVer version** requirement as defined by <https://semver.org>.
/// or a **Dev** version requirement, which can be one of "dev", "scm", or "git"
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub enum PackageVersionReq {
    SemVer { version_req: VersionReq },
    Dev { name_req: String },
}

impl PackageVersionReq {
    pub fn parse(text: &str) -> Result<Self> {
        PackageVersionReq::from_str(text)
    }
    pub fn matches(&self, version: &PackageVersion) -> bool {
        match (self, version) {
            (PackageVersionReq::SemVer { version_req }, PackageVersion::SemVer { version }) => {
                version_req.matches(version)
            }
            (PackageVersionReq::SemVer { .. }, PackageVersion::Dev { .. }) => true,
            (PackageVersionReq::Dev { .. }, PackageVersion::SemVer { .. }) => false,
            (PackageVersionReq::Dev { name_req }, PackageVersion::Dev { name }) => {
                name_req.ends_with(name)
            }
        }
    }
}

impl Display for PackageVersionReq {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PackageVersionReq::SemVer { version_req } => version_req.fmt(f),
            PackageVersionReq::Dev { name_req } => f.write_str(name_req.as_str()),
        }
    }
}

impl Default for PackageVersionReq {
    fn default() -> Self {
        PackageVersionReq::SemVer {
            version_req: VersionReq::default(),
        }
    }
}

impl From<PackageVersion> for PackageVersionReq {
    fn from(version: PackageVersion) -> Self {
        match version {
            PackageVersion::Dev { name } => PackageVersionReq::Dev { name_req: name },
            PackageVersion::SemVer { version } => PackageVersionReq::SemVer {
                version_req: VersionReq {
                    comparators: vec![Comparator {
                        op: Op::Exact,
                        major: version.major,
                        minor: Some(version.minor),
                        patch: Some(version.patch),
                        pre: version.pre,
                    }],
                },
            },
        }
    }
}

impl FromStr for PackageVersionReq {
    type Err = eyre::Error;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        if is_dev_version_str(text.trim_start_matches("==").trim()) {
            return Ok(PackageVersionReq::Dev {
                name_req: text.into(),
            });
        }
        Ok(PackageVersionReq::SemVer {
            version_req: parse_version_req(text)?,
        })
    }
}

fn is_dev_version_str(text: &str) -> bool {
    matches!(text, "dev" | "scm" | "git")
}

/// Parses a Version from a string, automatically supplying any missing details (i.e. missing
/// minor/patch sections).
fn parse_version(s: &str) -> Result<Version, Error> {
    Version::parse(&append_minor_patch_if_missing(s.to_string()))
}

/// Transform LuaRocks constraints into constraints that can be parsed by the semver crate.
fn parse_version_req(version_constraints: &str) -> Result<VersionReq, Error> {
    let unescaped = decode_html_entities(version_constraints)
        .to_string()
        .as_str()
        .to_owned();
    let transformed = match unescaped {
        s if s.starts_with("~>") => parse_pessimistic_version_constraint(s)?,
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
    let min_version = Version::parse(&append_minor_patch_if_missing(min_version_str.to_string()))?;

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

/// Recursively append .0 until the version string has a minor or patch version
fn append_minor_patch_if_missing(version: String) -> String {
    if version.matches('.').count() < 2 {
        append_minor_patch_if_missing(version + ".0")
    } else {
        version
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn parse_dev_version() {
        assert_eq!(
            PackageVersion::parse("dev").unwrap(),
            PackageVersion::Dev { name: "dev".into() }
        );
        assert_eq!(
            PackageVersion::parse("scm").unwrap(),
            PackageVersion::Dev { name: "scm".into() }
        );
        assert_eq!(
            PackageVersion::parse("git").unwrap(),
            PackageVersion::Dev { name: "git".into() }
        );
    }

    #[tokio::test]
    async fn parse_dev_version_req() {
        assert_eq!(
            PackageVersionReq::parse("dev").unwrap(),
            PackageVersionReq::Dev {
                name_req: "dev".into()
            }
        );
        assert_eq!(
            PackageVersionReq::parse("scm").unwrap(),
            PackageVersionReq::Dev {
                name_req: "scm".into()
            }
        );
        assert_eq!(
            PackageVersionReq::parse("git").unwrap(),
            PackageVersionReq::Dev {
                name_req: "git".into()
            }
        );
        assert_eq!(
            PackageVersionReq::parse("==dev").unwrap(),
            PackageVersionReq::Dev {
                name_req: "==dev".into()
            }
        );
        assert_eq!(
            PackageVersionReq::parse("==git").unwrap(),
            PackageVersionReq::Dev {
                name_req: "==git".into()
            }
        );
        assert_eq!(
            PackageVersionReq::parse("== dev").unwrap(),
            PackageVersionReq::Dev {
                name_req: "== dev".into()
            }
        );
        assert_eq!(
            PackageVersionReq::parse("== scm").unwrap(),
            PackageVersionReq::Dev {
                name_req: "== scm".into()
            }
        );
    }
}
