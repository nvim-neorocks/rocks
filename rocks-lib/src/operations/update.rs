use itertools::Itertools;
use std::sync::Arc;
use thiserror::Error;

use crate::{
    build::BuildBehaviour,
    config::Config,
    lockfile::{LocalPackage, PinnedState},
    package::{PackageReq, RockConstraintUnsatisfied},
    progress::{MultiProgress, Progress},
    remote_package_db::RemotePackageDB,
};

use super::{install, remove, InstallError, RemoveError};

#[derive(Error, Debug)]
pub enum UpdateError {
    #[error(transparent)]
    RockConstraintUnsatisfied(#[from] RockConstraintUnsatisfied),
    #[error("failed to update rock: {0}")]
    Install(#[from] InstallError),
    #[error("failed to remove old rock: {0}")]
    Remove(#[from] RemoveError),
}

pub async fn update(
    packages: Vec<(LocalPackage, PackageReq)>,
    package_db: &RemotePackageDB,
    config: &Config,
    progress: Arc<Progress<MultiProgress>>,
) -> Result<(), UpdateError> {
    let updatable = packages
        .clone()
        .into_iter()
        .filter_map(|(package, constraint)| {
            match package
                .to_package()
                .has_update_with(&constraint, package_db)
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
        install(
            updatable
                .iter()
                .map(|constraint| (BuildBehaviour::NoForce, constraint.clone()))
                .collect(),
            PinnedState::Unpinned,
            package_db,
            config,
            progress.clone(),
        )
        .await?;

        remove(
            packages.into_iter().map(|package| package.0).collect(),
            config,
            &Arc::clone(&progress),
        )
        .await?;

        Ok(())
    }
}
