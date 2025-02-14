//! Structs and utilities for `lux.toml`

use std::{collections::HashMap, path::PathBuf};

use itertools::Itertools;
use serde::{Deserialize, Deserializer};
use thiserror::Error;

use crate::{
    config::{Config, LuaVersion},
    lua_rockspec::{
        BuildSpec, BuildSpecInternal, BuildSpecInternalError, DisplayAsLuaKV, ExternalDependencies,
        ExternalDependencySpec, FromPlatformOverridable, LuaRockspec, LuaVersionError,
        PartialLuaRockspec, PerPlatform, PlatformIdentifier, PlatformSupport,
        PlatformValidationError, RockDescription, RockSource, RockSourceError, RockSourceInternal,
        RockspecError, RockspecFormat, TestSpec, TestSpecError, TestSpecInternal,
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
pub enum ProjectTomlValidationError {
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
    #[error("{}copy_directories cannot contain a rockspec name", ._0.as_ref().map(|p| format!("{p}: ")).unwrap_or_default())]
    CopyDirectoriesContainRockspecName(Option<String>),
}

/// The `lux.toml` file.
/// The only required fields are `package` and `build`, which are required to build a project using `lux build`.
/// The rest of the fields are optional, but are required to build a rockspec.
#[derive(Clone, Debug, Deserialize)]
pub struct ProjectToml {
    pub(crate) package: PackageName,
    pub(crate) version: PackageVersion,
    pub(crate) build: BuildSpecInternal,
    pub(crate) rockspec_format: Option<RockspecFormat>,
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

/// Equivalent to [`ProjectToml`], but with all fields required
#[derive(Debug)]
pub struct ProjectTomlValidated {
    package: PackageName,
    version: PackageVersion,
    lua: PackageVersionReq,
    rockspec_format: Option<RockspecFormat>,
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
    internal: ProjectToml,
}

impl ProjectToml {
    pub fn new(str: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(str)
    }

    pub fn into_validated(&self) -> Result<ProjectTomlValidated, ProjectTomlValidationError> {
        let project = self.clone();

        let validated = ProjectTomlValidated {
            internal: project.clone(),

            package: project.package,
            version: project.version,
            lua: project
                .lua
                .clone()
                .ok_or(ProjectTomlValidationError::NoLuaVersion)?,
            description: project.description.unwrap_or_default(),
            supported_platforms: PlatformSupport::parse(
                &project
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
            // Merge dependencies internally with lua version
            // so the output of `dependencies()` is consistent
            dependencies: PerPlatform::new(
                project
                    .dependencies
                    .unwrap_or_default()
                    .into_iter()
                    .chain(std::iter::once(PackageReq {
                        name: "lua".into(),
                        version_req: project.lua.unwrap_or(PackageVersionReq::any()),
                    }))
                    .collect(),
            ),
            build_dependencies: PerPlatform::new(project.build_dependencies.unwrap_or_default()),
            external_dependencies: PerPlatform::new(
                project.external_dependencies.unwrap_or_default(),
            ),
            test_dependencies: PerPlatform::new(project.test_dependencies.unwrap_or_default()),
            source: PerPlatform::new(RockSource::from_platform_overridable(
                project.source.ok_or(ProjectTomlValidationError::NoSource)?,
            )?),
            test: PerPlatform::new(TestSpec::from_platform_overridable(
                project.test.clone().unwrap_or_default(),
            )?),
            build: PerPlatform::new(BuildSpec::from_internal_spec(project.build.clone())?),
            rockspec_format: self.rockspec_format.clone(),
        };

        let rockspec_file_name = format!("{}-{}.rockspec", validated.package, validated.version);

        if validated
            .build
            .default
            .copy_directories
            .contains(&PathBuf::from(&rockspec_file_name))
        {
            return Err(ProjectTomlValidationError::CopyDirectoriesContainRockspecName(None));
        }

        for (platform, build_override) in &validated.build.per_platform {
            if build_override
                .copy_directories
                .contains(&PathBuf::from(&rockspec_file_name))
            {
                return Err(
                    ProjectTomlValidationError::CopyDirectoriesContainRockspecName(Some(
                        platform.to_string(),
                    )),
                );
            }
        }

        Ok(validated)
    }

    // In the not-yet-validated struct, we create getters only
    // for the non-optional fields.
    pub fn package(&self) -> &PackageName {
        &self.package
    }

    pub fn version(&self) -> &PackageVersion {
        &self.version
    }

    /// Merge the `ProjectToml` struct with an unvalidated `LuaRockspec`.
    /// The final merged struct can then be validated.
    pub fn merge(self, other: PartialLuaRockspec) -> Self {
        ProjectToml {
            package: other.package.unwrap_or(self.package),
            version: other.version.unwrap_or(self.version),
            lua: other
                .dependencies
                .as_ref()
                .and_then(|deps| {
                    deps.iter()
                        .find(|dep| dep.name == "lua".into())
                        .and_then(|dep| {
                            if dep.version_req.is_any() {
                                None
                            } else {
                                Some(dep.version_req.clone())
                            }
                        })
                })
                .or(self.lua),
            build: other.build.unwrap_or(self.build),
            description: other.description.or(self.description),
            supported_platforms: other
                .supported_platforms
                .map(|platform_support| platform_support.platforms().clone())
                .or(self.supported_platforms),
            dependencies: other
                .dependencies
                .map(|deps| {
                    deps.into_iter()
                        .filter(|dep| dep.name != "lua".into())
                        .collect()
                })
                .or(self.dependencies),
            build_dependencies: other.build_dependencies.or(self.build_dependencies),
            test_dependencies: other.test_dependencies.or(self.test_dependencies),
            external_dependencies: other.external_dependencies.or(self.external_dependencies),
            source: other.source.or(self.source),
            test: other.test.or(self.test),
            rockspec_format: other.rockspec_format.or(self.rockspec_format),
        }
    }
}

impl ProjectTomlValidated {
    pub fn to_rockspec_string(&self) -> String {
        let starter = format!(
            r#"
rockspec_format = "{}"
package = "{}"
version = "{}""#,
            self.rockspec_format.as_ref().unwrap_or(&"3.0".into()),
            self.package,
            self.version
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
// but we also add a special implementation for `ProjectToml` (as providing a lua version)
// is required even by the non-validated struct.
impl LuaVersionCompatibility for ProjectToml {
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

impl Rockspec for ProjectTomlValidated {
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
        &self.rockspec_format
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        lua_rockspec::{LuaRockspec, PartialLuaRockspec, PerPlatform},
        package::PackageReq,
        rockspec::Rockspec,
    };

    use super::ProjectToml;

    #[test]
    fn project_toml_parsing() {
        let project_toml = r#"
        package = "my-package"
        version = "1.0.0"
        lua = "5.3"

        rockspec_format = "1.0"

        [source]
        url = "https://example.com"

        [dependencies]
        foo = "1.0"
        bar = ">=2.0"

        [build]
        type = "builtin"
        "#;

        let project = ProjectToml::new(project_toml).unwrap();
        let _ = project.into_validated().unwrap();

        let project_toml = r#"
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

        let project = ProjectToml::new(project_toml).unwrap();
        let _ = project.into_validated().unwrap();
    }

    #[test]
    fn compare_project_toml_with_rockspec() {
        let project_toml = r#"
        package = "my-package"
        version = "1.0.0"
        lua = "5.1"

        # For testing, specify a custom rockspec format
        # (defaults to 3.0)
        rockspec_format = "1.0"

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
            rockspec_format = "1.0"
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

        let project_toml = ProjectToml::new(project_toml).unwrap();
        let rockspec = project_toml
            .into_validated()
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
        assert_eq!(rockspec.format(), expected_rockspec.format());
    }

    #[test]
    fn merge_project_toml_with_partial_rockspec() {
        let project_toml = r#"
        package = "my-package"
        version = "1.0.0"
        lua = "5.1"

        # For testing, specify a custom rockspec format
        # (defaults to 3.0)
        rockspec_format = "1.0"

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

        let mergable_rockspec_content = r#"
            rockspec_format = "1.0"
            package = "my-package-overwritten"
            version = "2.0.0"

            description = {
                summary = "A summary overwritten",
                detailed = "A detailed description overwritten",
                license = "GPL-2.0",
                homepage = "https://example.com/overwritten",
                issues_url = "https://example.com/issues/overwritten",
                maintainer = "John Doe Overwritten",
                labels = {"over", "written"},
            }

            -- Inverted supported platforms
            supported_platforms = {"!linux", "windows"}

            dependencies = {
                "lua 5.1",
                "foo >1.0",
                "bar <=2.0",
            }

            build_dependencies = {
                "baz >1.0",
            }

            external_dependencies = {
                foo = { header = "overwritten.h" },
                bar = { library = "overwritten.so" },
            }

            test_dependencies = {
                "busted >1.0",
            }

            source = {
                url = "https://example.com/overwritten",
                hash = "sha256-QL5OCZFBGixecdEoriGck4iG83tjM09ewYbWVSbcfa4=",
                file = "my-package-1.0.0.tar.gz.overwritten",
                dir = "my-package-1.0.0.overwritten",
            }

            test = {
                type = "command",
                script = "overwritten.lua",
                flags = {"over", "written"},
            }

            build = {
                type = "builtin",
            }
        "#;

        let project_toml = ProjectToml::new(project_toml).unwrap();
        let partial_rockspec = PartialLuaRockspec::new(mergable_rockspec_content).unwrap();
        let expected_rockspec = LuaRockspec::new(mergable_rockspec_content).unwrap();

        let merged = project_toml
            .merge(partial_rockspec)
            .into_validated()
            .unwrap();

        let sorted_package_reqs = |v: &PerPlatform<Vec<PackageReq>>| {
            let mut v = v.current_platform().clone();
            v.sort_by(|a, b| a.name().cmp(b.name()));
            v
        };

        assert_eq!(merged.package(), expected_rockspec.package());
        assert_eq!(merged.version(), expected_rockspec.version());
        assert_eq!(merged.description(), expected_rockspec.description());
        assert_eq!(
            merged.supported_platforms(),
            expected_rockspec.supported_platforms()
        );
        assert_eq!(
            sorted_package_reqs(merged.dependencies()),
            sorted_package_reqs(expected_rockspec.dependencies())
        );
        assert_eq!(
            sorted_package_reqs(merged.build_dependencies()),
            sorted_package_reqs(expected_rockspec.build_dependencies())
        );
        assert_eq!(
            merged.external_dependencies(),
            expected_rockspec.external_dependencies()
        );
        assert_eq!(
            sorted_package_reqs(merged.test_dependencies()),
            sorted_package_reqs(expected_rockspec.test_dependencies())
        );
        assert_eq!(merged.source(), expected_rockspec.source());
        assert_eq!(merged.test(), expected_rockspec.test());
        assert_eq!(merged.build(), expected_rockspec.build());
        assert_eq!(merged.format(), expected_rockspec.format());
    }
}
