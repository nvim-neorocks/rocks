//! Structs and utilities for `rocks.toml`

use std::collections::HashMap;

use itertools::Itertools;
use serde::{Deserialize, Deserializer};
use thiserror::Error;

use crate::{
    config::{Config, LuaVersion},
    lua_rockspec::{
        BuildSpec, BuildSpecInternal, BuildSpecInternalError, DisplayAsLuaKV, ExternalDependencies,
        ExternalDependencySpec, FromPlatformOverridable, LuaRockspec, LuaVersionError, PerPlatform,
        PlatformIdentifier, PlatformSupport, PlatformValidationError, RockDescription, RockSource,
        RockSourceError, RockSourceInternal, RockspecError, RockspecFormat, TestSpec,
        TestSpecError, TestSpecInternal,
    },
    package::{
        BuildDependencies, Dependencies, PackageName, PackageReq, PackageVersion,
        PackageVersionReq, TestDependencies,
    },
    rockspec::{latest_lua_version, LuaVersionCompatibility, Rockspec},
};

fn parse_map_to_package_vec_opt<'de, D>(
    deserializer: D,
) -> Result<Option<Vec<PackageReq>>, D::Error>
where
    D: Deserializer<'de>,
{
    let packages: Option<HashMap<PackageName, PackageVersionReq>> =
        Option::deserialize(deserializer)?;

    Ok(packages.map(|pkgs| {
        pkgs.into_iter()
            .map(|(name, version_req)| PackageReq { name, version_req })
            .collect()
    }))
}

