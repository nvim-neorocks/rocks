use std::path::PathBuf;

use bytes::Bytes;
use eyre::{eyre, Result};
use indicatif::MultiProgress;

use crate::{
    config::Config,
    progress::with_spinner,
    remote_package::{PackageName, PackageReq, PackageVersion, RemotePackage},
};

pub struct DownloadedRockBytes {
    pub name: PackageName,
    pub version: PackageVersion,
    pub bytes: Bytes,
}

pub struct DownloadedRock {
    pub name: PackageName,
    pub version: PackageVersion,
    pub path: PathBuf,
}

pub async fn download(
    progress: &MultiProgress,
    package_req: &PackageReq,
    config: &Config,
) -> Result<DownloadedRockBytes> {
    let package = with_spinner(progress, "🔎 Searching manifest...".into(), || async {
        search_manifest(package_req, config).await
    })
    .await?;
    with_spinner(progress, format!("📥 Downloading {}", package), || async {
        download_impl(package, config).await
    })
    .await
}

pub async fn download_to_file(
    progress: &MultiProgress,
    package_req: &PackageReq,
    destination_dir: Option<PathBuf>,
    config: &Config,
) -> Result<DownloadedRock> {
    let rock = download(progress, package_req, config).await?;
    let full_rock_name = full_rock_name(&rock.name, &rock.version);
    std::fs::write(
        destination_dir
            .map(|dest| dest.join(&full_rock_name))
            .unwrap_or_else(|| full_rock_name.clone().into()),
        &rock.bytes,
    )?;

    Ok(DownloadedRock {
        name: rock.name.to_owned(),
        version: rock.version.to_owned(),
        path: full_rock_name.into(),
    })
}

async fn search_manifest(package_req: &PackageReq, config: &Config) -> Result<RemotePackage> {
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

async fn download_impl(package: RemotePackage, config: &Config) -> Result<DownloadedRockBytes> {
    // TODO(vhyrro): Check if the rock has a `src` attribute, add better error checking. Make sure to use the latest version of a rock if the version is ommitted.

    let full_rock_name = full_rock_name(package.name(), package.version());

    let bytes = reqwest::get(format!("{}/{}", config.server(), full_rock_name))
        .await?
        .bytes()
        .await?;
    Ok(DownloadedRockBytes {
        name: package.name().to_owned(),
        version: package.version().to_owned(),
        bytes,
    })
}

fn full_rock_name(name: &PackageName, version: &PackageVersion) -> String {
    format!("{}-{}.src.rock", name, version)
}
