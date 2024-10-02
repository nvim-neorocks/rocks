mod build;
mod dependency;
mod platform;
mod rock_source;
mod test_spec;

use std::{collections::HashMap, path::PathBuf, str::FromStr};

use eyre::{eyre, Result};
use mlua::{FromLua, Lua, LuaSerdeExt, Value};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

pub use build::*;
pub use dependency::*;
pub use platform::*;
pub use rock_source::*;
pub use test_spec::*;

use crate::{
    config::LuaVersion,
    lua_package::{LuaPackageReq, PackageName, PackageVersion},
};

#[derive(Debug)]
pub struct Rockspec {
    /// The file format version. Example: "1.0"
    pub rockspec_format: Option<RockspecFormat>,
    /// The name of the package. Example: "luasocket"
    pub package: PackageName,
    /// The version of the package, plus a suffix indicating the revision of the rockspec. Example: "2.0.1-1"
    pub version: PackageVersion,
    pub description: RockDescription,
    pub supported_platforms: PlatformSupport,
    pub dependencies: PerPlatform<Vec<LuaPackageReq>>,
    pub build_dependencies: PerPlatform<Vec<LuaPackageReq>>,
    pub external_dependencies: PerPlatform<HashMap<String, ExternalDependency>>,
    pub test_dependencies: PerPlatform<Vec<LuaPackageReq>>,
    pub source: PerPlatform<RockSource>,
    pub build: PerPlatform<BuildSpec>,
    pub test: PerPlatform<TestSpec>,
}

impl Rockspec {
    pub fn new(rockspec_content: &String) -> Result<Self> {
        let lua = Lua::new();
        lua.load(rockspec_content).exec()?;

        let globals = lua.globals();
        let rockspec = Rockspec {
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
        };

        let rockspec_file_name = format!("{}-{}.rockspec", rockspec.package, rockspec.version);
        if rockspec
            .build
            .default
            .copy_directories
            .contains(&PathBuf::from(&rockspec_file_name))
        {
            return Err(eyre!("copy_directories cannot contain the rockspec name!"));
        }

        for (platform, build_override) in &rockspec.build.per_platform {
            if build_override
                .copy_directories
                .contains(&PathBuf::from(&rockspec_file_name))
            {
                return Err(eyre!(
                    "platform {}: copy_directories cannot contain the rockspec name!",
                    platform
                ));
            }
        }
        Ok(rockspec)
    }

    pub fn lua_version(&self) -> Option<LuaVersion> {
        self.dependencies
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
    #[serde(default)]
    pub labels: Vec<String>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RockspecFormat {
    #[serde(rename = "1.0")]
    _1_0,
    #[serde(rename = "2.0")]
    _2_0,
    #[serde(rename = "3.0")]
    _3_0,
}

impl FromStr for RockspecFormat {
    type Err = eyre::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "1.0" => Ok(Self::_1_0),
            "2.0" => Ok(Self::_2_0),
            "3.0" => Ok(Self::_3_0),
            txt => Err(eyre!("Invalid rockspec format: {}", txt)),
        }
    }
}

impl From<&str> for RockspecFormat {
    fn from(s: &str) -> Self {
        Self::from_str(s).unwrap()
    }
}

impl<'lua> FromLua<'lua> for RockspecFormat {
    fn from_lua(
        value: mlua::prelude::LuaValue<'lua>,
        lua: &'lua mlua::prelude::Lua,
    ) -> mlua::prelude::LuaResult<Self> {
        let s = String::from_lua(value, lua)?;
        Self::from_str(&s).map_err(|err| mlua::Error::DeserializeError(err.to_string()))
    }
}

fn parse_lua_tbl_or_default<T>(lua: &Lua, lua_var_name: &str) -> Result<T>
where
    T: Default,
    T: DeserializeOwned,
{
    let ret = match lua.globals().get(lua_var_name)? {
        Value::Nil => T::default(),
        value @ Value::Table(_) => lua.from_value(value)?,
        value => Err(eyre!(format!(
            "Could not parse {}. Expected list, but got {}",
            lua_var_name,
            value.type_name(),
        )))?,
    };
    Ok(ret)
}

