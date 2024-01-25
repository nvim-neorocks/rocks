use std::str::FromStr;

use eyre::{eyre, Result};
use html_escape::decode_html_entities;
use semver::{Version, VersionReq};

pub struct LuaDependency {
    pub rock_name: String,
    rock_version_req: Option<VersionReq>,
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
        let rock_version_req = match str.trim_start_matches(&rock_name) {
            "" => None,
            version_constraints => Some(parse_version_req(version_constraints.trim_start())?),
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
        self.rock_version_req
            .as_ref()
            .map(|ver| ver.matches(&rock.version))
            .unwrap_or(true)
    }
}

/// Transform LuaRocks constraints into constraints that can be parsed by the semver crate.
fn parse_version_req(version_constraints: &str) -> Result<VersionReq> {
    // TODO: Handle special Rockspec cases: ~>
    let unescaped = decode_html_entities(version_constraints)
        .to_string()
        .as_str()
        .to_owned();
    let version_req = VersionReq::parse(&unescaped)?;
    Ok(version_req)
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
            version: Version::parse(append_zeros(version).as_str())?,
        })
    }
}

/// Recursively append .0 until the version string has a minor or patch version
fn append_zeros(version: String) -> String {
    if version.matches(".").count() < 2 {
        return append_zeros(version + ".0");
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
    }
}
