use std::path::PathBuf;

use eyre::{eyre, Result};

use crate::config::Config;

pub async fn download(
    rock_name: &String,
    rock_version: Option<&String>,
    destination_dir: Option<PathBuf>,
    config: &Config,
) -> Result<(String, String)> {
    // TODO(vhyrro): Check if the rock has a `src` attribute, add better error checking. Make sure to use the latest version of a rock if the version is ommitted.

    let manifest = crate::manifest::ManifestMetadata::from_config(&config).await?;

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
            .unwrap_or_else(|| full_rock_name.into()),
        &rock,
    )?;

    Ok((rock_name.clone(), rock_version.clone()))
}