#[derive(Debug, Error)]
pub enum RocksTomlValidationError {
    #[error("no source url provided")]
    NoSource,
    #[error("no lua version provided")]
    NoLuaVersion,
    #[error(transparent)]
    RockSourceError(#[from] RockSourceError),
    #[error(transparent)]
    TestSpecError(#[from] TestSpecError),
    #[error(transparent)]
    BuildSpecInternal(#[from] BuildSpecInternalError),
    #[error(transparent)]
    PlatformValidationError(#[from] PlatformValidationError),
}

/// The `rocks.toml` file.
/// The only required fields are `package` and `build`, which are required to build a project using `rocks build`.
/// The rest of the fields are optional, but are required to build a rockspec.
#[derive(Clone, Debug, Deserialize)]
pub struct RocksToml {
    pub(crate) package: PackageName,
    pub(crate) version: PackageVersion,
    pub(crate) build: BuildSpecInternal,
    #[serde(default)]
    pub(crate) lua: Option<PackageVersionReq>,
    #[serde(default)]
    pub(crate) description: Option<RockDescription>,
    #[serde(default)]
    pub(crate) supported_platforms: Option<HashMap<PlatformIdentifier, bool>>,
    #[serde(default, deserialize_with = "parse_map_to_package_vec_opt")]
    pub(crate) dependencies: Option<Vec<PackageReq>>,
    #[serde(default, deserialize_with = "parse_map_to_package_vec_opt")]
    pub(crate) build_dependencies: Option<Vec<PackageReq>>,
    #[serde(default)]
    pub(crate) external_dependencies: Option<HashMap<String, ExternalDependencySpec>>,
    #[serde(default, deserialize_with = "parse_map_to_package_vec_opt")]
    pub(crate) test_dependencies: Option<Vec<PackageReq>>,
    #[serde(default)]
    pub(crate) source: Option<RockSourceInternal>,
    #[serde(default)]
    pub(crate) test: Option<TestSpecInternal>,
}

/// Equivalent to [`RocksToml`], but with all fields required
#[derive(Debug)]
pub struct RocksTomlValidated {
    package: PackageName,
    version: PackageVersion,
    lua: PackageVersionReq,
    description: RockDescription,
    supported_platforms: PlatformSupport,
    dependencies: PerPlatform<Vec<PackageReq>>,
    build_dependencies: PerPlatform<Vec<PackageReq>>,
    external_dependencies: PerPlatform<HashMap<String, ExternalDependencySpec>>,
    test_dependencies: PerPlatform<Vec<PackageReq>>,
    source: PerPlatform<RockSource>,
    test: PerPlatform<TestSpec>,
    build: PerPlatform<BuildSpec>,

    // Used for simpler serialization
    internal: RocksToml,
}

impl RocksToml {
    pub fn new(str: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(str)
    }

    pub fn into_validated_rocks_toml(
        &self,
    ) -> Result<RocksTomlValidated, RocksTomlValidationError> {
        let rocks = self.clone();

        Ok(RocksTomlValidated {
            internal: rocks.clone(),

            package: rocks.package,
            version: rocks.version,
            lua: rocks.lua.ok_or(RocksTomlValidationError::NoLuaVersion)?,
            description: rocks.description.unwrap_or_default(),
            supported_platforms: PlatformSupport::parse(
                &rocks
                    .supported_platforms
                    .unwrap_or_default()
                    .into_iter()
                    .map(|(platform, supported)| {
                        if supported {
                            format!("{}", platform)
                        } else {
                            format!("!{}", platform)
                        }
                    })
                    .collect_vec(),
            )?,
            dependencies: PerPlatform::new(rocks.dependencies.unwrap_or_default()),
            build_dependencies: PerPlatform::new(rocks.build_dependencies.unwrap_or_default()),
            external_dependencies: PerPlatform::new(
                rocks.external_dependencies.unwrap_or_default(),
            ),
            test_dependencies: PerPlatform::new(rocks.test_dependencies.unwrap_or_default()),
            source: PerPlatform::new(RockSource::from_platform_overridable(
                rocks.source.ok_or(RocksTomlValidationError::NoSource)?,
            )?),
            test: PerPlatform::new(TestSpec::from_platform_overridable(
                rocks.test.clone().unwrap_or_default(),
            )?),
            build: PerPlatform::new(BuildSpec::from_internal_spec(rocks.build.clone())?),
        })
    }

    // In the not-yet-validated struct, we create getters only
    // for the non-optional fields.
    pub fn package(&self) -> &PackageName {
        &self.package
    }

    pub fn version(&self) -> &PackageVersion {
        &self.version
    }
}

impl RocksTomlValidated {
    pub fn to_rockspec_string(&self) -> String {
        let starter = format!(
            r#"
rockspec_format = "3.0"
package = "{}"
version = "{}""#,
            self.package, self.version
        );

        let mut template = Vec::new();

        if self.description != RockDescription::default() {
            template.push(self.description.display_lua());
        }

        if self.supported_platforms != PlatformSupport::default() {
            template.push(self.supported_platforms.display_lua());
        }

        {
            let mut dependencies = self.internal.dependencies.clone().unwrap_or_default();
            dependencies.insert(
                0,
                PackageReq {
                    name: "lua".into(),
                    version_req: self.lua.clone(),
                },
            );
            template.push(Dependencies(&dependencies).display_lua());
        }

        match self.internal.build_dependencies {
            Some(ref build_dependencies) if !build_dependencies.is_empty() => {
                template.push(BuildDependencies(build_dependencies).display_lua());
            }
            _ => {}
        }

        match self.internal.external_dependencies {
            Some(ref external_dependencies) if !external_dependencies.is_empty() => {
                template.push(ExternalDependencies(external_dependencies).display_lua());
            }
            _ => {}
        }

        match self.internal.test_dependencies {
            Some(ref test_dependencies) if !test_dependencies.is_empty() => {
                template.push(TestDependencies(test_dependencies).display_lua());
            }
            _ => {}
        }

        if let Some(ref source) = self.internal.source {
            template.push(source.display_lua());
        }

        if let Some(ref test) = self.internal.test {
            template.push(test.display_lua());
        }

        template.push(self.internal.build.display_lua());

        std::iter::once(starter)
            .chain(template.into_iter().map(|kv| kv.to_string()))
            .join("\n\n")
    }

    pub fn to_rockspec(&self) -> Result<LuaRockspec, RockspecError> {
        LuaRockspec::new(&self.to_rockspec_string())
    }
}

// This is automatically implemented for `RocksTomlValidated` by `Rockspec`,
// but we also add a special implementation for `RocksToml` (as providing a lua version)
// is required even by the non-validated struct.
impl LuaVersionCompatibility for RocksToml {
    fn validate_lua_version(&self, config: &Config) -> Result<(), LuaVersionError> {
        let _ = self.lua_version_matches(config)?;
        Ok(())
    }

    fn lua_version_matches(&self, config: &Config) -> Result<LuaVersion, LuaVersionError> {
        let version = LuaVersion::from(config)?;
        if self.supports_lua_version(&version) {
            Ok(version)
        } else {
            Err(LuaVersionError::LuaVersionUnsupported(
                version,
                self.package.clone(),
                self.version.clone(),
            ))
        }
    }

    fn supports_lua_version(&self, lua_version: &LuaVersion) -> bool {
        let binding = self.dependencies.as_ref().cloned().unwrap_or_default();
        let lua_version_reqs = binding
            .iter()
            .filter(|val| *val.name() == "lua".into())
            .collect_vec();
        let lua_pkg_version = lua_version.as_version();
        lua_version_reqs.is_empty()
            || lua_version_reqs
                .into_iter()
                .any(|lua| lua.version_req().matches(&lua_pkg_version))
    }

    fn lua_version(&self) -> Option<LuaVersion> {
        latest_lua_version(
            &self
                .dependencies
                .as_ref()
                .map(|deps| PerPlatform::new(deps.clone()))
                .unwrap_or_default(),
        )
    }

    fn test_lua_version(&self) -> Option<LuaVersion> {
        latest_lua_version(
            &self
                .test_dependencies
                .as_ref()
                .map(|deps| PerPlatform::new(deps.clone()))
                .unwrap_or_default(),
        )
        .or(self.lua_version())
    }
}

impl Rockspec for RocksTomlValidated {
    fn package(&self) -> &PackageName {
        &self.package
    }

    fn version(&self) -> &PackageVersion {
        &self.version
    }

    fn description(&self) -> &RockDescription {
        &self.description
    }

    fn supported_platforms(&self) -> &PlatformSupport {
        &self.supported_platforms
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
        self.to_rockspec_string()
    }

    fn format(&self) -> &Option<RockspecFormat> {
        &Some(RockspecFormat::_3_0)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        lua_rockspec::{LuaRockspec, PerPlatform},
        package::PackageReq,
        rockspec::Rockspec,
    };

    use super::RocksToml;

    #[test]
    fn rocks_toml_parsing() {
        let rocks_toml = r#"
        package = "my-package"
        version = "1.0.0"
        lua = "5.3"

        [source]
        url = "https://example.com"

        [dependencies]
        foo = "1.0"
        bar = ">=2.0"

        [build]
        type = "builtin"
        "#;

        let rocks = RocksToml::new(rocks_toml).unwrap();
        let _ = rocks.into_validated_rocks_toml().unwrap();

        let rocks_toml = r#"
        package = "my-package"
        version = "1.0.0"
        lua = "5.1"

        [description]
        summary = "A summary"
        detailed = "A detailed description"
        license = "MIT"
        homepage = "https://example.com"
        issues_url = "https://example.com/issues"
        maintainer = "John Doe"
        labels = ["label1", "label2"]

        [supported_platforms]
        linux = true
        windows = false

        [dependencies]
        foo = "1.0"
        bar = ">=2.0"

        [build_dependencies]
        baz = "1.0"

        [external_dependencies.foo]
        header = "foo.h"

        [external_dependencies.bar]
        library = "libbar.so"

        [test_dependencies]
        busted = "69.420"

        [source]
        url = "https://example.com"
        hash = "sha256-di00mD8txN7rjaVpvxzNbnQsAh6H16zUtJZapH7U4HU="
        file = "my-package-1.0.0.tar.gz"
        dir = "my-package-1.0.0"

        [test]
        type = "command"
        script = "test.lua"
        flags = [ "foo", "bar" ]

        [build]
        type = "builtin"
        "#;

        let rocks = RocksToml::new(rocks_toml).unwrap();
        let _ = rocks.into_validated_rocks_toml().unwrap();
    }

    #[test]
    fn compare_rocks_toml_with_rockspec() {
        let rocks_toml = r#"
        package = "my-package"
        version = "1.0.0"
        lua = "5.1"

        [description]
        summary = "A summary"
        detailed = "A detailed description"
        license = "MIT"
        homepage = "https://example.com"
        issues_url = "https://example.com/issues"
        maintainer = "John Doe"
        labels = ["label1", "label2"]

        [supported_platforms]
        linux = true
        windows = false

        [dependencies]
        foo = "1.0"
        bar = ">=2.0"

        [build_dependencies]
        baz = "1.0"

        [external_dependencies.foo]
        header = "foo.h"

        [external_dependencies.bar]
        library = "libbar.so"

        [test_dependencies]
        busted = "1.0"

        [source]
        url = "https://example.com"
        hash = "sha256-di00mD8txN7rjaVpvxzNbnQsAh6H16zUtJZapH7U4HU="
        file = "my-package-1.0.0.tar.gz"
        dir = "my-package-1.0.0"

        [test]
        type = "command"
        script = "test.lua"
        flags = [ "foo", "bar" ]

        [build]
        type = "builtin"
        "#;

        let expected_rockspec = r#"
            rockspec_format = "3.0"
            package = "my-package"
            version = "1.0.0"

            description = {
                summary = "A summary",
                detailed = "A detailed description",
                license = "MIT",
                homepage = "https://example.com",
                issues_url = "https://example.com/issues",
                maintainer = "John Doe",
                labels = {"label1", "label2"},
            }

            supported_platforms = {"linux", "!windows"}

            dependencies = {
                "lua 5.1",
                "foo 1.0",
                "bar >=2.0",
            }

            build_dependencies = {
                "baz 1.0",
            }

            external_dependencies = {
                foo = { header = "foo.h" },
                bar = { library = "libbar.so" },
            }

            test_dependencies = {
                "busted 1.0",
            }

            source = {
                url = "https://example.com",
                hash = "sha256-di00mD8txN7rjaVpvxzNbnQsAh6H16zUtJZapH7U4HU=",
                file = "my-package-1.0.0.tar.gz",
                dir = "my-package-1.0.0",
            }

            test = {
                type = "command",
                script = "test.lua",
                flags = {"foo", "bar"},
            }

            build = {
                type = "builtin",
            }
        "#;

        let expected_rockspec = LuaRockspec::new(expected_rockspec).unwrap();

        let rocks_toml = RocksToml::new(rocks_toml).unwrap();
        let rockspec = rocks_toml
            .into_validated_rocks_toml()
            .unwrap()
            .to_rockspec()
            .unwrap();

        let sorted_package_reqs = |v: &PerPlatform<Vec<PackageReq>>| {
            let mut v = v.current_platform().clone();
            v.sort_by(|a, b| a.name().cmp(b.name()));
            v
        };

        assert_eq!(rockspec.package(), expected_rockspec.package());
        assert_eq!(rockspec.version(), expected_rockspec.version());
        assert_eq!(rockspec.description(), expected_rockspec.description());
        assert_eq!(
            rockspec.supported_platforms(),
            expected_rockspec.supported_platforms()
        );
        assert_eq!(
            sorted_package_reqs(rockspec.dependencies()),
            sorted_package_reqs(expected_rockspec.dependencies())
        );
        assert_eq!(
            sorted_package_reqs(rockspec.build_dependencies()),
            sorted_package_reqs(expected_rockspec.build_dependencies())
        );
        assert_eq!(
            rockspec.external_dependencies(),
            expected_rockspec.external_dependencies()
        );
        assert_eq!(
            sorted_package_reqs(rockspec.test_dependencies()),
            sorted_package_reqs(expected_rockspec.test_dependencies())
        );
        assert_eq!(rockspec.source(), expected_rockspec.source());
        assert_eq!(rockspec.test(), expected_rockspec.test());
        assert_eq!(rockspec.build(), expected_rockspec.build());
    }
}
