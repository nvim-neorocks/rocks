use eyre::{eyre, Result};
use mlua::{Lua, LuaSerdeExt, Value};
use serde::Deserialize;

use crate::rocks::PlatformSupport;

use super::{parse_dependencies, LuaDependency};

pub struct Rockspec {
    /// The file format version. Example: "1.0"
    pub rockspec_format: Option<String>,
    /// The name of the package. Example: "LuaSocket"
    pub package: String,
    /// The version of the package, plus a suffix indicating the revision of the rockspec. Example: "2.0.1-1"
    pub version: String,
    pub description: RockDescription,
    pub supported_platforms: PlatformSupport,
    pub dependencies: Vec<LuaDependency>,
    pub build_dependencies: Vec<LuaDependency>,
}

impl Rockspec {
    pub fn new(rockspec_content: &String) -> Result<Self> {
        let lua = Lua::new();
        lua.load(rockspec_content).exec()?;

        let rockspec = Rockspec {
            rockspec_format: lua.from_value(lua.globals().get("rockspec_format")?)?,
            package: lua.from_value(lua.globals().get("package")?)?,
            version: lua.from_value(lua.globals().get("version")?)?,
            description: RockDescription::from_lua(&lua)?,
            supported_platforms: match lua.globals().get("supported_platforms")? {
                Value::Nil => PlatformSupport::default(),
                value @ Value::Table(_) => PlatformSupport::new(&lua.from_value(value)?)?,
                value => Err(eyre!(format!(
                    "Could not parse supported_platforms. Expected list, but got {}",
                    value.type_name()
                )))?,
            },
            // TODO(mrcjkb): support per-platform overrides: https://github.com/luarocks/luarocks/wiki/platform-overrides
            dependencies: parse_lua_dependencies(&lua, "dependencies")?,
            build_dependencies: parse_lua_dependencies(&lua, "build_dependencies")?,
        };

        Ok(rockspec)
    }
}

#[derive(Deserialize, Debug, PartialEq, Default)]
pub struct RockDescription {
    /// A one-line description of the package.
    pub summary: Option<String>,
    /// A longer description of the package.
    pub detailed: Option<String>,
    /// The license used by the package.
    pub license: Option<String>,
    /// An URL for the project. This is not the URL for the tarball, but the address of a website.
    pub homepage: Option<String>,
    /// An URL for the project's issue tracker.
    pub issues_url: Option<String>,
    /// Contact information for the rockspec maintainer.
    pub maintainer: Option<String>,
    /// A list of short strings that specify labels for categorization of this rock.
    pub labels: Vec<String>,
}

impl RockDescription {
    fn from_lua(lua: &Lua) -> Result<RockDescription> {
        match lua.globals().get("description")? {
            Value::Nil => Ok(RockDescription::default()),
            Value::Table(tbl) => {
                let labels = if tbl.contains_key("labels")? {
                    lua.from_value(tbl.get("labels")?)?
                } else {
                    Vec::new()
                };
                Ok(RockDescription {
                    summary: lua.from_value(tbl.get("summary")?)?,
                    detailed: lua.from_value(tbl.get("detailed")?)?,
                    license: lua.from_value(tbl.get("license")?)?,
                    homepage: lua.from_value(tbl.get("homepage")?)?,
                    issues_url: lua.from_value(tbl.get("issues_url")?)?,
                    maintainer: lua.from_value(tbl.get("maintainer")?)?,
                    labels,
                })
            }
            value => Err(eyre!(format!(
                "Could not parse rockspec description. Expected table, but got {}",
                value.type_name()
            ))),
        }
    }
}

fn parse_lua_dependencies(lua: &Lua, lua_var_name: &str) -> Result<Vec<LuaDependency>> {
    let dependencies = match lua.globals().get(lua_var_name)? {
        Value::Nil => Vec::default(),
        value @ Value::Table(_) => parse_dependencies(&lua.from_value(value)?)?,
        value => Err(eyre!(format!(
            "Could not parse {}. Expected list, but got {}",
            lua_var_name,
            value.type_name(),
        )))?,
    };
    Ok(dependencies)
}

#[cfg(test)]
mod tests {

    use crate::rocks::{LuaRock, PlatformIdentifier};

    use super::*;

