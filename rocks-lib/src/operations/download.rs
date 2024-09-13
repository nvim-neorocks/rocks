use std::path::PathBuf;

use eyre::{eyre, Result};

use crate::{
    config::Config,
    lua_package::{LuaPackageReq, PackageName, PackageVersion},
};

pub struct DownloadedRock {
    pub name: PackageName,
    pub version: PackageVersion,
    pub path: PathBuf,
}

pub async fn download(
    package_req: &LuaPackageReq,
    destination_dir: Option<PathBuf>,
    config: &Config,
) -> Result<DownloadedRock> {
    // TODO(vhyrro): Check if the rock has a `src` attribute, add better error checking. Make sure to use the latest version of a rock if the version is ommitted.

    let manifest = crate::manifest::ManifestMetadata::from_config(config).await?;

    if !manifest.has_rock(package_req.name()) {
        return Err(eyre!(format!(
            "Rock '{}' does not exist on {}'s manifest.",
            package_req.name(),
            config.server
        )));
    }

    let package = manifest.latest_match(package_req).unwrap();

    let full_rock_name = format!("{}-{}.src.rock", package.name(), package.version());

    let rock = reqwest::get(format!("{}/{}", config.server, full_rock_name))
        .await?
        .bytes()
        .await?;

    std::fs::write(
        destination_dir
            .map(|dest| dest.join(&full_rock_name))
            .unwrap_or_else(|| full_rock_name.clone().into()),
        &rock,
    )?;

    Ok(DownloadedRock {
        name: package.name().to_owned(),
        version: package.version().to_owned(),
        path: full_rock_name.into(),
    })
}
