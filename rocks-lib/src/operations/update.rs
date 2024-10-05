use eyre::Result;
use indicatif::MultiProgress;

use crate::{
    config::Config,
    lua_package::{LuaPackage, LuaPackageReq},
    manifest::ManifestMetadata,
    progress::with_spinner,
};

use super::{install, remove};

pub async fn update(
    progress: &MultiProgress,
    package: LuaPackage,
    constraint: LuaPackageReq,
    manifest: &ManifestMetadata,
    config: &Config,
) -> Result<()> {
    with_spinner(
        progress,
        format!("Updating {}...", package.name()),
        || async move {
            let latest_version = package.has_update_with(&constraint, manifest)?;

            if latest_version.is_some() {
                // Install the newest package.
                install(progress, constraint, config).await?;

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
