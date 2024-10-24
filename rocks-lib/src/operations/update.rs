use indicatif::MultiProgress;
use thiserror::Error;

use crate::{
    config::Config,
    lockfile::LocalPackage,
    manifest::ManifestMetadata,
    package::{PackageReq, RemotePackage, RockConstraintUnsatisfied},
    progress::with_spinner,
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
    with_spinner(
        progress,
        format!("Updating {}...", package.name),
        || async move {
            let latest_version = package
                .to_package()
                .has_update_with(&constraint, manifest)?;

            if latest_version.is_some() && !package.pinned() {
                // Install the newest package.
                install(progress, constraint, false, config)
                    .await
                    .map_err(|error| UpdateError::Install {
                        error,
                        package: package.to_package(),
                    })?;

                // Remove the old package
                remove(progress, package.clone(), config)
                    .await
                    .map_err(|error| UpdateError::Remove {
                        error,
                        package: package.to_package(),
                    })?;
            } else {
                // TODO: Print "nothing to update" progress update
            }

            Ok(())
        },
    )
    .await
}
