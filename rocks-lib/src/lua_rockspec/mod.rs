mod build;
mod dependency;
mod platform;
mod rock_source;
mod serde_util;
mod test_spec;

use std::{collections::HashMap, io, path::PathBuf, str::FromStr};

use mlua::{FromLua, Lua, LuaSerdeExt, Value};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

pub use build::*;
pub use dependency::*;
pub use platform::*;
pub use rock_source::*;
pub use serde_util::*;
use ssri::Integrity;
pub use test_spec::*;
use thiserror::Error;
use url::Url;

use crate::{
    config::{LuaVersion, LuaVersionUnset},
    hash::HasIntegrity,
    package::{PackageName, PackageReq, PackageVersion},
    rockspec::Rockspec,
};

#[derive(Error, Debug)]
pub enum RockspecError {
    #[error("could not parse rockspec: {0}")]
    MLua(#[from] mlua::Error),
    #[error("{}copy_directories cannot contain the rockspec name", ._0.as_ref().map(|p| format!("{p}: ")).unwrap_or_default())]
    CopyDirectoriesContainRockspecName(Option<String>),
    #[error("could not parse rockspec: {0}")]
    LuaTable(#[from] LuaTableError),
}

#[derive(Clone, Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub struct LuaRockspec {
    /// The file format version. Example: "1.0"
    rockspec_format: Option<RockspecFormat>,
    /// The name of the package. Example: "luasocket"
    package: PackageName,
    /// The version of the package, plus a suffix indicating the revision of the rockspec. Example: "2.0.1-1"
    version: PackageVersion,
    description: RockDescription,
    supported_platforms: PlatformSupport,
    dependencies: PerPlatform<Vec<PackageReq>>,
    build_dependencies: PerPlatform<Vec<PackageReq>>,
    external_dependencies: PerPlatform<HashMap<String, ExternalDependencySpec>>,
    test_dependencies: PerPlatform<Vec<PackageReq>>,
    source: PerPlatform<RockSource>,
    build: PerPlatform<BuildSpec>,
    test: PerPlatform<TestSpec>,
    /// The original content of this rockspec, needed by luarocks
    raw_content: String,
    /// The sha256 of this rockspec
    hash: Integrity,
}

impl LuaRockspec {
    pub fn new(rockspec_content: &str) -> Result<Self, RockspecError> {
        let lua = Lua::new();
        lua.load(rockspec_content).exec()?;

        let globals = lua.globals();
        let rockspec = LuaRockspec {
            rockspec_format: globals.get("rockspec_format")?,
            package: globals.get("package")?,
            version: globals.get("version")?,
            description: parse_lua_tbl_or_default(&lua, "description")?,
            supported_platforms: parse_lua_tbl_or_default(&lua, "supported_platforms")?,
            dependencies: globals.get("dependencies")?,
            build_dependencies: globals.get("build_dependencies")?,
            test_dependencies: globals.get("test_dependencies")?,
            external_dependencies: globals.get("external_dependencies")?,
            source: globals.get("source")?,
            build: globals.get("build")?,
            test: globals.get("test")?,
            hash: Integrity::from(rockspec_content),
            raw_content: rockspec_content.into(),
        };

        let rockspec_file_name = format!("{}-{}.rockspec", rockspec.package, rockspec.version);
        if rockspec
            .build
            .default
            .copy_directories
            .contains(&PathBuf::from(&rockspec_file_name))
        {
            return Err(RockspecError::CopyDirectoriesContainRockspecName(None));
        }

        for (platform, build_override) in &rockspec.build.per_platform {
            if build_override
                .copy_directories
                .contains(&PathBuf::from(&rockspec_file_name))
            {
                return Err(RockspecError::CopyDirectoriesContainRockspecName(Some(
                    platform.to_string(),
                )));
            }
        }
        Ok(rockspec)
    }
}

impl Rockspec for LuaRockspec {
    fn package(&self) -> &PackageName {
        &self.package
    }

    fn version(&self) -> &PackageVersion {
        &self.version
    }

    fn description(&self) -> &RockDescription {
        &self.description
    }

    fn dependencies(&self) -> &PerPlatform<Vec<PackageReq>> {
        &self.dependencies
    }

    fn build_dependencies(&self) -> &PerPlatform<Vec<PackageReq>> {
        &self.build_dependencies
    }

    fn external_dependencies(&self) -> &PerPlatform<HashMap<String, ExternalDependencySpec>> {
        &self.external_dependencies
    }

    fn test_dependencies(&self) -> &PerPlatform<Vec<PackageReq>> {
        &self.test_dependencies
    }

    fn source(&self) -> &PerPlatform<RockSource> {
        &self.source
    }

    fn build(&self) -> &PerPlatform<BuildSpec> {
        &self.build
    }

    fn test(&self) -> &PerPlatform<TestSpec> {
        &self.test
    }

    fn supported_platforms(&self) -> &PlatformSupport {
        &self.supported_platforms
    }

