use std::sync::Arc;

use bon::Builder;
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
#[derive(Builder)]
#[builder(start_fn = new, finish_fn(name = _update, vis = ""))]
pub struct Update<'a> {
    #[builder(start_fn)]
    config: &'a Config,

    #[builder(field)]
    packages: Vec<(LocalPackage, PackageReq)>,

    package_db: Option<RemotePackageDB>,

    #[builder(default = MultiProgress::new_arc())]
    progress: Arc<Progress<MultiProgress>>,
}

impl<State: update_builder::State> UpdateBuilder<'_, State> {
    pub fn package(mut self, package_with_req: (LocalPackage, PackageReq)) -> Self {
        self.packages.push(package_with_req);
        self
    }

    pub fn packages(
        mut self,
        packages: impl IntoIterator<Item = (LocalPackage, PackageReq)>,
    ) -> Self {
        self.packages.extend(packages);
        self
    }

    pub async fn update(self) -> Result<(), UpdateError>
    where
        State: update_builder::IsComplete,
    {
        let new_self = self._update();

        let package_db = match new_self.package_db {
            Some(db) => db,
            None => {
                let bar = new_self.progress.map(|p| p.new_bar());
                let db = RemotePackageDB::from_config(new_self.config, &bar).await?;
                bar.map(|b| b.finish_and_clear());
                db
            }
        };

        update(
            new_self.packages,
            package_db,
            new_self.config,
            new_self.progress,
        )
        .await
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
            .packages(packages.into_iter().map(|package| package.0.id()))
            .progress(progress)
            .remove()
            .await?;
        Ok(())
    }
}