#[cfg(test)]
mod tests {

    use std::path::PathBuf;

    use crate::lua_package::LuaPackage;
    use crate::rockspec::PlatformIdentifier;

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
        let rockspec = Rockspec::new(&rockspec_content).unwrap();
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
        let rockspec = Rockspec::new(&rockspec_content).unwrap();
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
        let rockspec = Rockspec::new(&rockspec_content).unwrap();
        assert_eq!(rockspec.rockspec_format, None);
        assert_eq!(rockspec.package, "rocks".into());
        assert_eq!(rockspec.version, "3.0.0-1".parse().unwrap());
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
        external_dependencies = { FOO = { library = 'foo' } }\n
        source = {\n
            url = 'https://github.com/nvim-neorocks/rocks.nvim/archive/1.0.0/rocks.nvim.zip',\n
        }\n
        "
        .to_string();
        let rockspec = Rockspec::new(&rockspec_content).unwrap();
        assert_eq!(rockspec.rockspec_format, None);
        assert_eq!(rockspec.package, "rocks".into());
        assert_eq!(rockspec.version, "3.0.0-1".parse().unwrap());
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
        assert_eq!(
            *rockspec.external_dependencies.default.get("FOO").unwrap(),
            ExternalDependency::Library("foo".into())
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
            url = 'git://github.com/nvim-neorocks/rocks.nvim',\n
            hash = 'sha256-uU0nuZNNPgilLlLX2n2r+sSE7+N6U4DukIj3rOLvzek=',\n
        }\n
        "
        .to_string();
        let rockspec = Rockspec::new(&rockspec_content).unwrap();
        assert_eq!(rockspec.rockspec_format, None);
        assert_eq!(rockspec.package, "rocks".into());
        assert_eq!(rockspec.version, "3.0.0-1".parse().unwrap());
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
        let neorg = LuaPackage::parse("neorg".into(), "6.0.0".into()).unwrap();
        assert!(rockspec
            .dependencies
            .default
            .into_iter()
            .any(|dep| dep.matches(&neorg)));
        let foo = LuaPackage::parse("foo".into(), "1.0.0".into()).unwrap();
        assert!(rockspec
            .build_dependencies
            .default
            .into_iter()
            .any(|dep| dep.matches(&foo)));
        let busted = LuaPackage::parse("busted".into(), "2.2.0".into()).unwrap();
        assert_eq!(
            *rockspec.external_dependencies.default.get("FOO").unwrap(),
            ExternalDependency::Header("foo.h".into())
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
            url = 'git://hub.com/example-project/',\n
            branch = 'bar',\n
        }\n
        "
        .to_string();
        let rockspec = Rockspec::new(&rockspec_content).unwrap();
        assert_eq!(
            rockspec.source.default.source_spec,
            RockSourceSpec::Git(GitSource {
                url: "git://hub.com/example-project/".parse().unwrap(),
                checkout_ref: Some("bar".into())
            })
        );
        assert_eq!(rockspec.test, PerPlatform::default());
        let rockspec_content = "
        rockspec_format = '1.0'\n
        package = 'foo'\n
        version = '1.0.0-1'\n
        source = {\n
            url = 'git://hub.com/example-project/',\n
            tag = 'bar',\n
        }\n
        "
        .to_string();
        let rockspec = Rockspec::new(&rockspec_content).unwrap();
        assert_eq!(
            rockspec.source.default.source_spec,
            RockSourceSpec::Git(GitSource {
                url: "git://hub.com/example-project/".parse().unwrap(),
                checkout_ref: Some("bar".into())
            })
        );
        let rockspec_content = "
        rockspec_format = '1.0'\n
        package = 'foo'\n
        version = '1.0.0-1'\n
        source = {\n
            url = 'git://hub.com/example-project/',\n
            branch = 'bar',\n
            tag = 'baz',\n
        }\n
        "
        .to_string();
        let _rockspec = Rockspec::new(&rockspec_content).unwrap_err();
        let rockspec_content = "
        rockspec_format = '1.0'\n
        package = 'foo'\n
        version = '1.0.0-1'\n
        source = {\n
            url = 'git://hub.com/example-project/',\n
            module = 'bar',\n
        }\n
        "
        .to_string();
        let _rockspec = Rockspec::new(&rockspec_content).unwrap_err();
        let rockspec_content = "
        rockspec_format = '1.0'\n
        package = 'foo'\n
        version = '1.0.0-1'\n
        source = {\n
            url = 'git://hub.com/example-project/',\n
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
        let rockspec = Rockspec::new(&rockspec_content).unwrap();
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
            url = 'git://hub.com/example-project/foo.zip',\n
        }\n
        build = {\n
            install = {\n
                lua = {['foo.bar'] = 'src/bar.lua'},\n
                bin = {['foo.bar'] = 'bin/bar'},\n
            },\n
        }\n
        "
        .to_string();
        let rockspec = Rockspec::new(&rockspec_content).unwrap();
        assert!(matches!(
            rockspec.build.default.build_backend,
            Some(BuildBackendSpec::Builtin { .. })
        ));
        let foo_bar_path = rockspec.build.default.install.lua.get("foo.bar").unwrap();
        assert_eq!(*foo_bar_path, PathBuf::from("src/bar.lua"));
        let foo_bar_path = rockspec.build.default.install.bin.get("foo.bar").unwrap();
        assert_eq!(*foo_bar_path, PathBuf::from("bin/bar"));
        let rockspec_content = "
        rockspec_format = '1.0'\n
        package = 'foo'\n
        version = '1.0.0-1'\n
        source = {\n
            url = 'git://hub.com/example-project/',\n
        }\n
        build = {\n
            copy_directories = { 'lua' },\n
        }\n
        "
        .to_string();
        let _rockspec = Rockspec::new(&rockspec_content).unwrap_err();
        let rockspec_content = "
        rockspec_format = '1.0'\n
        package = 'foo'\n
        version = '1.0.0-1'\n
        source = {\n
            url = 'git://hub.com/example-project/',\n
        }\n
        build = {\n
            copy_directories = { 'lib' },\n
        }\n
        "
        .to_string();
        let _rockspec = Rockspec::new(&rockspec_content).unwrap_err();
        let rockspec_content = "
        rockspec_format = '1.0'\n
        package = 'foo'\n
        version = '1.0.0-1'\n
        source = {\n
            url = 'git://hub.com/example-project/',\n
        }\n
        build = {\n
            copy_directories = { 'rock_manifest' },\n
        }\n
        "
        .to_string();
        let _rockspec = Rockspec::new(&rockspec_content).unwrap_err();
        let rockspec_content = "
        rockspec_format = '1.0'\n
        package = 'foo'\n
        version = '1.0.0-1'\n
        source = {\n
            url = 'git://hub.com/example-project/foo.zip',\n
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
        let rockspec = Rockspec::new(&rockspec_content).unwrap();
        assert_eq!(rockspec.source.default.unpack_dir, Some("baz".into()));
        assert_eq!(
            rockspec.build.default.build_backend,
            Some(BuildBackendSpec::Make(MakeBuildSpec::default()))
        );
        let foo_bar_path = rockspec.build.default.install.lib.get("foo.bar").unwrap();
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
            url = 'git://hub.com/example-project/foo.zip',\n
        }\n
        build = {\n
            type = 'cmake',\n
        }\n
        "
        .to_string();
        let rockspec = Rockspec::new(&rockspec_content).unwrap();
        assert_eq!(
            rockspec.build.default.build_backend,
            Some(BuildBackendSpec::CMake(CMakeBuildSpec::default()))
        );
        let rockspec_content = "
        rockspec_format = '1.0'\n
        package = 'foo'\n
        version = '1.0.0-1'\n
        source = {\n
            url = 'git://hub.com/example-project/foo.zip',\n
        }\n
        build = {\n
            type = 'command',\n
            build_command = 'foo',\n
            install_command = 'bar',\n
        }\n
        "
        .to_string();
        let rockspec = Rockspec::new(&rockspec_content).unwrap();
        assert!(matches!(
            rockspec.build.default.build_backend,
            Some(BuildBackendSpec::Command(CommandBuildSpec { .. }))
        ));
        let rockspec_content = "
        rockspec_format = '1.0'\n
        package = 'foo'\n
        version = '1.0.0-1'\n
        source = {\n
            url = 'git://hub.com/example-project/foo.zip',\n
        }\n
        build = {\n
            type = 'command',\n
            install_command = 'foo',\n
        }\n
        "
        .to_string();
        let _rockspec = Rockspec::new(&rockspec_content).unwrap_err();
        let rockspec_content = "
        rockspec_format = '1.0'\n
        package = 'foo'\n
        version = '1.0.0-1'\n
        source = {\n
            url = 'git://hub.com/example-project/foo.zip',\n
        }\n
        build = {\n
            type = 'command',\n
            build_command = 'foo',\n
        }\n
        "
        .to_string();
        let _rockspec = Rockspec::new(&rockspec_content).unwrap_err();
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
            url = 'git://github.com/nvim-neorocks/rocks.nvim',\n
            hash = 'sha256-uU0nuZNNPgilLlLX2n2r+sSE7+N6U4DukIj3rOLvzek=',\n
        }\n
        "
        .to_string();
        let rockspec = Rockspec::new(&rockspec_content).unwrap();
        let neorg_override = LuaPackage::parse("neorg".into(), "5.0.0".into()).unwrap();
        let toml_edit = LuaPackage::parse("toml-edit".into(), "1.0.0".into()).unwrap();
        let toml = LuaPackage::parse("toml".into(), "1.0.0".into()).unwrap();
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
        let rockspec = Rockspec::new(&rockspec_content).unwrap();
        assert_eq!(
            *rockspec.external_dependencies.default.get("FOO").unwrap(),
            ExternalDependency::Library("foo".into())
        );
        let per_platform = rockspec.external_dependencies.per_platform;
        assert_eq!(
            *per_platform
                .get(&PlatformIdentifier::Windows)
                .and_then(|it| it.get("FOO"))
                .unwrap(),
            ExternalDependency::Library("foo.dll".into())
        );
        assert_eq!(
            *per_platform
                .get(&PlatformIdentifier::Unix)
                .and_then(|it| it.get("FOO"))
                .unwrap(),
            ExternalDependency::Library("foo".into())
        );
        assert_eq!(
            *per_platform
                .get(&PlatformIdentifier::Unix)
                .and_then(|it| it.get("BAR"))
                .unwrap(),
            ExternalDependency::Header("bar.h".into())
        );
        assert_eq!(
            *per_platform
                .get(&PlatformIdentifier::Linux)
                .and_then(|it| it.get("BAR"))
                .unwrap(),
            ExternalDependency::Header("bar.h".into())
        );
        assert_eq!(
            *per_platform
                .get(&PlatformIdentifier::Linux)
                .and_then(|it| it.get("FOO"))
                .unwrap(),
            ExternalDependency::Library("foo.so".into())
        );
        let rockspec_content = "
        rockspec_format = '1.0'\n
        package = 'foo'\n
        version = '1.0.0-1'\n
        source = {\n
            url = 'git://hub.com/example-project/.git',\n
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
        let rockspec = Rockspec::new(&rockspec_content).unwrap();
        assert_eq!(
            rockspec.source.default.source_spec,
            RockSourceSpec::Git(GitSource {
                url: "git://hub.com/example-project/.git".parse().unwrap(),
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
                url: "git://hub.com/example-project/.git".parse().unwrap(),
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
        source = { url = 'git://hub.com/example-project/foo.zip' }\n
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
        let rockspec = Rockspec::new(&rockspec_content).unwrap();
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
        source = { url = 'git://hub.com/example-project/foo.zip' }\n
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
        let rockspec = Rockspec::new(&rockspec_content).unwrap();
        let per_platform = rockspec.build.per_platform;
        let win32 = per_platform.get(&PlatformIdentifier::Windows).unwrap();
        assert_eq!(
            win32.build_backend,
            Some(BuildBackendSpec::Builtin(BuiltinBuildSpec {
                modules: vec![(
                    "cjson".into(),
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
        let rockspec = Rockspec::new(&rockspec_content).unwrap();
        assert_eq!(rockspec.package, "foo".into());
        assert_eq!(rockspec.version, "scm-1".parse().unwrap());
    }
}
