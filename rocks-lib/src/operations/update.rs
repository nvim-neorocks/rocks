use thiserror::Error;

use crate::{
    build::BuildBehaviour,
    config::Config,
    lockfile::{LocalPackage, PinnedState},
    manifest::ManifestMetadata,
    package::{PackageReq, RemotePackage, RockConstraintUnsatisfied},
    progress::{MultiProgress, ProgressBar},
};

use super::{install, remove, InstallError, RemoveError};

#[derive(Error, Debug)]
pub enum UpdateError {
    #[error(transparent)]
    RockConstraintUnsatisfied(#[from] RockConstraintUnsatisfied),
    #[error("failed to update rock {package}: {error}")]
    Install {
        #[source]
        error: InstallError,
        package: RemotePackage,
    },
    #[error("failed to remove old rock ({package}) after update: {error}")]
    Remove {
        #[source]
        error: RemoveError,
        package: RemotePackage,
    },
}

pub async fn update(
    progress: &MultiProgress,
    package: LocalPackage,
    constraint: PackageReq,
    manifest: &ManifestMetadata,
    config: &Config,
) -> Result<(), UpdateError> {
    let bar = progress.add(ProgressBar::from(format!("Updating {}...", package.name())));

    let latest_version = package
        .to_package()
        .has_update_with(&constraint, manifest)?;

    if latest_version.is_some() && package.pinned() == PinnedState::Unpinned {
        // TODO(vhyrro): There's a slight dissonance in the API here.
        // `install` expects a MultiProgress, since it assumes you'll be installing
        // many rocks. We might want to have a function for installing a single package, too,
        // which would then allow us to just pass a `ProgressBar` instead.

        // Install the newest package.
        install(
            progress,
            vec![(BuildBehaviour::NoForce, constraint)],
            PinnedState::Unpinned,
            manifest,
            config,
        )
        .await
        .map_err(|error| UpdateError::Install {
            error,
            package: package.to_package(),
        })?;

        // Remove the old package
        remove(&bar, package.clone(), config)
            .await
            .map_err(|error| UpdateError::Remove {
                error,
                package: package.to_package(),
            })?;
    } else {
        // TODO: Print "nothing to update" progress update
    }

    Ok(())
}
