use std::path::PathBuf;

use eyre::{eyre, Result};
use indicatif::MultiProgress;

use crate::{
    config::Config,
    lua_package::{LuaPackage, LuaPackageReq, PackageName, PackageVersion},
    progress::with_spinner,
};

pub struct DownloadedRock {
    pub name: PackageName,
    pub version: PackageVersion,
    pub path: PathBuf,
}

pub async fn download(
    progress: &MultiProgress,
    package_req: &LuaPackageReq,
    destination_dir: Option<PathBuf>,
    config: &Config,
) -> Result<DownloadedRock> {
    let package = with_spinner(progress, "🔎 Searching manifest...".into(), || async {
        search_manifest(package_req, config).await
    })
    .await?;
    with_spinner(progress, format!("📥 Downloading {}", package), || async {
        download_impl(package, destination_dir, config).await
    })
    .await
}

async fn search_manifest(package_req: &LuaPackageReq, config: &Config) -> Result<LuaPackage> {
    let manifest = crate::manifest::ManifestMetadata::from_config(config).await?;
    if !manifest.has_rock(package_req.name()) {
        return Err(eyre!(format!(
            "Rock '{}' does not exist on {}'s manifest.",
            package_req.name(),
            config.server()
        )));
    }
    Ok(manifest.latest_match(package_req).unwrap())
}

async fn download_impl(
    package: LuaPackage,
    destination_dir: Option<PathBuf>,
    config: &Config,
) -> Result<DownloadedRock> {
    // TODO(vhyrro): Check if the rock has a `src` attribute, add better error checking. Make sure to use the latest version of a rock if the version is ommitted.

    let full_rock_name = format!("{}-{}.src.rock", package.name(), package.version());

    let rock = reqwest::get(format!("{}/{}", config.server(), full_rock_name))
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
