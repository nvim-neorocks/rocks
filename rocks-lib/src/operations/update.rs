use std::sync::Arc;

use itertools::Itertools as _;
use thiserror::Error;

use crate::{
    build::BuildBehaviour,
    config::Config,
    lockfile::{LocalPackage, PinnedState},
    package::{PackageReq, RockConstraintUnsatisfied},
    progress::{MultiProgress, Progress},
    remote_package_db::{RemotePackageDB, RemotePackageDBError},
};

use super::{Install, InstallError, Remove, RemoveError};

/// A rocks package updater, providing fine-grained control
/// over how packages should be updated.
/// Can update multiple packages in parallel.
pub struct Update<'a> {
    config: &'a Config,
    packages: Vec<(LocalPackage, PackageReq)>,
    package_db: Option<RemotePackageDB>,
    progress: Option<Arc<Progress<MultiProgress>>>,
}

impl<'a> Update<'a> {
    /// Construct a new updater.
    pub fn new(config: &'a Config) -> Self {
        Self {
            config,
            packages: Vec::new(),
            package_db: None,
            progress: None,
        }
    }

    /// Specify packages to update.
    /// The first element of each tuple is a local package to update,
    /// the second element is a package to update to.
    pub fn packages<I>(self, packages: I) -> Self
    where
        I: IntoIterator<Item = (LocalPackage, PackageReq)>,
    {
        Self {
            packages: self.packages.into_iter().chain(packages).collect_vec(),
            ..self
        }
    }

    /// Add a package to the set of packages to update.
    pub fn package(self, from: LocalPackage, to: PackageReq) -> Self {
        self.packages(std::iter::once((from, to)))
    }

    /// Sets the package database to use for searching for packages.
    /// Instantiated from the config if not set.
    pub fn package_db(self, package_db: RemotePackageDB) -> Self {
        Self {
            package_db: Some(package_db),
            ..self
        }
    }

    /// Pass a `MultiProgress` to this installer.
    /// By default, a new one will be created.
    pub fn progress(self, progress: Arc<Progress<MultiProgress>>) -> Self {
        Self {
            progress: Some(progress),
            ..self
        }
    }

    /// Update the packages.
    pub async fn update(self) -> Result<(), UpdateError> {
        let package_db = match self.package_db {
            Some(db) => db,
            None => RemotePackageDB::from_config(self.config).await?,
        };
        let progress = match self.progress {
            Some(p) => p,
            None => MultiProgress::new_arc(),
        };
        update(self.packages, package_db, self.config, progress).await
    }
}

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
}

async fn update(
    packages: Vec<(LocalPackage, PackageReq)>,
    package_db: RemotePackageDB,
    config: &Config,
    progress: Arc<Progress<MultiProgress>>,
) -> Result<(), UpdateError> {
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
        println!("Nothing to update.");
        Ok(())
    } else {
        Install::new(config)
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
            .packages(packages.into_iter().map(|package| package.0))
            .progress(progress)
            .remove()
            .await?;
        Ok(())
    }
}
