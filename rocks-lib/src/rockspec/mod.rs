mod build;
mod dependency;
mod platform;
mod rock_source;

use std::{collections::HashMap, path::PathBuf};

use eyre::{eyre, Result};
use mlua::{Lua, LuaSerdeExt, Value};
use serde::{de::DeserializeOwned, Deserialize};

pub use build::*;
pub use dependency::*;
pub use platform::*;
pub use rock_source::*;

#[derive(Debug)]
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
    pub external_dependencies: HashMap<String, ExternalDependency>,
    pub test_dependencies: Vec<LuaDependency>,
    pub source: RockSource,
    pub build: BuildSpec,
}

impl Rockspec {
    pub fn new(rockspec_content: &String) -> Result<Self> {
        let lua = Lua::new();
        lua.load(rockspec_content).exec()?;
        let rockspec = Rockspec {
            rockspec_format: lua.from_value(lua.globals().get("rockspec_format")?)?,
            package: lua.from_value(lua.globals().get("package")?)?,
            version: lua.from_value(lua.globals().get("version")?)?,
            description: parse_lua_tbl_or_default(&lua, "description")?,
            supported_platforms: parse_lua_tbl_or_default(&lua, "supported_platforms")?,
            // TODO(mrcjkb): support per-platform overrides: https://github.com/luarocks/luarocks/wiki/platform-overrides
            dependencies: parse_lua_tbl_or_default(&lua, "dependencies")?,
            build_dependencies: parse_lua_tbl_or_default(&lua, "build_dependencies")?,
            test_dependencies: parse_lua_tbl_or_default(&lua, "test_dependencies")?,
            external_dependencies: parse_lua_tbl_or_default(&lua, "external_dependencies")?,
            source: lua.from_value(lua.globals().get("source")?)?,
            build: parse_lua_tbl_or_default(&lua, "build")?,
        };
        let rockspec_file_name = format!("{}-{}.rockspec", rockspec.package, rockspec.version);
        if rockspec
            .build
            .copy_directories
            .contains(&PathBuf::from(rockspec_file_name.clone()))
        {
            return Err(eyre!("copy_directories cannot contain the rockspec name!"));
        }
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
    #[serde(default)]
    pub labels: Vec<String>,
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

    use crate::rockspec::{LuaRock, PlatformIdentifier};

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
        assert_eq!(rockspec.package, "foo");
        assert_eq!(rockspec.version, "1.0.0-1");
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
        source = {\n
            url = 'https://github.com/nvim-neorocks/rocks.nvim/archive/1.0.0/rocks.nvim.zip',\n
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
        external_dependencies = { FOO = { library = 'foo' } }\n
        source = {\n
            url = 'https://github.com/nvim-neorocks/rocks.nvim/archive/1.0.0/rocks.nvim.zip',\n
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
        assert_eq!(
            *rockspec.external_dependencies.get("FOO").unwrap(),
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
        let busted = LuaRock::new("busted".into(), "2.2.0".into()).unwrap();
        assert_eq!(
            *rockspec.external_dependencies.get("FOO").unwrap(),
            ExternalDependency::Header("foo.h".into())
        );
        assert!(rockspec
            .test_dependencies
            .into_iter()
            .any(|dep| dep.matches(&busted)));

        let rockspec_content = "
        rockspec_format = '1.0'\n
        package = 'foo'\n
        version = '1.0.0-1'\n
        source = {\n
            url = 'git://foo',\n
            branch = 'bar',\n
        }\n
        "
        .to_string();
        let rockspec = Rockspec::new(&rockspec_content).unwrap();
        assert_eq!(
            rockspec.source.source_spec,
            RockSourceSpec::Git(GitSource {
                url: "git://foo".into(),
                checkout_ref: Some("bar".into())
            })
        );
        let rockspec_content = "
        rockspec_format = '1.0'\n
        package = 'foo'\n
        version = '1.0.0-1'\n
        source = {\n
            url = 'git://foo',\n
            tag = 'bar',\n
        }\n
        "
        .to_string();
        let rockspec = Rockspec::new(&rockspec_content).unwrap();
        assert_eq!(
            rockspec.source.source_spec,
            RockSourceSpec::Git(GitSource {
                url: "git://foo".into(),
                checkout_ref: Some("bar".into())
            })
        );
        let rockspec_content = "
        rockspec_format = '1.0'\n
        package = 'foo'\n
        version = '1.0.0-1'\n
        source = {\n
            url = 'git://foo',\n
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
            url = 'git://foo',\n
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
            url = 'git://foo',\n
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
        assert_eq!(rockspec.source.archive_name, "foo.tar.gz");
        let foo_bar_path = rockspec.build.install.conf.get("foo.bar").unwrap();
        assert_eq!(*foo_bar_path, PathBuf::from("config/bar.toml"));
        let rockspec_content = "
        rockspec_format = '1.0'\n
        package = 'foo'\n
        version = '1.0.0-1'\n
        source = {\n
            url = 'git://foo.zip',\n
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
        assert_eq!(rockspec.source.archive_name, "foo.zip");
        assert_eq!(rockspec.source.unpack_dir, "foo");
        assert_eq!(rockspec.build.build_type, BuildType::Builtin);
        let foo_bar_path = rockspec.build.install.lua.get("foo.bar").unwrap();
        assert_eq!(*foo_bar_path, PathBuf::from("src/bar.lua"));
        let foo_bar_path = rockspec.build.install.bin.get("foo.bar").unwrap();
        assert_eq!(*foo_bar_path, PathBuf::from("bin/bar"));
        let rockspec_content = "
        rockspec_format = '1.0'\n
        package = 'foo'\n
        version = '1.0.0-1'\n
        source = {\n
            url = 'git://foo',\n
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
            url = 'git://foo',\n
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
            url = 'git://foo',\n
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
            url = 'git://foo',\n
        }\n
        build = {\n
            copy_directories = { 'foo-1.0.0-1.rockspec' },\n
        }\n
        "
        .to_string();
        let _rockspec = Rockspec::new(&rockspec_content).unwrap_err();
        let rockspec_content = "
        rockspec_format = '1.0'\n
        package = 'foo'\n
        version = '1.0.0-1'\n
        source = {\n
            url = 'git://foo.zip',\n
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
        }\n
        "
        .to_string();
        let rockspec = Rockspec::new(&rockspec_content).unwrap();
        assert_eq!(rockspec.source.archive_name, "foo.zip");
        assert_eq!(rockspec.source.unpack_dir, "baz");
        assert_eq!(rockspec.build.build_type, BuildType::Make);
        let foo_bar_path = rockspec.build.install.lib.get("foo.bar").unwrap();
        assert_eq!(*foo_bar_path, PathBuf::from("lib/bar.so"));
        let copy_directories = rockspec.build.copy_directories;
        assert_eq!(
            copy_directories,
            vec![PathBuf::from("plugin"), PathBuf::from("ftplugin")]
        );
    }
}
