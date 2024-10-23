use eyre::Result;
use indicatif::MultiProgress;

use crate::{
    config::Config,
    lockfile::{LocalPackage, PinnedState},
    manifest::ManifestMetadata,
    package::PackageReq,
    progress::with_spinner,
};

use super::{install, remove};

pub async fn update(
    progress: &MultiProgress,
    package: LocalPackage,
    constraint: PackageReq,
    manifest: &ManifestMetadata,
    config: &Config,
) -> Result<()> {
    with_spinner(
        progress,
        format!("Updating {}...", package.name),
        || async move {
            let latest_version = package
                .to_package()
                .has_update_with(&constraint, manifest)?;

            if latest_version.is_some() && package.pinned() == PinnedState::Unpinned {
                // Install the newest package.
                install(progress, constraint, PinnedState::Unpinned, config).await?;

                // Remove the old package
                remove(progress, package, config).await?;
            } else {
                // TODO: Print "nothing to update" progress update
            }

            Ok(())
        },
    )
    .await
}
