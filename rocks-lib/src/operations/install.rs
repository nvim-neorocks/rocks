use std::{collections::HashMap, io, sync::Arc};

use crate::{
    build::{Build, BuildBehaviour, BuildError},
    config::{Config, LuaVersion, LuaVersionUnset},
    lockfile::{LocalPackage, LocalPackageId, Lockfile, PinnedState},
    luarocks_installation::{
        InstallBuildDependenciesError, LuaRocksError, LuaRocksInstallError, LuaRocksInstallation,
    },
    package::{PackageName, PackageReq},
    progress::{MultiProgress, Progress, ProgressBar},
    remote_package_db::{RemotePackageDB, RemotePackageDBError},
    rockspec::{BuildBackendSpec, LuaVersionError},
    tree::Tree,
};

use futures::future::join_all;
use itertools::Itertools;
use thiserror::Error;

use super::{resolve::get_all_dependencies, SearchAndDownloadError};

/// A rocks package installer, providing fine-grained control
/// over how packages should be installed.
/// Can install multiple packages in parallel.
pub struct Install<'a> {
    config: &'a Config,
    package_db: Option<RemotePackageDB>,
    packages: Vec<(BuildBehaviour, PackageReq)>,
    pin: PinnedState,
    progress: Option<Arc<Progress<MultiProgress>>>,
}

impl<'a> Install<'a> {
    /// Construct a new installer.
    pub fn new(config: &'a Config) -> Self {
        Self {
            config,
            package_db: None,
            packages: Vec::new(),
            pin: PinnedState::default(),
            progress: None,
        }
    }

    /// Sets the package database to use for searching for packages.
    /// Instantiated from the config if not set.
    pub fn package_db(self, package_db: RemotePackageDB) -> Self {
        Self {
            package_db: Some(package_db),
            ..self
        }
    }

    /// Specify packages to install, along with each package's build behaviour.
    pub fn packages<I>(self, packages: I) -> Self
    where
        I: IntoIterator<Item = (BuildBehaviour, PackageReq)>,
    {
        Self {
            packages: self.packages.into_iter().chain(packages).collect_vec(),
            ..self
        }
    }

    /// Add a package to the set of packages to install.
    pub fn package(self, behaviour: BuildBehaviour, package: PackageReq) -> Self {
        self.packages(std::iter::once((behaviour, package)))
    }

    pub fn pin(self, pin: PinnedState) -> Self {
        Self { pin, ..self }
    }

    /// Pass a `MultiProgress` to this installer.
    /// By default, a new one will be created.
    pub fn progress(self, progress: Arc<Progress<MultiProgress>>) -> Self {
        Self {
            progress: Some(progress),
            ..self
        }
    }

    /// Install the packages.
    pub async fn install(self) -> Result<Vec<LocalPackage>, InstallError> {
        let package_db = match self.package_db {
            Some(db) => db,
            None => RemotePackageDB::from_config(self.config).await?,
        };
        let progress = match self.progress {
            Some(p) => p,
            None => MultiProgress::new_arc(),
        };
        install(self.packages, self.pin, package_db, self.config, progress).await
    }
}

#[derive(Error, Debug)]
pub enum InstallError {
    #[error(transparent)]
    SearchAndDownloadError(#[from] SearchAndDownloadError),
    #[error(transparent)]
    LuaVersionError(#[from] LuaVersionError),
    #[error(transparent)]
    LuaVersionUnset(#[from] LuaVersionUnset),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("error instantiating LuaRocks compatibility layer: {0}")]
    LuaRocksError(#[from] LuaRocksError),
    #[error("error installing LuaRocks compatibility layer: {0}")]
    LuaRocksInstallError(#[from] LuaRocksInstallError),
    #[error("error installing LuaRocks build dependencies: {0}")]
    InstallBuildDependenciesError(#[from] InstallBuildDependenciesError),
    #[error("failed to build {0}: {1}")]
    BuildError(PackageName, BuildError),
    #[error("error initialising remote package DB: {0}")]
    RemotePackageDB(#[from] RemotePackageDBError),
}

async fn install(
    packages: Vec<(BuildBehaviour, PackageReq)>,
    pin: PinnedState,
    package_db: RemotePackageDB,
    config: &Config,
    progress: Arc<Progress<MultiProgress>>,
) -> Result<Vec<LocalPackage>, InstallError>
where
{
    let lua_version = LuaVersion::from(config)?;
    let tree = Tree::new(config.tree().clone(), lua_version)?;
    let mut lockfile = tree.lockfile()?;
    let result = install_impl(packages, pin, package_db, config, &mut lockfile, progress).await;
    lockfile.flush()?;
    result
}

async fn install_impl(
    packages: Vec<(BuildBehaviour, PackageReq)>,
    pin: PinnedState,
    package_db: RemotePackageDB,
    config: &Config,
    lockfile: &mut Lockfile,
    progress_arc: Arc<Progress<MultiProgress>>,
) -> Result<Vec<LocalPackage>, InstallError> {
    let progress = Arc::clone(&progress_arc);
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    get_all_dependencies(
        tx,
        packages,
        pin,
        Arc::new(package_db),
        Arc::new(lockfile.clone()),
        config,
        progress_arc.clone(),
    )
    .await?;

    let mut all_packages = HashMap::with_capacity(rx.len());

    while let Some(dep) = rx.recv().await {
        all_packages.insert(dep.spec.id(), dep);
    }

    let installed_packages = join_all(all_packages.clone().into_values().map(|install_spec| {
        let progress_arc = progress_arc.clone();
        let package = install_spec.rockspec.package.clone();

        let bar = progress.map(|p| {
            p.add(ProgressBar::from(format!(
                "💻 Installing {}",
                install_spec.rockspec.package,
            )))
        });
        let config = config.clone();

        tokio::spawn(async move {
            let rockspec = install_spec.rockspec;
            if let Some(BuildBackendSpec::LuaRock(build_backend)) =
                &rockspec.build.current_platform().build_backend
            {
                let luarocks = LuaRocksInstallation::new(&config)?;
                luarocks.ensure_installed(&bar).await?;
                luarocks
                    .install_build_dependencies(build_backend, &rockspec, progress_arc)
                    .await?;
            }

            let pkg = Build::new(rockspec, &config, &bar)
                .pin(pin)
                .constraint(install_spec.spec.constraint())
                .behaviour(install_spec.build_behaviour)
                .source(install_spec.source)
                .build()
                .await
                .map_err(|err| InstallError::BuildError(package, err))?;

            bar.map(|b| b.finish_and_clear());

            Ok::<_, InstallError>((pkg.id(), pkg))
        })
    }))
    .await
    .into_iter()
    .flatten()
    .try_collect::<_, HashMap<LocalPackageId, LocalPackage>, _>()?;

    installed_packages.iter().for_each(|(id, pkg)| {
        lockfile.add(pkg);

        all_packages
            .get(id)
            .map(|pkg| pkg.spec.dependencies())
            .unwrap_or_default()
            .into_iter()
            .for_each(|dependency_id| {
                lockfile.add_dependency(
                    pkg,
                    installed_packages
                        .get(dependency_id)
                        .expect("required dependency not found"),
                );
            });
    });

    Ok(installed_packages.into_values().collect_vec())
}