    fn source_mut(&mut self) -> &mut PerPlatform<RockSource> {
        &mut self.source
    }

    fn build_mut(&mut self) -> &mut PerPlatform<BuildSpec> {
        &mut self.build
    }

    fn test_mut(&mut self) -> &mut PerPlatform<TestSpec> {
        &mut self.test
    }

    fn to_rockspec_str(&self) -> String {
        self.raw_content.clone()
    }

    fn format(&self) -> &Option<RockspecFormat> {
        &self.rockspec_format
    }
}

#[derive(Error, Debug)]
pub enum LuaVersionError {
    #[error("The lua version {0} is not supported by {1} version {1}!")]
    LuaVersionUnsupported(LuaVersion, PackageName, PackageVersion),
    #[error(transparent)]
    LuaVersionUnset(#[from] LuaVersionUnset),
}

impl HasIntegrity for LuaRockspec {
    fn hash(&self) -> io::Result<Integrity> {
        Ok(self.hash.to_owned())
    }
}

#[derive(Clone, Deserialize, Debug, PartialEq, Default)]
pub struct RockDescription {
    /// A one-line description of the package.
    pub summary: Option<String>,
    /// A longer description of the package.
    pub detailed: Option<String>,
    /// The license used by the package.
    pub license: Option<String>,
    /// An URL for the project. This is not the URL for the tarball, but the address of a website.
    #[serde(default, deserialize_with = "deserialize_url")]
    pub homepage: Option<Url>,
    /// An URL for the project's issue tracker.
    pub issues_url: Option<String>,
    /// Contact information for the rockspec maintainer.
    pub maintainer: Option<String>,
    /// A list of short strings that specify labels for categorization of this rock.
    #[serde(default)]
    pub labels: Vec<String>,
}

fn deserialize_url<'de, D>(deserializer: D) -> Result<Option<Url>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = Option::<String>::deserialize(deserializer)?;
    s.map(|s| Url::parse(&s).map_err(serde::de::Error::custom))
        .transpose()
}

impl DisplayAsLuaKV for RockDescription {
    fn display_lua(&self) -> DisplayLuaKV {
        let mut description = Vec::new();

        if let Some(summary) = &self.summary {
            description.push(DisplayLuaKV {
                key: "summary".to_string(),
                value: DisplayLuaValue::String(summary.clone()),
            })
        }
        if let Some(detailed) = &self.detailed {
            description.push(DisplayLuaKV {
                key: "detailed".to_string(),
                value: DisplayLuaValue::String(detailed.clone()),
            })
        }
        if let Some(license) = &self.license {
            description.push(DisplayLuaKV {
                key: "license".to_string(),
                value: DisplayLuaValue::String(license.clone()),
            })
        }
        if let Some(homepage) = &self.homepage {
            description.push(DisplayLuaKV {
                key: "homepage".to_string(),
                value: DisplayLuaValue::String(homepage.to_string()),
            })
        }
        if let Some(issues_url) = &self.issues_url {
            description.push(DisplayLuaKV {
                key: "issues_url".to_string(),
                value: DisplayLuaValue::String(issues_url.clone()),
            })
        }
        if let Some(maintainer) = &self.maintainer {
            description.push(DisplayLuaKV {
                key: "maintainer".to_string(),
                value: DisplayLuaValue::String(maintainer.clone()),
            })
        }
        if !self.labels.is_empty() {
            description.push(DisplayLuaKV {
                key: "labels".to_string(),
                value: DisplayLuaValue::List(
                    self.labels
                        .iter()
                        .cloned()
                        .map(DisplayLuaValue::String)
                        .collect(),
                ),
            })
        }

        DisplayLuaKV {
            key: "description".to_string(),
            value: DisplayLuaValue::Table(description),
        }
    }
}

#[derive(Error, Debug)]
#[error("invalid rockspec format: {0}")]
pub struct InvalidRockspecFormat(String);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RockspecFormat {
    #[serde(rename = "1.0")]
    _1_0,
    #[serde(rename = "2.0")]
    _2_0,
    #[serde(rename = "3.0")]
    _3_0,
}

impl FromStr for RockspecFormat {
    type Err = InvalidRockspecFormat;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "1.0" => Ok(Self::_1_0),
            "2.0" => Ok(Self::_2_0),
            "3.0" => Ok(Self::_3_0),
            txt => Err(InvalidRockspecFormat(txt.to_string())),
        }
    }
}

impl From<&str> for RockspecFormat {
    fn from(s: &str) -> Self {
        Self::from_str(s).unwrap()
    }
}

impl FromLua for RockspecFormat {
    fn from_lua(
        value: mlua::prelude::LuaValue,
        lua: &mlua::prelude::Lua,
    ) -> mlua::prelude::LuaResult<Self> {
        let s = String::from_lua(value, lua)?;
        Self::from_str(&s).map_err(|err| mlua::Error::DeserializeError(err.to_string()))
    }
}

