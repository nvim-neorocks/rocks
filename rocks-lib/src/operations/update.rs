use itertools::Itertools;
use thiserror::Error;

use crate::{
    build::BuildBehaviour,
    config::Config,
    lockfile::{LocalPackage, PinnedState},
    manifest::ManifestMetadata,
    package::{PackageReq, RockConstraintUnsatisfied},
    progress::{MultiProgress, Progress},
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
    manifest: &ManifestMetadata,
    config: &Config,
    progress: &Progress<MultiProgress>,
) -> Result<(), UpdateError> {
    let updatable = packages
        .clone()
        .into_iter()
        .filter_map(|(package, constraint)| {
            match package.to_package().has_update_with(&constraint, manifest) {
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
            manifest,
            config,
            progress,
        )
        .await?;

        remove(
            packages.into_iter().map(|package| package.0).collect(),
            config,
            progress,
        )
        .await?;

        Ok(())
    }
}
