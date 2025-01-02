use std::sync::Arc;

use thiserror::Error;

use crate::{
    build::BuildBehaviour,
    config::Config,
    lockfile::{LocalPackage, PinnedState},
    package::{PackageReq, PackageSpec, RockConstraintUnsatisfied},
    progress::{MultiProgress, Progress, ProgressBar},
    remote_package_db::RemotePackageDB,
};

use super::{remove, Install, InstallError, RemoveError};

#[derive(Error, Debug)]
pub enum UpdateError {
    #[error(transparent)]
    RockConstraintUnsatisfied(#[from] RockConstraintUnsatisfied),
    #[error("failed to update rock {package}: {error}")]
    Install {
        #[source]
        error: InstallError,
        package: PackageSpec,
    },
    #[error("failed to remove old rock ({package}) after update: {error}")]
    Remove {
        #[source]
        error: RemoveError,
        package: PackageSpec,
    },
}

pub async fn update(
    package: LocalPackage,
    constraint: PackageReq,
    package_db: RemotePackageDB,
    config: &Config,
    progress: Arc<Progress<MultiProgress>>,
) -> Result<(), UpdateError> {
    let bar = progress.map(|p| p.add(ProgressBar::from(format!("Updating {}...", package.name()))));

    let latest_version = package
        .to_package()
        .has_update_with(&constraint, &package_db)?;

    if latest_version.is_some() && package.pinned() == PinnedState::Unpinned {
        // TODO(vhyrro): There's a slight dissonance in the API here.
        // `install` expects a MultiProgress, since it assumes you'll be installing
        // many rocks. We might want to have a function for installing a single package, too,
        // which would then allow us to just pass a `ProgressBar` instead.

        // Install the newest package.
        Install::new(config)
            .package(BuildBehaviour::NoForce, constraint)
            .package_db(package_db)
            .progress(progress)
            .install()
            .await
            .map_err(|error| UpdateError::Install {
                error,
                package: package.to_package(),
            })?;

        // Remove the old package
        remove(package.clone(), config, &bar)
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
