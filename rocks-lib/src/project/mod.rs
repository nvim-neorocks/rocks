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
    lua_rockspec::{ExternalDependencySpec, LuaRockspec, RockSourceSpec, RockspecError},
    package::PackageReq,
    rockspec::Rockspec,
    tree::Tree,
};

pub mod rocks_toml;

pub const ROCKS_TOML: &str = "rocks.toml";

#[derive(Error, Debug)]
#[error(transparent)]
pub enum ProjectError {
    Io(#[from] io::Error),
    Rocks(#[from] RocksTomlValidationError),
    Toml(#[from] toml::de::Error),
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
    //RocksTomlValidationError(#[from] RocksTomlValidationError),
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
                let rocks = RocksToml::new(&rocks_content)?;

                let root = path.parent().unwrap();

                std::fs::create_dir_all(root)?;

                Ok(Some(Project {
                    root: root.to_path_buf(),
                    rocks,
                }))
            }
            None => Ok(None),
        }
    }

    /// Get the `rocks.toml` path.
    pub fn rocks_path(&self) -> PathBuf {
        self.root.join(ROCKS_TOML)
    }

    /// Get the `rocks.lock` lockfile path.
    pub fn lockfile_path(&self) -> PathBuf {
        self.root.join("rocks.lock")
    }

    /// Get the `rocks.lock` lockfile in the project root, if present.
    pub fn lockfile(&self) -> Result<Option<Lockfile<ReadOnly>>, ProjectError> {
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

    pub async fn add(&mut self, dependencies: DependencyType) -> Result<(), ProjectEditError> {
        let mut rocks =
            toml_edit::DocumentMut::from_str(&tokio::fs::read_to_string(self.rocks_path()).await?)?;

        if !rocks.contains_table("dependencies") {
            rocks["dependencies"] = toml_edit::table();
        }
        if !rocks.contains_table("build_dependencies") {
            rocks["build_dependencies"] = toml_edit::table();
        }
        if !rocks.contains_table("test_dependencies") {
            rocks["test_dependencies"] = toml_edit::table();
        }
        if !rocks.contains_table("external_dependencies") {
            rocks["external_dependencies"] = toml_edit::table();
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
                    table[dep.name().to_string()] = toml_edit::value(dep.version_req().to_string());
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

// TODO: Add plenty of tests
#[cfg(test)]
mod tests {
    use assert_fs::prelude::PathCopy;

    use super::*;
    use crate::package::PackageReq;

    #[tokio::test]
    async fn add_various_dependencies() {
        let sample_project: PathBuf = "resources/test/sample-project-busted/".into();
        let project_root = assert_fs::TempDir::new().unwrap();
        project_root.copy_from(&sample_project, &["**"]).unwrap();
        let project_root: PathBuf = project_root.path().into();
        let mut project = Project::from(&project_root).unwrap().unwrap();
        let expected_dependencies =
            vec![PackageReq::new("busted".into(), Some(">= 1.0.0".into())).unwrap()];

        project
            .add(DependencyType::Regular(expected_dependencies.clone()))
            .await
            .unwrap();

        project
            .add(DependencyType::Build(expected_dependencies.clone()))
            .await
            .unwrap();
        project
            .add(DependencyType::Test(expected_dependencies.clone()))
            .await
            .unwrap();

        project
            .add(DependencyType::External(HashMap::from([(
                "lib".into(),
                ExternalDependencySpec::Library("path.so".into()),
            )])))
            .await
            .unwrap();

        // Reparse the rocks.toml (not usually necessary, but we want to test that the file was
        // written correctly)
        let project = Project::from(&project_root).unwrap().unwrap();
        let validated_rocks_toml = project.rocks().into_validated_rocks_toml().unwrap();
        assert_eq!(
            validated_rocks_toml.dependencies().current_platform(),
            &expected_dependencies
        );
        assert_eq!(
            validated_rocks_toml.build_dependencies().current_platform(),
            &expected_dependencies
        );
        assert_eq!(
            validated_rocks_toml.test_dependencies().current_platform(),
            &expected_dependencies
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
}
