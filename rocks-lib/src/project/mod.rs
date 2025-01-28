use lets_find_up::{find_up_with, FindUpKind, FindUpOptions};
use rocks_toml::{RocksToml, RocksTomlValidationError};
use std::{
    collections::HashMap,
    io,
    path::{Path, PathBuf},
    str::FromStr,
};
use thiserror::Error;

use crate::{
    config::LuaVersion,
    lockfile::{Lockfile, ReadOnly},
    lua_rockspec::{
        ExternalDependencySpec, LuaRockspec, PartialLuaRockspec, PartialRockspecError,
        RockSourceSpec, RockspecError,
    },
    package::PackageReq,
    remote_package_db::RemotePackageDB,
    rockspec::Rockspec,
    tree::Tree,
};

pub mod rocks_toml;

pub const ROCKS_TOML: &str = "rocks.toml";
pub const EXTRA_ROCKSPEC: &str = "extra.rockspec";

#[derive(Error, Debug)]
#[error(transparent)]
pub enum ProjectError {
    Io(#[from] io::Error),
    Rocks(#[from] RocksTomlValidationError),
    Toml(#[from] toml::de::Error),
    #[error("error when parsing `extra.rockspec`: {0}")]
    Rockspec(#[from] PartialRockspecError),
}

#[derive(Error, Debug)]
#[error(transparent)]
pub enum IntoRockspecError {
    RocksTomlValidationError(#[from] RocksTomlValidationError),
    RockspecError(#[from] RockspecError),
}

#[derive(Error, Debug)]
#[error(transparent)]
pub enum ProjectEditError {
    Io(#[from] tokio::io::Error),
    Toml(#[from] toml_edit::TomlError),
}

pub enum DependencyType {
    Regular(Vec<PackageReq>),
    Build(Vec<PackageReq>),
    Test(Vec<PackageReq>),
    External(HashMap<String, ExternalDependencySpec>),
}

#[derive(Clone, Debug)]
pub struct Project {
    /// The path where the `project.rockspec` resides.
    root: PathBuf,
    /// The parsed rockspec.
    rocks: RocksToml,
}

impl Project {
    pub fn current() -> Result<Option<Self>, ProjectError> {
        Self::from(&std::env::current_dir()?)
    }

    pub fn from(start: impl AsRef<Path>) -> Result<Option<Self>, ProjectError> {
        if !start.as_ref().exists() {
            return Ok(None);
        }

        match find_up_with(
            ROCKS_TOML,
            FindUpOptions {
                cwd: start.as_ref(),
                kind: FindUpKind::File,
            },
        )? {
            Some(path) => {
                let rocks_content = std::fs::read_to_string(&path)?;
                let root = path.parent().unwrap();

                let mut project = Project {
                    root: root.to_path_buf(),
                    rocks: RocksToml::new(&rocks_content)?,
                };

                if let Some(extra_rockspec) = project.extra_rockspec()? {
                    project.rocks = project.rocks.merge(extra_rockspec);
                }

                std::fs::create_dir_all(root)?;

                Ok(Some(project))
            }
            None => Ok(None),
        }
    }

    /// Get the `rocks.toml` path.
    pub fn rocks_path(&self) -> PathBuf {
        self.root.join(ROCKS_TOML)
    }

    /// Get the `extra.rockspec` path.
    pub fn extra_rockspec_path(&self) -> PathBuf {
        self.root.join(EXTRA_ROCKSPEC)
    }

    /// Get the `rocks.lock` lockfile path.
    pub fn lockfile_path(&self) -> PathBuf {
        self.root.join("rocks.lock")
    }

    /// Get the `rocks.lock` lockfile in the project root, if present.
    pub fn lockfile(&self) -> Result<Lockfile<ReadOnly>, io::Error> {
        Lockfile::new(self.lockfile_path())
    }

    /// Get the `rocks.lock` lockfile in the project root, if present.
    pub fn try_lockfile(&self) -> Result<Option<Lockfile<ReadOnly>>, io::Error> {
        let path = self.lockfile_path();
        if path.is_file() {
            Ok(Some(Lockfile::new(path)?))
        } else {
            Ok(None)
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn rocks(&self) -> &RocksToml {
        &self.rocks
    }

    pub fn rockspec(&self) -> Result<LuaRockspec, IntoRockspecError> {
        Ok(self.rocks().into_validated_rocks_toml()?.to_rockspec()?)
    }

    pub fn extra_rockspec(&self) -> Result<Option<PartialLuaRockspec>, PartialRockspecError> {
        if self.extra_rockspec_path().exists() {
            Ok(Some(PartialLuaRockspec::new(&std::fs::read_to_string(
                self.extra_rockspec_path(),
            )?)?))
        } else {
            Ok(None)
        }
    }

    /// Create a RockSpec with the source set to the project root.
    pub fn new_local_rockspec(&self) -> Result<LuaRockspec, IntoRockspecError> {
        let mut rocks = self.rockspec()?;
        let mut source = rocks.source().current_platform().clone();
        source.source_spec = RockSourceSpec::File(self.root().to_path_buf());
        source.archive_name = None;
        source.integrity = None;
        rocks.source_mut().current_platform_set(source);
        Ok(rocks)
    }

    pub fn tree(&self, lua_version: LuaVersion) -> io::Result<Tree> {
        Tree::new(self.root.join(".rocks"), lua_version)
    }

    pub async fn add(
        &mut self,
        dependencies: DependencyType,
        package_db: &RemotePackageDB,
    ) -> Result<(), ProjectEditError> {
        let mut rocks =
            toml_edit::DocumentMut::from_str(&tokio::fs::read_to_string(self.rocks_path()).await?)?;

        if !rocks.contains_table("dependencies") {
            let mut table = toml_edit::table().into_table().unwrap();
            table.set_implicit(true);

            rocks["dependencies"] = toml_edit::Item::Table(table);
        }
        if !rocks.contains_table("build_dependencies") {
            let mut table = toml_edit::table().into_table().unwrap();
            table.set_implicit(true);

            rocks["build_dependencies"] = toml_edit::Item::Table(table);
        }
        if !rocks.contains_table("test_dependencies") {
            let mut table = toml_edit::table().into_table().unwrap();
            table.set_implicit(true);

            rocks["test_dependencies"] = toml_edit::Item::Table(table);
        }
        if !rocks.contains_table("external_dependencies") {
            let mut table = toml_edit::table().into_table().unwrap();
            table.set_implicit(true);

            rocks["external_dependencies"] = toml_edit::Item::Table(table);
        }

        let table = match dependencies {
            DependencyType::Regular(_) => &mut rocks["dependencies"],
            DependencyType::Build(_) => &mut rocks["build_dependencies"],
            DependencyType::Test(_) => &mut rocks["test_dependencies"],
            DependencyType::External(_) => &mut rocks["external_dependencies"],
        };

        match dependencies {
            DependencyType::Regular(ref deps)
            | DependencyType::Build(ref deps)
            | DependencyType::Test(ref deps) => {
                for dep in deps {
                    table[dep.name().to_string()] = toml_edit::value(
                        dep.version_req().map(|v| v.to_string()).unwrap_or(
                            package_db
                                .latest_version(dep.name())
                                // This condition should never be reached, as the package should
                                // have been found in the database or an error should have been
                                // reported prior.
                                // Still worth making an error message for this in the future,
                                // though.
                                .expect("unable to query latest version for package")
                                .to_string(),
                        ),
                    );
                }
            }
            DependencyType::External(ref deps) => {
                for (name, dep) in deps {
                    match dep {
                        ExternalDependencySpec::Header(path) => {
                            table[name]["header"] =
                                toml_edit::value(path.to_string_lossy().to_string());
                        }
                        ExternalDependencySpec::Library(path) => {
                            table[name]["library"] =
                                toml_edit::value(path.to_string_lossy().to_string());
                        }
                    }
                }
            }
        };

        tokio::fs::write(self.rocks_path(), rocks.to_string()).await?;

        match dependencies {
            DependencyType::Regular(deps) => {
                self.rocks.dependencies = Some(
                    self.rocks
                        .dependencies
                        .take()
                        .unwrap_or_default()
                        .into_iter()
                        .chain(deps)
                        .collect(),
                )
            }
            DependencyType::Build(deps) => {
                self.rocks.build_dependencies = Some(
                    self.rocks
                        .build_dependencies
                        .take()
                        .unwrap_or_default()
                        .into_iter()
                        .chain(deps)
                        .collect(),
                )
            }
            DependencyType::Test(deps) => {
                self.rocks.test_dependencies = Some(
                    self.rocks
                        .test_dependencies
                        .take()
                        .unwrap_or_default()
                        .into_iter()
                        .chain(deps)
                        .collect(),
                )
            }
            DependencyType::External(deps) => {
                self.rocks.external_dependencies = Some(
                    self.rocks
                        .external_dependencies
                        .take()
                        .unwrap_or_default()
                        .into_iter()
                        .chain(deps)
                        .collect(),
                )
            }
        };

        Ok(())
    }
}

// TODO: More project-based test
#[cfg(test)]
mod tests {
    use assert_fs::prelude::PathCopy;
    use url::Url;

    use super::*;
    use crate::{
        manifest::{Manifest, ManifestMetadata},
        package::PackageReq,
    };

    #[tokio::test]
    async fn add_various_dependencies() {
        let sample_project: PathBuf = "resources/test/sample-project-busted/".into();
        let project_root = assert_fs::TempDir::new().unwrap();
        project_root.copy_from(&sample_project, &["**"]).unwrap();
        let project_root: PathBuf = project_root.path().into();
        let mut project = Project::from(&project_root).unwrap().unwrap();
        let expected_dependencies =
            vec![PackageReq::new("busted".into(), Some(">= 1.0.0".into())).unwrap()];

        let test_manifest_path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/test/manifest-5.1");
        let content = String::from_utf8(std::fs::read(&test_manifest_path).unwrap()).unwrap();
        let metadata = ManifestMetadata::new(&content).unwrap();
        let package_db = Manifest::new(Url::parse("https://example.com").unwrap(), metadata).into();

        project
            .add(
                DependencyType::Regular(expected_dependencies.clone()),
                &package_db,
            )
            .await
            .unwrap();

        project
            .add(
                DependencyType::Build(expected_dependencies.clone()),
                &package_db,
            )
            .await
            .unwrap();
        project
            .add(
                DependencyType::Test(expected_dependencies.clone()),
                &package_db,
            )
            .await
            .unwrap();

        project
            .add(
                DependencyType::External(HashMap::from([(
                    "lib".into(),
                    ExternalDependencySpec::Library("path.so".into()),
                )])),
                &package_db,
            )
            .await
            .unwrap();

        let strip_lua = |deps: &Vec<PackageReq>| -> Vec<PackageReq> {
            deps.iter()
                .filter(|dep| dep.name() != &"lua".into())
                .cloned()
                .collect()
        };

        // Reparse the rocks.toml (not usually necessary, but we want to test that the file was
        // written correctly)
        let project = Project::from(&project_root).unwrap().unwrap();
        let validated_rocks_toml = project.rocks().into_validated_rocks_toml().unwrap();
        assert_eq!(
            strip_lua(validated_rocks_toml.dependencies().current_platform()),
            expected_dependencies
        );
        assert_eq!(
            strip_lua(validated_rocks_toml.build_dependencies().current_platform()),
            expected_dependencies
        );
        assert_eq!(
            strip_lua(validated_rocks_toml.test_dependencies().current_platform()),
            expected_dependencies
        );
        assert_eq!(
            validated_rocks_toml
                .external_dependencies()
                .current_platform()
                .get("lib")
                .unwrap(),
            &ExternalDependencySpec::Library("path.so".into())
        );
    }

    #[tokio::test]
    async fn extra_rockspec_parsing() {
        let sample_project: PathBuf = "resources/test/sample-project-extra-rockspec".into();
        let project_root = assert_fs::TempDir::new().unwrap();
        project_root.copy_from(&sample_project, &["**"]).unwrap();
        let project_root: PathBuf = project_root.path().into();
        let project = Project::from(project_root).unwrap().unwrap();

        let extra_rockspec = project.extra_rockspec().unwrap();

        assert!(extra_rockspec.is_some());

        let rocks = project.rocks().into_validated_rocks_toml().unwrap();

        assert_eq!(rocks.package().to_string(), "custom-package");
        assert_eq!(rocks.version().to_string(), "2.0.0-1");
        assert!(
            matches!(&rocks.source().current_platform().source_spec, RockSourceSpec::Url(url) if url == &Url::parse("https://github.com/custom/url").unwrap())
        );
    }
}
