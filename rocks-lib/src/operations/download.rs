use std::path::PathBuf;

use eyre::{eyre, Result};

use crate::{config::Config, lua_package::PackageName};

pub struct DownloadedRock {
    pub name: PackageName,
    pub version: String,
    pub path: PathBuf,
}

pub async fn download(
    rock_name: &PackageName,
    rock_version: Option<&String>,
    destination_dir: Option<PathBuf>,
    config: &Config,
) -> Result<DownloadedRock> {
    // TODO(vhyrro): Check if the rock has a `src` attribute, add better error checking. Make sure to use the latest version of a rock if the version is ommitted.

    let manifest = crate::manifest::ManifestMetadata::from_config(config).await?;

    if !manifest.has_rock(rock_name) {
        return Err(eyre!(format!(
            "Rock '{}' does not exist on {}'s manifest.",
            rock_name, config.server
        )));
    }

    let rock_version = rock_version.unwrap_or_else(|| manifest.latest_version(rock_name).unwrap());

    let full_rock_name = format!("{}-{}.src.rock", rock_name, rock_version);

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
        name: rock_name.clone(),
        version: rock_version.clone(),
        path: full_rock_name.into(),
    })
}
