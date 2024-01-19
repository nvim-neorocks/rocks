use anyhow::Result;

use crate::config::Config;

pub async fn download(
    rock_name: &String,
    rock_version: Option<&String>,
    config: &Config,
) -> Result<()> {
    // TODO(vhyrro): Check if the rock has a `src` attribute, allow custom manifests, allow custom
    // URLs, add better error checking. Make sure to use the latest version of a rock if the
    // version is ommitted.

    let full_rock_name = format!("{}-{}.src.rock", rock_name, rock_version.unwrap());

    let rock = reqwest::get(format!("{}/{}", config.server, full_rock_name))
        .await?
        .bytes()
        .await?;

    std::fs::write(full_rock_name, &rock)?;

    Ok(())
}
