use std::{io, sync::Arc};

use bon::Builder;
use itertools::Itertools;
use thiserror::Error;

use crate::{
    build::BuildBehaviour,
    config::{Config, LuaVersion, LuaVersionUnset},
    lockfile::{
        LocalPackage, LocalPackageLockType, Lockfile, PinnedState, ProjectLockfile, ReadOnly,
        ReadWrite,
    },
    luarocks::luarocks_installation::{LuaRocksError, LuaRocksInstallation},
    package::{PackageName, PackageReq, RockConstraintUnsatisfied},
    progress::{MultiProgress, Progress},
    project::{Project, ProjectError, ProjectTreeError},
    remote_package_db::{RemotePackageDB, RemotePackageDBError},
    rockspec::Rockspec,
    tree::Tree,
};

use super::{Install, InstallError, Remove, RemoveError, SyncError};

#[derive(Error, Debug)]
pub enum UpdateError {
    #[error(transparent)]
    RockConstraintUnsatisfied(#[from] RockConstraintUnsatisfied),
    #[error("failed to update rock: {0}")]
    Install(#[from] InstallError),
    #[error("failed to remove old rock: {0}")]
    Remove(#[from] RemoveError),
    #[error("error initialising remote package DB: {0}")]
    RemotePackageDB(#[from] RemotePackageDBError),
    #[error("error loading project: {0}")]
    Project(#[from] ProjectError),
    #[error(transparent)]
    LuaVersionUnset(#[from] LuaVersionUnset),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("error initialising project tree: {0}")]
    ProjectTree(#[from] ProjectTreeError),
    #[error("error initialising luarocks build backend: {0}")]
    LuaRocks(#[from] LuaRocksError),
    #[error("error syncing the project tree: {0}")]
    Sync(#[from] SyncError),
}

/// A rocks package updater, providing fine-grained control
/// over how packages should be updated.
/// Can update multiple packages in parallel.
#[derive(Builder)]
#[builder(start_fn = new, finish_fn(name = _update, vis = ""))]
pub struct Update<'a> {
    #[builder(start_fn)]
    config: &'a Config,

    /// Packages to update.
    #[builder(field)]
    packages: Option<Vec<PackageReq>>,

    /// Whether to validate the integrity when syncing the project lockfile.
    validate_integrity: Option<bool>,

    package_db: Option<RemotePackageDB>,

    #[builder(default = MultiProgress::new_arc())]
    progress: Arc<Progress<MultiProgress>>,
}

impl<State: update_builder::State> UpdateBuilder<'_, State> {
    pub fn packages(mut self, packages: Option<Vec<PackageReq>>) -> Self {
        self.packages = packages;
        self
    }
}

impl<State: update_builder::State> UpdateBuilder<'_, State> {
    /// Returns the packages that were installed or removed
    pub async fn update(self) -> Result<Vec<LocalPackage>, UpdateError>
    where
        State: update_builder::IsComplete,
    {
        let args = self._update();

        let package_db = match &args.package_db {
            Some(db) => db.clone(),
            None => {
                let bar = args.progress.map(|p| p.new_bar());
                let db = RemotePackageDB::from_config(args.config, &bar).await?;
                bar.map(|b| b.finish_and_clear());
                db
            }
        };

        match Project::current()? {
            Some(project) => update_project(project, args, package_db).await,
            None => update_install_tree(args, package_db).await,
        }
    }
}

async fn update_project(
    project: Project,
    args: Update<'_>,
    package_db: RemotePackageDB,
) -> Result<Vec<LocalPackage>, UpdateError> {
    let toml = project.toml().into_validated().unwrap(); // TODO(mrcjkb): rebase on vhyrro's build refactor
    let mut project_lockfile = project.lockfile()?.write_guard();
    let tree = project.tree(args.config)?;

    let dependencies = toml
        .dependencies()
        .current_platform()
        .iter()
        .filter(|package| !package.name().eq(&PackageName::new("lua".into())))
        .cloned()
        .collect_vec();
    let dep_report = super::Sync::new(&tree, &mut project_lockfile, args.config)
        .validate_integrity(args.validate_integrity.unwrap_or(false))
        .packages(dependencies)
        .sync_dependencies()
        .await?;

    let updated_dependencies = update_dependency_tree(
        &tree,
        &mut project_lockfile,
        LocalPackageLockType::Regular,
        package_db.clone(),
        args.config,
        args.progress.clone(),
        &args.packages,
    )
    .await?
    .into_iter()
    .chain(dep_report.added)
    .chain(dep_report.removed);

    let test_tree = project.test_tree(args.config)?;
    let test_dependencies = toml.test_dependencies().current_platform().clone();
    let dep_report = super::Sync::new(&test_tree, &mut project_lockfile, args.config)
        .validate_integrity(false)
        .packages(test_dependencies)
        .sync_test_dependencies()
        .await?;
    let updated_test_dependencies = update_dependency_tree(
        &test_tree,
        &mut project_lockfile,
        LocalPackageLockType::Test,
        package_db.clone(),
        args.config,
        args.progress.clone(),
        &args.packages,
    )
    .await?
    .into_iter()
    .chain(dep_report.added)
    .chain(dep_report.removed);

    let luarocks = LuaRocksInstallation::new(args.config)?;
    let build_dependencies = toml.build_dependencies().current_platform().clone();
    let dep_report = super::Sync::new(luarocks.tree(), &mut project_lockfile, luarocks.config())
        .validate_integrity(false)
        .packages(build_dependencies)
        .sync_build_dependencies()
        .await?;
    let updated_build_dependencies = update_dependency_tree(
        luarocks.tree(),
        &mut project_lockfile,
        LocalPackageLockType::Build,
        package_db.clone(),
        luarocks.config(),
        args.progress.clone(),
        &args.packages,
    )
    .await?
    .into_iter()
    .chain(dep_report.added)
    .chain(dep_report.removed);

    Ok(updated_dependencies
        .into_iter()
        .chain(updated_test_dependencies)
        .chain(updated_build_dependencies)
        .collect_vec())
}

async fn update_dependency_tree(
    tree: &Tree,
    project_lockfile: &mut ProjectLockfile<ReadWrite>,
    lock_type: LocalPackageLockType,
    package_db: RemotePackageDB,
    config: &Config,
    progress: Arc<Progress<MultiProgress>>,
    packages: &Option<Vec<PackageReq>>,
) -> Result<Vec<LocalPackage>, UpdateError> {
    let lockfile = tree.lockfile()?;
    let dependencies = unpinned_packages(&lockfile)
        .into_iter()
        .filter(|pkg| is_included(pkg, packages))
        .collect_vec();
    let updated_dependencies = update(dependencies, package_db, tree, config, progress).await?;
    if !updated_dependencies.is_empty() {
        let updated_lockfile = tree.lockfile()?;
        project_lockfile.sync(updated_lockfile.local_pkg_lock(), &lock_type);
    }
    Ok(updated_dependencies)
}

fn is_included(
    (pkg, _): &(LocalPackage, PackageReq),
    package_reqs: &Option<Vec<PackageReq>>,
) -> bool {
    package_reqs.is_none()
        || package_reqs.as_ref().is_some_and(|packages| {
            packages
                .iter()
                .any(|req| req.matches(&pkg.as_package_spec()))
        })
}

async fn update_install_tree(
    args: Update<'_>,
    package_db: RemotePackageDB,
) -> Result<Vec<LocalPackage>, UpdateError> {
    let tree = args.config.tree(LuaVersion::from(args.config)?)?;
    let lockfile = tree.lockfile()?;
    let packages = unpinned_packages(&lockfile)
        .into_iter()
        .filter(|pkg| is_included(pkg, &args.packages))
        .collect_vec();
    update(packages, package_db, &tree, args.config, args.progress).await
}

async fn update(
    packages: Vec<(LocalPackage, PackageReq)>,
    package_db: RemotePackageDB,
    tree: &Tree,
    config: &Config,
    progress: Arc<Progress<MultiProgress>>,
) -> Result<Vec<LocalPackage>, UpdateError> {
    let updatable = packages
        .clone()
        .into_iter()
        .filter_map(|(package, constraint)| {
            match package
                .to_package()
                .has_update_with(&constraint, &package_db)
            {
                Ok(Some(_)) if package.pinned() == PinnedState::Unpinned => Some(constraint),
                _ => None,
            }
        })
        .collect_vec();
    if updatable.is_empty() {
        Ok(Vec::new())
    } else {
        let updated_packages = Install::new(tree, config)
            .packages(
                updatable
                    .iter()
                    .map(|constraint| (BuildBehaviour::NoForce, constraint.clone())),
            )
            .package_db(package_db)
            .progress(progress.clone())
            .install()
            .await?;
        Remove::new(config)
            .packages(packages.into_iter().map(|package| package.0.id()))
            .progress(progress)
            .remove()
            .await?;
        Ok(updated_packages)
    }
}

fn unpinned_packages(lockfile: &Lockfile<ReadOnly>) -> Vec<(LocalPackage, PackageReq)> {
    lockfile
        .rocks()
        .values()
        .filter(|package| package.pinned() == PinnedState::Unpinned)
        .map(|package| (package.clone(), package.to_package().into_package_req()))
        .collect_vec()
}
