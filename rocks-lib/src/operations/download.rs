use std::path::PathBuf;

use bytes::Bytes;
use eyre::{eyre, Result};
use indicatif::MultiProgress;

use crate::{
    config::Config,
    package::{PackageName, PackageReq, PackageVersion, RemotePackage},
    progress::with_spinner,
    rockspec::Rockspec,
};

pub struct DownloadedSrcRockBytes {
    pub name: PackageName,
    pub version: PackageVersion,
    pub bytes: Bytes,
    pub file_name: String,
}

pub struct DownloadedSrcRock {
    pub name: PackageName,
    pub version: PackageVersion,
    pub path: PathBuf,
}

pub async fn download_rockspec(
    progress: &MultiProgress,
    package_req: &PackageReq,
    config: &Config,
) -> Result<Rockspec> {
    let package = search_manifest(progress, package_req, config).await?;
    with_spinner(
        progress,
        format!("📥 Downloading RockSpec for {}", package),
        || async { download_rockspec_impl(package, config).await },
    )
    .await
}

pub async fn search_and_download_src_rock(
    progress: &MultiProgress,
    package_req: &PackageReq,
    config: &Config,
) -> Result<DownloadedSrcRockBytes> {
    let package = search_manifest(progress, package_req, config).await?;
    download_src_rock(progress, &package, config).await
}

pub async fn download_src_rock(
    progress: &MultiProgress,
    package: &RemotePackage,
    config: &Config,
) -> Result<DownloadedSrcRockBytes> {
    with_spinner(progress, format!("📥 Downloading {}", package), || async {
        download_src_rock_impl(package, config).await
    })
    .await
}

pub async fn download_to_file(
    progress: &MultiProgress,
    package_req: &PackageReq,
    destination_dir: Option<PathBuf>,
    config: &Config,
) -> Result<DownloadedSrcRock> {
    let rock = search_and_download_src_rock(progress, package_req, config).await?;
    let full_rock_name = full_rock_name(&rock.name, &rock.version);
    std::fs::write(
        destination_dir
            .map(|dest| dest.join(&full_rock_name))
            .unwrap_or_else(|| full_rock_name.clone().into()),
        &rock.bytes,
    )?;

    Ok(DownloadedSrcRock {
        name: rock.name.to_owned(),
        version: rock.version.to_owned(),
        path: full_rock_name.into(),
    })
}

async fn search_manifest(
    progress: &MultiProgress,
    package_req: &PackageReq,
    config: &Config,
) -> Result<RemotePackage> {
    with_spinner(progress, "🔎 Searching manifest...".into(), || async {
        search_manifest_impl(package_req, config).await
    })
    .await
}

async fn search_manifest_impl(package_req: &PackageReq, config: &Config) -> Result<RemotePackage> {
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

async fn download_rockspec_impl(package: RemotePackage, config: &Config) -> Result<Rockspec> {
    let rockspec_name = format!("{}-{}.rockspec", package.name(), package.version());
    let bytes = reqwest::get(format!("{}/{}", config.server(), rockspec_name))
        .await?
        .bytes()
        .await?;
    let content = String::from_utf8(bytes.into())?;
    Rockspec::new(&content)
}

async fn download_src_rock_impl(
    package: &RemotePackage,
    config: &Config,
) -> Result<DownloadedSrcRockBytes> {
    let full_rock_name = full_rock_name(package.name(), package.version());

    let bytes = reqwest::get(format!("{}/{}", config.server(), full_rock_name))
        .await?
        .bytes()
        .await?;
    Ok(DownloadedSrcRockBytes {
        name: package.name().to_owned(),
        version: package.version().to_owned(),
        bytes,
        file_name: full_rock_name,
    })
}

fn full_rock_name(name: &PackageName, version: &PackageVersion) -> String {
    format!("{}-{}.src.rock", name, version)
}