#[derive(Error, Debug)]
pub enum LuaTableError {
    #[error("could not parse {variable}. Expected list, but got {invalid_type}")]
    ParseError {
        variable: String,
        invalid_type: String,
    },
    #[error(transparent)]
    MLua(#[from] mlua::Error),
}

fn parse_lua_tbl_or_default<T>(lua: &Lua, lua_var_name: &str) -> Result<T, LuaTableError>
where
    T: Default,
    T: DeserializeOwned,
{
    let ret = match lua.globals().get(lua_var_name)? {
        Value::Nil => T::default(),
        value @ Value::Table(_) => lua.from_value(value)?,
        value => Err(LuaTableError::ParseError {
            variable: lua_var_name.to_string(),
            invalid_type: value.type_name().to_string(),
        })?,
    };
    Ok(ret)
}

#[cfg(test)]
mod tests {

    use std::path::PathBuf;

    use crate::lua_rockspec::PlatformIdentifier;
    use crate::package::PackageSpec;

    use super::*;

    #[tokio::test]
    pub async fn parse_rockspec() {
        let rockspec_content = "
        rockspec_format = '1.0'\n
        package = 'foo'\n
        version = '1.0.0-1'\n
        source = {\n
            url = 'https://github.com/nvim-neorocks/rocks.nvim/archive/1.0.0/rocks.nvim.zip',\n
        }\n
        "
        .to_string();
        let rockspec = LuaRockspec::new(&rockspec_content).unwrap();
        assert_eq!(rockspec.rockspec_format, Some("1.0".into()));
        assert_eq!(rockspec.package, "foo".into());
        assert_eq!(rockspec.version, "1.0.0-1".parse().unwrap());
        assert_eq!(rockspec.description, RockDescription::default());

        let rockspec_content = "
        package = 'bar'\n
        version = '2.0.0-1'\n
        description = {}\n
        source = {\n
            url = 'https://github.com/nvim-neorocks/rocks.nvim/archive/1.0.0/rocks.nvim.zip',\n
        }\n
        "
        .to_string();
        let rockspec = LuaRockspec::new(&rockspec_content).unwrap();
        assert_eq!(rockspec.rockspec_format, None);
        assert_eq!(rockspec.package, "bar".into());
        assert_eq!(rockspec.version, "2.0.0-1".parse().unwrap());
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
        source = {\n
            url = 'https://github.com/nvim-neorocks/rocks.nvim/archive/1.0.0/rocks.nvim.zip',\n
        }\n
        "
        .to_string();
        let rockspec = LuaRockspec::new(&rockspec_content).unwrap();
        assert_eq!(rockspec.rockspec_format, None);
        assert_eq!(rockspec.package, "rocks".into());
        assert_eq!(rockspec.version, "3.0.0-1".parse().unwrap());
        let expected_description = RockDescription {
            summary: Some("some summary".into()),
            detailed: Some("some detailed description".into()),
            license: Some("MIT".into()),
            homepage: Some(Url::parse("https://github.com/nvim-neorocks/rocks").unwrap()),
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
        external_dependencies = { FOO = { library = 'foo' } }\n
        source = {\n
            url = 'https://github.com/nvim-neorocks/rocks.nvim/archive/1.0.0/rocks.nvim.zip',\n
        }\n
        "
        .to_string();
        let rockspec = LuaRockspec::new(&rockspec_content).unwrap();
        assert_eq!(rockspec.rockspec_format, None);
        assert_eq!(rockspec.package, "rocks".into());
        assert_eq!(rockspec.version, "3.0.0-1".parse().unwrap());
        let expected_description = RockDescription {
            summary: Some("some summary".into()),
            detailed: Some("some detailed description".into()),
            license: Some("MIT".into()),
            homepage: Some(Url::parse("https://github.com/nvim-neorocks/rocks").unwrap()),
            issues_url: Some("https://github.com/nvim-neorocks/rocks/issues".into()),
            maintainer: Some("neorocks".into()),
            labels: Vec::new(),
        };
        assert_eq!(rockspec.description, expected_description);
        assert_eq!(
            *rockspec.external_dependencies.default.get("FOO").unwrap(),
            ExternalDependencySpec::Library("foo".into())
        );

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
        external_dependencies = { FOO = { header = 'foo.h' } }\n
        test_dependencies = { 'busted >= 2.0.0' }\n
        source = {\n
            url = 'git+https://github.com/nvim-neorocks/rocks.nvim',\n
            hash = 'sha256-uU0nuZNNPgilLlLX2n2r+sSE7+N6U4DukIj3rOLvzek=',\n
        }\n
        "
        .to_string();
        let rockspec = LuaRockspec::new(&rockspec_content).unwrap();
        assert_eq!(rockspec.rockspec_format, None);
        assert_eq!(rockspec.package, "rocks".into());
        assert_eq!(rockspec.version, "3.0.0-1".parse().unwrap());
        let expected_description = RockDescription {
            summary: Some("some summary".into()),
            detailed: Some("some detailed description".into()),
            license: Some("MIT".into()),
            homepage: Some(Url::parse("https://github.com/nvim-neorocks/rocks").unwrap()),
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
        let neorg = PackageSpec::parse("neorg".into(), "6.0.0".into()).unwrap();
        assert!(rockspec
            .dependencies
            .default
            .into_iter()
            .any(|dep| dep.matches(&neorg)));
        let foo = PackageSpec::parse("foo".into(), "1.0.0".into()).unwrap();
        assert!(rockspec
            .build_dependencies
            .default
            .into_iter()
            .any(|dep| dep.matches(&foo)));
        let busted = PackageSpec::parse("busted".into(), "2.2.0".into()).unwrap();
        assert_eq!(
            *rockspec.external_dependencies.default.get("FOO").unwrap(),
            ExternalDependencySpec::Header("foo.h".into())
        );
        assert!(rockspec
            .test_dependencies
            .default
            .into_iter()
            .any(|dep| dep.matches(&busted)));

        let rockspec_content = "
        rockspec_format = '1.0'\n
        package = 'foo'\n
        version = '1.0.0-1'\n
        source = {\n
            url = 'git+https://hub.com/example-project/',\n
            branch = 'bar',\n
        }\n
        "
        .to_string();
        let rockspec = LuaRockspec::new(&rockspec_content).unwrap();
        assert_eq!(
            rockspec.source.default.source_spec,
            RockSourceSpec::Git(GitSource {
                url: "https://hub.com/example-project/".parse().unwrap(),
                checkout_ref: Some("bar".into())
            })
        );
        assert_eq!(rockspec.test, PerPlatform::default());
        let rockspec_content = "
        rockspec_format = '1.0'\n
        package = 'foo'\n
        version = '1.0.0-1'\n
        source = {\n
            url = 'git+https://hub.com/example-project/',\n
            tag = 'bar',\n
        }\n
        "
        .to_string();
        let rockspec = LuaRockspec::new(&rockspec_content).unwrap();
        assert_eq!(
            rockspec.source.default.source_spec,
            RockSourceSpec::Git(GitSource {
                url: "https://hub.com/example-project/".parse().unwrap(),
                checkout_ref: Some("bar".into())
            })
        );
        let rockspec_content = "
        rockspec_format = '1.0'\n
        package = 'foo'\n
        version = '1.0.0-1'\n
        source = {\n
            url = 'git+https://hub.com/example-project/',\n
            branch = 'bar',\n
            tag = 'baz',\n
        }\n
        "
        .to_string();
        let _rockspec = LuaRockspec::new(&rockspec_content).unwrap_err();
        let rockspec_content = "
        rockspec_format = '1.0'\n
        package = 'foo'\n
        version = '1.0.0-1'\n
        source = {\n
            url = 'git+https://hub.com/example-project/',\n
            module = 'bar',\n
        }\n
        "
        .to_string();
        let _rockspec = LuaRockspec::new(&rockspec_content).unwrap_err();
        let rockspec_content = "
        rockspec_format = '1.0'\n
        package = 'foo'\n
        version = '1.0.0-1'\n
        source = {\n
            url = 'git+https://hub.com/example-project/',\n
            tag = 'bar',\n
            file = 'foo.tar.gz',\n
        }\n
        build = {\n
            install = {\n
                conf = {['foo.bar'] = 'config/bar.toml'},\n
            },\n
        }\n
        "
        .to_string();
        let rockspec = LuaRockspec::new(&rockspec_content).unwrap();
        assert_eq!(
            rockspec.source.default.archive_name,
            Some("foo.tar.gz".into())
        );
        let foo_bar_path = rockspec.build.default.install.conf.get("foo.bar").unwrap();
        assert_eq!(*foo_bar_path, PathBuf::from("config/bar.toml"));
        let rockspec_content = "
        rockspec_format = '1.0'\n
        package = 'foo'\n
        version = '1.0.0-1'\n
        source = {\n
            url = 'git+https://hub.com/example-project/foo.zip',\n
        }\n
        build = {\n
            install = {\n
                lua = {['foo.bar'] = 'src/bar.lua'},\n
                bin = {['foo.bar'] = 'bin/bar'},\n
            },\n
        }\n
        "
        .to_string();
        let rockspec = LuaRockspec::new(&rockspec_content).unwrap();
        assert!(matches!(
            rockspec.build.default.build_backend,
            Some(BuildBackendSpec::Builtin { .. })
        ));
        let foo_bar_path = rockspec
            .build
            .default
            .install
            .lua
            .get(&LuaModule::from_str("foo.bar").unwrap())
            .unwrap();
        assert_eq!(*foo_bar_path, PathBuf::from("src/bar.lua"));
        let foo_bar_path = rockspec.build.default.install.bin.get("foo.bar").unwrap();
        assert_eq!(*foo_bar_path, PathBuf::from("bin/bar"));
        let rockspec_content = "
        rockspec_format = '1.0'\n
        package = 'foo'\n
        version = '1.0.0-1'\n
        source = {\n
            url = 'git+https://hub.com/example-project/',\n
        }\n
        build = {\n
            copy_directories = { 'lua' },\n
        }\n
        "
        .to_string();
        let _rockspec = LuaRockspec::new(&rockspec_content).unwrap_err();
        let rockspec_content = "
        rockspec_format = '1.0'\n
        package = 'foo'\n
        version = '1.0.0-1'\n
        source = {\n
            url = 'git+https://hub.com/example-project/',\n
        }\n
        build = {\n
            copy_directories = { 'lib' },\n
        }\n
        "
        .to_string();
        let _rockspec = LuaRockspec::new(&rockspec_content).unwrap_err();
        let rockspec_content = "
        rockspec_format = '1.0'\n
        package = 'foo'\n
        version = '1.0.0-1'\n
        source = {\n
            url = 'git+https://hub.com/example-project/',\n
        }\n
        build = {\n
            copy_directories = { 'rock_manifest' },\n
        }\n
        "
        .to_string();
        let _rockspec = LuaRockspec::new(&rockspec_content).unwrap_err();
        let rockspec_content = "
        rockspec_format = '1.0'\n
        package = 'foo'\n
        version = '1.0.0-1'\n
        source = {\n
            url = 'git+https://hub.com/example-project/foo.zip',\n
            dir = 'baz',\n
        }\n
        build = {\n
            type = 'make',\n
            install = {\n
                lib = {['foo.bar'] = 'lib/bar.so'},\n
            },\n
            copy_directories = {\n
                'plugin',\n
                'ftplugin',\n
            },\n
            patches = {\n
                ['lua51-support.diff'] = [[\n
                    --- before.c\n
                    +++ path/to/after.c\n
                ]],\n
            },\n
        }\n
        "
        .to_string();
        let rockspec = LuaRockspec::new(&rockspec_content).unwrap();
        assert_eq!(rockspec.source.default.unpack_dir, Some("baz".into()));
        assert_eq!(
            rockspec.build.default.build_backend,
            Some(BuildBackendSpec::Make(MakeBuildSpec::default()))
        );
        let foo_bar_path = rockspec
            .build
            .default
            .install
            .lib
            .get(&LuaModule::from_str("foo.bar").unwrap())
            .unwrap();
        assert_eq!(*foo_bar_path, PathBuf::from("lib/bar.so"));
        let copy_directories = rockspec.build.default.copy_directories;
        assert_eq!(
            copy_directories,
            vec![PathBuf::from("plugin"), PathBuf::from("ftplugin")]
        );
        let patches = rockspec.build.default.patches;
        let _patch = patches.get(&PathBuf::from("lua51-support.diff")).unwrap();
        let rockspec_content = "
        rockspec_format = '1.0'\n
        package = 'foo'\n
        version = '1.0.0-1'\n
        source = {\n
            url = 'git+https://hub.com/example-project/foo.zip',\n
        }\n
        build = {\n
            type = 'cmake',\n
        }\n
        "
        .to_string();
        let rockspec = LuaRockspec::new(&rockspec_content).unwrap();
        assert_eq!(
            rockspec.build.default.build_backend,
            Some(BuildBackendSpec::CMake(CMakeBuildSpec::default()))
        );
        let rockspec_content = "
        rockspec_format = '1.0'\n
        package = 'foo'\n
        version = '1.0.0-1'\n
        source = {\n
            url = 'git+https://hub.com/example-project/foo.zip',\n
        }\n
        build = {\n
            type = 'command',\n
            build_command = 'foo',\n
            install_command = 'bar',\n
        }\n
        "
        .to_string();
        let rockspec = LuaRockspec::new(&rockspec_content).unwrap();
        assert!(matches!(
            rockspec.build.default.build_backend,
            Some(BuildBackendSpec::Command(CommandBuildSpec { .. }))
        ));
        let rockspec_content = "
        rockspec_format = '1.0'\n
        package = 'foo'\n
        version = '1.0.0-1'\n
        source = {\n
            url = 'git+https://hub.com/example-project/foo.zip',\n
        }\n
        build = {\n
            type = 'command',\n
            install_command = 'foo',\n
        }\n
        "
        .to_string();
        let _rockspec = LuaRockspec::new(&rockspec_content).unwrap_err();
        let rockspec_content = "
        rockspec_format = '1.0'\n
        package = 'foo'\n
        version = '1.0.0-1'\n
        source = {\n
            url = 'git+https://hub.com/example-project/foo.zip',\n
        }\n
        build = {\n
            type = 'command',\n
            build_command = 'foo',\n
        }\n
        "
        .to_string();
        let _rockspec = LuaRockspec::new(&rockspec_content).unwrap_err();
        // platform overrides
        let rockspec_content = "
        package = 'rocks'\n
        version = '3.0.0-1'\n
        dependencies = {\n
          'neorg ~> 6',\n
          'toml-edit ~> 1',\n
          platforms = {\n
            windows = {\n
              'neorg = 5.0.0',\n
              'toml = 1.0.0',\n
            },\n
            unix = {\n
              'neorg = 5.0.0',\n
            },\n
            linux = {\n
              'toml = 1.0.0',\n
            },\n
          },\n
        }\n
        source = {\n
            url = 'git+https://github.com/nvim-neorocks/rocks.nvim',\n
            hash = 'sha256-uU0nuZNNPgilLlLX2n2r+sSE7+N6U4DukIj3rOLvzek=',\n
        }\n
        "
        .to_string();
        let rockspec = LuaRockspec::new(&rockspec_content).unwrap();
        let neorg_override = PackageSpec::parse("neorg".into(), "5.0.0".into()).unwrap();
        let toml_edit = PackageSpec::parse("toml-edit".into(), "1.0.0".into()).unwrap();
        let toml = PackageSpec::parse("toml".into(), "1.0.0".into()).unwrap();
        assert_eq!(rockspec.dependencies.default.len(), 2);
        let per_platform = &rockspec.dependencies.per_platform;
        assert_eq!(
            per_platform
                .get(&PlatformIdentifier::Windows)
                .unwrap()
                .iter()
                .filter(|dep| dep.matches(&neorg_override)
                    || dep.matches(&toml_edit)
                    || dep.matches(&toml))
                .count(),
            3
        );
        assert_eq!(
            per_platform
                .get(&PlatformIdentifier::Unix)
                .unwrap()
                .iter()
                .filter(|dep| dep.matches(&neorg_override)
                    || dep.matches(&toml_edit)
                    || dep.matches(&toml))
                .count(),
            2
        );
        assert_eq!(
            per_platform
                .get(&PlatformIdentifier::Linux)
                .unwrap()
                .iter()
                .filter(|dep| dep.matches(&neorg_override)
                    || dep.matches(&toml_edit)
                    || dep.matches(&toml))
                .count(),
            3
        );
        let rockspec_content = "
        package = 'rocks'\n
        version = '3.0.0-1'\n
        external_dependencies = {\n
            FOO = { library = 'foo' },\n
            platforms = {\n
              windows = {\n
                FOO = { library = 'foo.dll' },\n
              },\n
              unix = {\n
                BAR = { header = 'bar.h' },\n
              },\n
              linux = {\n
                FOO = { library = 'foo.so' },\n
              },\n
            },\n
        }\n
        source = {\n
            url = 'https://github.com/nvim-neorocks/rocks.nvim/archive/1.0.0/rocks.nvim.zip',\n
        }\n
        "
        .to_string();
        let rockspec = LuaRockspec::new(&rockspec_content).unwrap();
        assert_eq!(
            *rockspec.external_dependencies.default.get("FOO").unwrap(),
            ExternalDependencySpec::Library("foo".into())
        );
        let per_platform = rockspec.external_dependencies.per_platform;
        assert_eq!(
            *per_platform
                .get(&PlatformIdentifier::Windows)
                .and_then(|it| it.get("FOO"))
                .unwrap(),
            ExternalDependencySpec::Library("foo.dll".into())
        );
        assert_eq!(
            *per_platform
                .get(&PlatformIdentifier::Unix)
                .and_then(|it| it.get("FOO"))
                .unwrap(),
            ExternalDependencySpec::Library("foo".into())
        );
        assert_eq!(
            *per_platform
                .get(&PlatformIdentifier::Unix)
                .and_then(|it| it.get("BAR"))
                .unwrap(),
            ExternalDependencySpec::Header("bar.h".into())
        );
        assert_eq!(
            *per_platform
                .get(&PlatformIdentifier::Linux)
                .and_then(|it| it.get("BAR"))
                .unwrap(),
            ExternalDependencySpec::Header("bar.h".into())
        );
        assert_eq!(
            *per_platform
                .get(&PlatformIdentifier::Linux)
                .and_then(|it| it.get("FOO"))
                .unwrap(),
            ExternalDependencySpec::Library("foo.so".into())
        );
        let rockspec_content = "
        rockspec_format = '1.0'\n
        package = 'foo'\n
        version = '1.0.0-1'\n
        source = {\n
            url = 'git+https://hub.com/example-project/.git',\n
            branch = 'bar',\n
            platforms = {\n
                macosx = {\n
                    branch = 'mac',\n
                },\n
                windows = {\n
                    url = 'cvs://foo.cvs',\n
                    module = 'win',\n
                },\n
            },\n
        }\n
        "
        .to_string();
        let rockspec = LuaRockspec::new(&rockspec_content).unwrap();
        assert_eq!(
            rockspec.source.default.source_spec,
            RockSourceSpec::Git(GitSource {
                url: "https://hub.com/example-project/.git".parse().unwrap(),
                checkout_ref: Some("bar".into())
            })
        );
        assert_eq!(
            rockspec
                .source
                .per_platform
                .get(&PlatformIdentifier::MacOSX)
                .map(|it| it.source_spec.clone())
                .unwrap(),
            RockSourceSpec::Git(GitSource {
                url: "https://hub.com/example-project/.git".parse().unwrap(),
                checkout_ref: Some("mac".into())
            })
        );
        assert_eq!(
            rockspec
                .source
                .per_platform
                .get(&PlatformIdentifier::Windows)
                .map(|it| it.source_spec.clone())
                .unwrap(),
            RockSourceSpec::Cvs(CvsSource {
                url: "cvs://foo.cvs".into(),
                module: "win".into(),
            })
        );
        let rockspec_content = "
        rockspec_format = '1.0'\n
        package = 'foo'\n
        version = '1.0.0-1'\n
        source = { url = 'git+https://hub.com/example-project/foo.zip' }\n
        build = {\n
            type = 'make',\n
            install = {\n
                lib = {['foo.bar'] = 'lib/bar.so'},\n
            },\n
            copy_directories = { 'plugin' },\n
            platforms = {\n
                unix = {\n
                    copy_directories = { 'ftplugin' },\n
                },\n
                linux = {\n
                    copy_directories = { 'foo' },\n
                },\n
            },\n
        }\n
        "
        .to_string();
        let rockspec = LuaRockspec::new(&rockspec_content).unwrap();
        let per_platform = rockspec.build.per_platform;
        let unix = per_platform.get(&PlatformIdentifier::Unix).unwrap();
        assert_eq!(
            unix.copy_directories,
            vec![PathBuf::from("plugin"), PathBuf::from("ftplugin")]
        );
        let linux = per_platform.get(&PlatformIdentifier::Linux).unwrap();
        assert_eq!(
            linux.copy_directories,
            vec![
                PathBuf::from("plugin"),
                PathBuf::from("foo"),
                PathBuf::from("ftplugin")
            ]
        );
        let rockspec_content = "
        package = 'foo'\n
        version = '1.0.0-1'\n
        source = { url = 'git+https://hub.com/example-project/foo.zip' }\n
        build = {\n
            type = 'builtin',\n
            modules = {\n
                cjson = {\n
                    sources = { 'lua_cjson.c', 'strbuf.c', 'fpconv.c' },\n
                }\n
            },\n
            platforms = {\n
                win32 = { modules = { cjson = { defines = {\n
                    'DISABLE_INVALID_NUMBERS', 'USE_INTERNAL_ISINF'\n
                } } } }\n
            },\n
        }\n
        "
        .to_string();
        let rockspec = LuaRockspec::new(&rockspec_content).unwrap();
        let per_platform = rockspec.build.per_platform;
        let win32 = per_platform.get(&PlatformIdentifier::Windows).unwrap();
        assert_eq!(
            win32.build_backend,
            Some(BuildBackendSpec::Builtin(BuiltinBuildSpec {
                modules: vec![(
                    LuaModule::from_str("cjson").unwrap(),
                    ModuleSpec::ModulePaths(ModulePaths {
                        sources: vec!["lua_cjson.c".into(), "strbuf.c".into(), "fpconv.c".into()],
                        libraries: Vec::default(),
                        defines: vec![
                            ("DISABLE_INVALID_NUMBERS".into(), None),
                            ("USE_INTERNAL_ISINF".into(), None)
                        ],
                        incdirs: Vec::default(),
                        libdirs: Vec::default(),
                    })
                )]
                .into_iter()
                .collect()
            }))
        );
    }

    #[tokio::test]
    pub async fn parse_scm_rockspec() {
        let rockspec_content = "
        package = 'foo'\n
        version = 'scm-1'\n
        source = {\n
            url = 'https://github.com/nvim-neorocks/rocks.nvim/archive/1.0.0/rocks.nvim.zip',\n
        }\n
        "
        .to_string();
        let rockspec = LuaRockspec::new(&rockspec_content).unwrap();
        assert_eq!(rockspec.package, "foo".into());
        assert_eq!(rockspec.version, "scm-1".parse().unwrap());
    }

    #[tokio::test]
    pub async fn regression_luasystem() {
        let rockspec_content = "
local package_name = 'luasystem'
local package_version = '0.4.4'
local rockspec_revision = '1'
local github_account_name = 'lunarmodules'
local github_repo_name = 'luasystem'


package = package_name
version = package_version..'-'..rockspec_revision

source = {
  url = 'git+https://github.com/'..github_account_name..'/'..github_repo_name..'.git',
  branch = (package_version == 'scm') and 'master' or nil,
  tag = (package_version ~= 'scm') and 'v'..package_version or nil,
}

description = {
  summary = 'Platform independent system calls for Lua.',
  detailed = [[
    Adds a Lua API for making platform independent system calls.
  ]],
  license = 'MIT <http://opensource.org/licenses/MIT>',
  homepage = 'https://github.com/'..github_account_name..'/'..github_repo_name,
}

dependencies = {
  'lua >= 5.1',
}

local function make_platform(plat)
  local defines = {
    linux = { },
    unix = { },
    macosx = { },
    win32 = { 'WINVER=0x0600', '_WIN32_WINNT=0x0600' },
    mingw32 = { 'WINVER=0x0600', '_WIN32_WINNT=0x0600' },
  }
  local libraries = {
    linux = { 'rt' },
    unix = { },
    macosx = { },
    win32 = { 'advapi32', 'winmm' },
    mingw32 = { },
  }
  local libdirs = {
    linux = nil,
    unix = nil,
    macosx = nil,
    win32 = nil,
    mingw32 = { },
  }
  return {
    modules = {
      ['system.core'] = {
        sources = {
          'src/core.c',
          'src/compat.c',
          'src/time.c',
          'src/environment.c',
          'src/random.c',
          'src/term.c',
          'src/bitflags.c',
          'src/wcwidth.c',
        },
        defines = defines[plat],
        libraries = libraries[plat],
        libdirs = libdirs[plat],
      },
    },
  }
end

build = {
  type = 'builtin',
  platforms = {
    linux = make_platform('linux'),
    unix = make_platform('unix'),
    macosx = make_platform('macosx'),
    win32 = make_platform('win32'),
    mingw32 = make_platform('mingw32'),
  },
  modules = {
    ['system.init'] = 'system/init.lua',
  },
  copy_directories = {},
}
";
        let rockspec = LuaRockspec::new(rockspec_content).unwrap();
        let build_spec = rockspec.build.current_platform();
        assert!(matches!(
            build_spec.build_backend,
            Some(BuildBackendSpec::Builtin { .. })
        ));
        if let Some(BuildBackendSpec::Builtin(BuiltinBuildSpec { modules })) =
            &build_spec.build_backend
        {
            assert_eq!(
                modules.get(&LuaModule::from_str("system.init").unwrap()),
                Some(&ModuleSpec::SourcePath("system/init.lua".into()))
            );
            assert_eq!(
                modules.get(&LuaModule::from_str("system.core").unwrap()),
                Some(&ModuleSpec::ModulePaths(ModulePaths {
                    sources: vec![
                        "src/core.c".into(),
                        "src/compat.c".into(),
                        "src/time.c".into(),
                        "src/environment.c".into(),
                        "src/random.c".into(),
                        "src/term.c".into(),
                        "src/bitflags.c".into(),
                        "src/wcwidth.c".into(),
                    ],
                    defines: luasystem_expected_defines(),
                    libraries: luasystem_expected_libraries(),
                    incdirs: luasystem_expected_incdirs(),
                    libdirs: luasystem_expected_libdirs(),
                }))
            );
        }
    }

    fn luasystem_expected_defines() -> Vec<(String, Option<String>)> {
        if cfg!(target_os = "windows") {
            vec![
                ("WINVER".into(), Some("0x0600".into())),
                ("_WIN32_WINNT".into(), Some("0x0600".into())),
            ]
        } else {
            Vec::default()
        }
    }

    fn luasystem_expected_libraries() -> Vec<PathBuf> {
        if cfg!(target_os = "linux") {
            vec!["rt".into()]
        } else if cfg!(target_os = "windows") {
            vec!["advapi32".into(), "winmm".into()]
        } else {
            Vec::default()
        }
    }

    fn luasystem_expected_incdirs() -> Vec<PathBuf> {
        Vec::default()
    }

    fn luasystem_expected_libdirs() -> Vec<PathBuf> {
        Vec::default()
    }

    #[tokio::test]
    pub async fn rust_mlua_rockspec() {
        let rockspec_content = "
    package = 'foo'\n
    version = 'scm-1'\n
    source = {\n
        url = 'https://github.com/nvim-neorocks/rocks.nvim/archive/1.0.0/rocks.nvim.zip',\n
    }\n
    build = {
        type = 'rust-mlua',
        modules = {
            'foo',
            bar = 'baz',
        },
        target_path = 'path/to/cargo/target/directory',
        default_features = false,
        include = {
            'file.lua',
            ['path/to/another/file.lua'] = 'another-file.lua',
        },
        features = {'extra', 'features'},
    }
            ";
        let rockspec = LuaRockspec::new(rockspec_content).unwrap();
        let build_spec = rockspec.build.current_platform();
        if let Some(BuildBackendSpec::RustMlua(build_spec)) = build_spec.build_backend.to_owned() {
            assert_eq!(
                build_spec.modules.get("foo").unwrap(),
                &PathBuf::from(format!("libfoo.{}", std::env::consts::DLL_EXTENSION))
            );
            assert_eq!(
                build_spec.modules.get("bar").unwrap(),
                &PathBuf::from(format!("libbaz.{}", std::env::consts::DLL_EXTENSION))
            );
            assert_eq!(
                build_spec.include.get(&PathBuf::from("file.lua")).unwrap(),
                &PathBuf::from("file.lua")
            );
            assert_eq!(
                build_spec
                    .include
                    .get(&PathBuf::from("path/to/another/file.lua"))
                    .unwrap(),
                &PathBuf::from("another-file.lua")
            );
        } else {
            panic!("Expected RustMlua build backend");
        }
    }
}
