use itertools::Itertools;
use lets_find_up::{find_up_with, FindUpKind, FindUpOptions};
use mlua::{ExternalResult, UserData};
use project_toml::{
    LocalProjectTomlValidationError, PartialProjectToml, RemoteProjectTomlValidationError,
};
use std::{
    collections::HashMap,
    io,
    ops::Deref,
    path::{Path, PathBuf},
    str::FromStr,
};
use thiserror::Error;
use toml_edit::{DocumentMut, Item};

use crate::{
    config::{Config, LuaVersion},
    lockfile::{ProjectLockfile, ReadOnly},
    lua_rockspec::{
        ExternalDependencySpec, LocalLuaRockspec, LuaRockspecError, LuaVersionError,
        PartialLuaRockspec, PartialRockspecError, RemoteLuaRockspec,
    },
    package::{PackageName, PackageReq},
    remote_package_db::RemotePackageDB,
    rockspec::LuaVersionCompatibility,
    tree::Tree,
};

pub mod project_toml;

pub const PROJECT_TOML: &str = "lux.toml";
pub const EXTRA_ROCKSPEC: &str = "extra.rockspec";

#[derive(Error, Debug)]
#[error(transparent)]
pub enum ProjectError {
    Io(#[from] io::Error),
    Project(#[from] LocalProjectTomlValidationError),
    Toml(#[from] toml::de::Error),
    #[error("error when parsing `extra.rockspec`: {0}")]
    Rockspec(#[from] PartialRockspecError),
}

#[derive(Error, Debug)]
#[error(transparent)]
pub enum IntoLocalRockspecError {
    LocalProjectTomlValidationError(#[from] LocalProjectTomlValidationError),
    RockspecError(#[from] LuaRockspecError),
}

#[derive(Error, Debug)]
#[error(transparent)]
pub enum IntoRemoteRockspecError {
    RocksTomlValidationError(#[from] RemoteProjectTomlValidationError),
    RockspecError(#[from] LuaRockspecError),
}

#[derive(Error, Debug)]
#[error(transparent)]
pub enum ProjectEditError {
    Io(#[from] tokio::io::Error),
    Toml(#[from] toml_edit::TomlError),
}

pub enum DependencyType<T> {
    Regular(Vec<T>),
    Build(Vec<T>),
    Test(Vec<T>),
    External(HashMap<String, ExternalDependencySpec>),
}

pub enum LuaDependencyType<T> {
    Regular(Vec<T>),
    Build(Vec<T>),
    Test(Vec<T>),
}

#[derive(Error, Debug)]
#[error(transparent)]
pub enum ProjectTreeError {
    Io(#[from] io::Error),
    LuaVersionError(#[from] LuaVersionError),
}

/// A newtype for the project root directory.
/// This is used to ensure that the project root is a valid project directory.
#[derive(Clone, Debug)]
#[cfg_attr(test, derive(Default))]
pub struct ProjectRoot(PathBuf);

impl ProjectRoot {
    pub(crate) fn new() -> Self {
        Self(PathBuf::new())
    }
}

impl Deref for ProjectRoot {
    type Target = PathBuf;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Clone, Debug)]
pub struct Project {
    /// The path where the `lux.toml` resides.
    root: ProjectRoot,
    /// The parsed lux.toml.
    toml: PartialProjectToml,
}

impl UserData for Project {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("toml_path", |_, this, ()| Ok(this.toml_path()));
        methods.add_method("extra_rockspec_path", |_, this, ()| {
            Ok(this.extra_rockspec_path())
        });
        methods.add_method("lockfile_path", |_, this, ()| Ok(this.lockfile_path()));
        methods.add_method("root", |_, this, ()| Ok(this.root().0.clone()));
        methods.add_method("toml", |_, this, ()| Ok(this.toml().clone()));
        methods.add_method("local_rockspec", |_, this, ()| {
            this.local_rockspec().into_lua_err()
        });
        methods.add_method("remote_rockspec", |_, this, ()| {
            this.remote_rockspec().into_lua_err()
        });
        methods.add_method("tree", |_, this, config: Config| {
            this.tree(&config).into_lua_err()
        });
        methods.add_method("test_tree", |_, this, config: Config| {
            this.test_tree(&config).into_lua_err()
        });
        methods.add_method("lua_version", |_, this, config: Config| {
            this.lua_version(&config).into_lua_err()
        });

        //methods.add_method("lockfile", |_, this, ()| this.lockfile().into_lua_err());
        //methods.add_method("extra_rockspec", |_, this, ()| this.extra_rockspec().into_lua_err());
        //methods.add_method("add")
    }
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
            PROJECT_TOML,
            FindUpOptions {
                cwd: start.as_ref(),
                kind: FindUpKind::File,
            },
        )? {
            Some(path) => {
                let toml_content = std::fs::read_to_string(&path)?;
                let root = path.parent().unwrap();

                let mut project = Project {
                    root: ProjectRoot(root.to_path_buf()),
                    toml: PartialProjectToml::new(&toml_content, ProjectRoot(root.to_path_buf()))?,
                };

                if let Some(extra_rockspec) = project.extra_rockspec()? {
                    project.toml = project.toml.merge(extra_rockspec);
                }

                std::fs::create_dir_all(root)?;

                Ok(Some(project))
            }
            None => Ok(None),
        }
    }

    /// Get the `lux.toml` path.
    pub fn toml_path(&self) -> PathBuf {
        self.root.join(PROJECT_TOML)
    }

    /// Get the `extra.rockspec` path.
    pub fn extra_rockspec_path(&self) -> PathBuf {
        self.root.join(EXTRA_ROCKSPEC)
    }

    /// Get the `lux.lock` lockfile path.
    pub fn lockfile_path(&self) -> PathBuf {
        self.root.join("lux.lock")
    }

    /// Get the `lux.lock` lockfile in the project root.
    pub fn lockfile(&self) -> Result<ProjectLockfile<ReadOnly>, io::Error> {
        ProjectLockfile::new(self.lockfile_path())
    }

    /// Get the `lux.lock` lockfile in the project root, if present.
    pub fn try_lockfile(&self) -> Result<Option<ProjectLockfile<ReadOnly>>, io::Error> {
        let path = self.lockfile_path();
        if path.is_file() {
            Ok(Some(ProjectLockfile::new(path)?))
        } else {
            Ok(None)
        }
    }

    pub fn root(&self) -> &ProjectRoot {
        &self.root
    }

    pub fn toml(&self) -> &PartialProjectToml {
        &self.toml
    }

    pub fn local_rockspec(&self) -> Result<LocalLuaRockspec, IntoLocalRockspecError> {
        Ok(self.toml().into_local()?.to_lua_rockspec()?)
    }

    pub fn remote_rockspec(&self) -> Result<RemoteLuaRockspec, IntoRemoteRockspecError> {
        Ok(self.toml().into_remote()?.to_lua_rockspec()?)
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

    pub(crate) fn default_tree_root_dir(&self) -> PathBuf {
        self.root.join(".lux")
    }

    pub fn tree(&self, config: &Config) -> Result<Tree, ProjectTreeError> {
        Ok(config.tree(self.lua_version(config)?)?)
    }

    pub fn test_tree(&self, config: &Config) -> Result<Tree, ProjectTreeError> {
        let tree = self.tree(config)?;
        let test_tree_root = tree.root().join("test_dependencies");
        Ok(Tree::new(test_tree_root, self.lua_version(config)?)?)
    }

    pub fn lua_version(&self, config: &Config) -> Result<LuaVersion, LuaVersionError> {
        self.toml().lua_version_matches(config)
    }

    pub async fn add(
        &mut self,
        dependencies: DependencyType<PackageReq>,
        package_db: &RemotePackageDB,
    ) -> Result<(), ProjectEditError> {
        let mut project_toml =
            toml_edit::DocumentMut::from_str(&tokio::fs::read_to_string(self.toml_path()).await?)?;

        prepare_dependency_tables(&mut project_toml);
        let table = match dependencies {
            DependencyType::Regular(_) => &mut project_toml["dependencies"],
            DependencyType::Build(_) => &mut project_toml["build_dependencies"],
            DependencyType::Test(_) => &mut project_toml["test_dependencies"],
            DependencyType::External(_) => &mut project_toml["external_dependencies"],
        };

        match dependencies {
            DependencyType::Regular(ref deps)
            | DependencyType::Build(ref deps)
            | DependencyType::Test(ref deps) => {
                for dep in deps {
                    let dep_version_str = if dep.version_req().is_any() {
                        package_db
                            .latest_version(dep.name())
                            // This condition should never be reached, as the package should
                            // have been found in the database or an error should have been
                            // reported prior.
                            // Still worth making an error message for this in the future,
                            // though.
                            .expect("unable to query latest version for package")
                            .to_string()
                    } else {
                        dep.version_req().to_string()
                    };
                    table[dep.name().to_string()] = toml_edit::value(dep_version_str);
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

        tokio::fs::write(self.toml_path(), project_toml.to_string()).await?;

        match dependencies {
            DependencyType::Regular(deps) => {
                self.toml.dependencies = Some(
                    self.toml
                        .dependencies
                        .take()
                        .unwrap_or_default()
                        .into_iter()
                        .chain(deps)
                        .collect(),
                )
            }
            DependencyType::Build(deps) => {
                self.toml.build_dependencies = Some(
                    self.toml
                        .build_dependencies
                        .take()
                        .unwrap_or_default()
                        .into_iter()
                        .chain(deps)
                        .collect(),
                )
            }
            DependencyType::Test(deps) => {
                self.toml.test_dependencies = Some(
                    self.toml
                        .test_dependencies
                        .take()
                        .unwrap_or_default()
                        .into_iter()
                        .chain(deps)
                        .collect(),
                )
            }
            DependencyType::External(deps) => {
                self.toml.external_dependencies = Some(
                    self.toml
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

    pub async fn remove(
        &mut self,
        dependencies: DependencyType<PackageName>,
    ) -> Result<(), ProjectEditError> {
        let mut project_toml =
            toml_edit::DocumentMut::from_str(&tokio::fs::read_to_string(self.toml_path()).await?)?;

        prepare_dependency_tables(&mut project_toml);
        let table = match dependencies {
            DependencyType::Regular(_) => &mut project_toml["dependencies"],
            DependencyType::Build(_) => &mut project_toml["build_dependencies"],
            DependencyType::Test(_) => &mut project_toml["test_dependencies"],
            DependencyType::External(_) => &mut project_toml["external_dependencies"],
        };

        match dependencies {
            DependencyType::Regular(ref deps)
            | DependencyType::Build(ref deps)
            | DependencyType::Test(ref deps) => {
                for dep in deps {
                    table[dep.to_string()] = Item::None;
                }
            }
            DependencyType::External(ref deps) => {
                for (name, dep) in deps {
                    match dep {
                        ExternalDependencySpec::Header(_) => {
                            table[name]["header"] = Item::None;
                        }
                        ExternalDependencySpec::Library(_) => {
                            table[name]["library"] = Item::None;
                        }
                    }
                }
            }
        };

        tokio::fs::write(self.toml_path(), project_toml.to_string()).await?;

        match dependencies {
            DependencyType::Regular(deps) => {
                self.toml.dependencies = Some(
                    self.toml
                        .dependencies
                        .take()
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|dep| !&deps.iter().any(|d| d == dep.name()))
                        .collect(),
                )
            }
            DependencyType::Build(deps) => {
                self.toml.build_dependencies = Some(
                    self.toml
                        .build_dependencies
                        .take()
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|dep| !&deps.iter().any(|d| d == dep.name()))
                        .collect(),
                )
            }
            DependencyType::Test(deps) => {
                self.toml.test_dependencies = Some(
                    self.toml
                        .test_dependencies
                        .take()
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|dep| !&deps.iter().any(|d| d == dep.name()))
                        .collect(),
                )
            }
            DependencyType::External(deps) => {
                self.toml.external_dependencies = Some(
                    self.toml
                        .external_dependencies
                        .take()
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|(dep, _)| !&deps.iter().any(|(d, _)| d == dep))
                        .collect(),
                )
            }
        };

        Ok(())
    }

    pub async fn upgrade(
        &mut self,
        dependencies: LuaDependencyType<PackageName>,
        package_db: &RemotePackageDB,
    ) -> Result<(), ProjectEditError> {
        let mut project_toml =
            toml_edit::DocumentMut::from_str(&tokio::fs::read_to_string(self.toml_path()).await?)?;

        prepare_dependency_tables(&mut project_toml);
        let table = match dependencies {
            LuaDependencyType::Regular(_) => &mut project_toml["dependencies"],
            LuaDependencyType::Build(_) => &mut project_toml["build_dependencies"],
            LuaDependencyType::Test(_) => &mut project_toml["test_dependencies"],
        };

        match dependencies {
            LuaDependencyType::Regular(ref deps)
            | LuaDependencyType::Build(ref deps)
            | LuaDependencyType::Test(ref deps) => {
                for dep in deps {
                    let dep_version_str = package_db
                        .latest_version(dep)
                        .expect("unable to query latest version for package")
                        .to_string();
                    table[dep.to_string()] = toml_edit::value(dep_version_str);
                }
            }
        }

        Ok(())
    }

    pub async fn upgrade_all(
        &mut self,
        package_db: &RemotePackageDB,
    ) -> Result<(), ProjectEditError> {
        if let Some(dependencies) = &self.toml().dependencies {
            let packages = dependencies
                .iter()
                .map(|dep| dep.name())
                .cloned()
                .collect_vec();
            self.upgrade(LuaDependencyType::Regular(packages), package_db)
                .await?;
        }
        if let Some(dependencies) = &self.toml().build_dependencies {
            let packages = dependencies
                .iter()
                .map(|dep| dep.name())
                .cloned()
                .collect_vec();
            self.upgrade(LuaDependencyType::Build(packages), package_db)
                .await?;
        }
        if let Some(dependencies) = &self.toml().test_dependencies {
            let packages = dependencies
                .iter()
                .map(|dep| dep.name())
                .cloned()
                .collect_vec();
            self.upgrade(LuaDependencyType::Test(packages), package_db)
                .await?;
        }
        Ok(())
    }
}

fn prepare_dependency_tables(project_toml: &mut DocumentMut) {
    if !project_toml.contains_table("dependencies") {
        let mut table = toml_edit::table().into_table().unwrap();
        table.set_implicit(true);

        project_toml["dependencies"] = toml_edit::Item::Table(table);
    }
    if !project_toml.contains_table("build_dependencies") {
        let mut table = toml_edit::table().into_table().unwrap();
        table.set_implicit(true);

        project_toml["build_dependencies"] = toml_edit::Item::Table(table);
    }
    if !project_toml.contains_table("test_dependencies") {
        let mut table = toml_edit::table().into_table().unwrap();
        table.set_implicit(true);

        project_toml["test_dependencies"] = toml_edit::Item::Table(table);
    }
    if !project_toml.contains_table("external_dependencies") {
        let mut table = toml_edit::table().into_table().unwrap();
        table.set_implicit(true);

        project_toml["external_dependencies"] = toml_edit::Item::Table(table);
    }
}

// TODO: More project-based test
#[cfg(test)]
mod tests {
    use assert_fs::prelude::PathCopy;
    use url::Url;

    use super::*;
    use crate::{
        lua_rockspec::RockSourceSpec,
        manifest::{Manifest, ManifestMetadata},
        package::PackageReq,
        rockspec::Rockspec,
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

        // Reparse the lux.toml (not usually necessary, but we want to test that the file was
        // written correctly)
        let project = Project::from(&project_root).unwrap().unwrap();
        let validated_toml = project.toml().into_remote().unwrap();
        assert_eq!(
            strip_lua(validated_toml.dependencies().current_platform()),
            expected_dependencies
        );
        assert_eq!(
            strip_lua(validated_toml.build_dependencies().current_platform()),
            expected_dependencies
        );
        assert_eq!(
            strip_lua(validated_toml.test_dependencies().current_platform()),
            expected_dependencies
        );
        assert_eq!(
            validated_toml
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

        let rocks = project.toml().into_remote().unwrap();

        assert_eq!(rocks.package().to_string(), "custom-package");
        assert_eq!(rocks.version().to_string(), "2.0.0-1");
        assert!(
            matches!(&rocks.source().current_platform().source_spec, RockSourceSpec::Url(url) if url == &Url::parse("https://github.com/custom/url").unwrap())
        );
    }
}