    #[tokio::test]
    pub async fn parse_rockspec() {
        let rockspec_content = "
        rockspec_format = '1.0'\n
        package = 'foo'\n
        version = '1.0.0-1'\n
        "
        .to_string();
        let rockspec = Rockspec::new(&rockspec_content).unwrap();
        assert_eq!(rockspec.rockspec_format, Some("1.0".into()));
        assert_eq!(rockspec.package, "foo");
        assert_eq!(rockspec.version, "1.0.0-1");
        assert_eq!(rockspec.description, RockDescription::default());

        let rockspec_content = "
        package = 'bar'\n
        version = '2.0.0-1'\n
        description = {}\n
        "
        .to_string();
        let rockspec = Rockspec::new(&rockspec_content).unwrap();
        assert_eq!(rockspec.rockspec_format, None);
        assert_eq!(rockspec.package, "bar");
        assert_eq!(rockspec.version, "2.0.0-1");
        assert_eq!(rockspec.description, RockDescription::default());

        let rockspec_content = "
        package = 'rocks'\n
        version = '3.0.0-1'\n
        description = {\n
            summary = 'some summary',
            detailed = 'some detailed description',
            license = 'MIT',
            homepage = 'https://github.com/nvim-neorocks/rocks',
            issues_url = 'https://github.com/nvim-neorocks/rocks/issues',
            maintainer = 'neorocks',
        }\n
        "
        .to_string();
        let rockspec = Rockspec::new(&rockspec_content).unwrap();
        assert_eq!(rockspec.rockspec_format, None);
        assert_eq!(rockspec.package, "rocks");
        assert_eq!(rockspec.version, "3.0.0-1");
        let expected_description = RockDescription {
            summary: Some("some summary".into()),
            detailed: Some("some detailed description".into()),
            license: Some("MIT".into()),
            homepage: Some("https://github.com/nvim-neorocks/rocks".into()),
            issues_url: Some("https://github.com/nvim-neorocks/rocks/issues".into()),
            maintainer: Some("neorocks".into()),
            labels: Vec::new(),
        };
        assert_eq!(rockspec.description, expected_description);

        let rockspec_content = "
        package = 'rocks'\n
        version = '3.0.0-1'\n
        description = {\n
            summary = 'some summary',
            detailed = 'some detailed description',
            license = 'MIT',
            homepage = 'https://github.com/nvim-neorocks/rocks',
            issues_url = 'https://github.com/nvim-neorocks/rocks/issues',
            maintainer = 'neorocks',
            labels = {},
        }\n
        "
        .to_string();
        let rockspec = Rockspec::new(&rockspec_content).unwrap();
        assert_eq!(rockspec.rockspec_format, None);
        assert_eq!(rockspec.package, "rocks");
        assert_eq!(rockspec.version, "3.0.0-1");
        let expected_description = RockDescription {
            summary: Some("some summary".into()),
            detailed: Some("some detailed description".into()),
            license: Some("MIT".into()),
            homepage: Some("https://github.com/nvim-neorocks/rocks".into()),
            issues_url: Some("https://github.com/nvim-neorocks/rocks/issues".into()),
            maintainer: Some("neorocks".into()),
            labels: Vec::new(),
        };
        assert_eq!(rockspec.description, expected_description);

        let rockspec_content = "
        package = 'rocks'\n
        version = '3.0.0-1'\n
        description = {\n
            summary = 'some summary',
            detailed = 'some detailed description',
            license = 'MIT',
            homepage = 'https://github.com/nvim-neorocks/rocks',
            issues_url = 'https://github.com/nvim-neorocks/rocks/issues',
            maintainer = 'neorocks',
            labels = { 'package management', },
        }\n
        supported_platforms = { 'unix', '!windows' }\n
        dependencies = { 'neorg ~> 6' }\n
        build_dependencies = { 'foo' }\n
        "
        .to_string();
        let rockspec = Rockspec::new(&rockspec_content).unwrap();
        assert_eq!(rockspec.rockspec_format, None);
        assert_eq!(rockspec.package, "rocks");
        assert_eq!(rockspec.version, "3.0.0-1");
        let expected_description = RockDescription {
            summary: Some("some summary".into()),
            detailed: Some("some detailed description".into()),
            license: Some("MIT".into()),
            homepage: Some("https://github.com/nvim-neorocks/rocks".into()),
            issues_url: Some("https://github.com/nvim-neorocks/rocks/issues".into()),
            maintainer: Some("neorocks".into()),
            labels: vec!["package management".into()],
        };
        assert_eq!(rockspec.description, expected_description);
        assert!(rockspec
            .supported_platforms
            .is_supported(&PlatformIdentifier::Unix));
        assert!(!rockspec
            .supported_platforms
            .is_supported(&PlatformIdentifier::Windows));
        let neorg = LuaRock::new("neorg".into(), "6.0.0".into()).unwrap();
        assert!(rockspec
            .dependencies
            .into_iter()
            .any(|dep| dep.matches(&neorg)));
        let foo = LuaRock::new("foo".into(), "1.0.0".into()).unwrap();
        assert!(rockspec
            .build_dependencies
            .into_iter()
            .any(|dep| dep.matches(&foo)));
    }
}
