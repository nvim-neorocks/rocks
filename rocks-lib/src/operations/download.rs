use std::{io, path::PathBuf, string::FromUtf8Error};

use bytes::Bytes;
use thiserror::Error;

use crate::{
    config::Config,
    manifest::{ManifestError, ManifestMetadata},
    package::{PackageName, PackageReq, PackageVersion, RemotePackage},
    progress::{Progress, ProgressBar},
    rockspec::{Rockspec, RockspecError},
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

#[derive(Error, Debug)]
pub enum DownloadRockspecError {
    #[error("failed to download rockspec: {0}")]
    Request(#[from] reqwest::Error),
    #[error("failed to convert rockspec response: {0}")]
    ResponseConversion(#[from] FromUtf8Error),
}

pub async fn download_rockspec(
    package_req: &PackageReq,
    manifest: &ManifestMetadata,
    config: &Config,
    progress: &Progress<ProgressBar>,
) -> Result<Rockspec, SearchAndDownloadError> {
    let package = search_manifest(package_req, manifest, config, progress).await?;

    progress.map(|p| p.set_message(format!("📥 Downloading rockspec for {}", package_req)));

    download_rockspec_impl(package, config).await
}

#[derive(Error, Debug)]
pub enum SearchAndDownloadError {
    #[error(transparent)]
    Search(#[from] SearchManifestError),
    #[error(transparent)]
    Download(#[from] DownloadSrcRockError),
    #[error(transparent)]
    DownloadRockspec(#[from] DownloadRockspecError),
    #[error("io operation failed: {0}")]
    Io(#[from] io::Error),
    #[error("UTF-8 conversion failed: {0}")]
    Utf8(#[from] FromUtf8Error),
    #[error(transparent)]
    Rockspec(#[from] RockspecError),
}

pub async fn search_and_download_src_rock(
    package_req: &PackageReq,
    manifest: &ManifestMetadata,
    config: &Config,
    progress: &Progress<ProgressBar>,
) -> Result<DownloadedSrcRockBytes, SearchAndDownloadError> {
    let package = search_manifest(package_req, manifest, config, progress).await?;
    Ok(download_src_rock(&package, config, progress).await?)
}

#[derive(Error, Debug)]
#[error("failed to download source rock: {0}")]
pub struct DownloadSrcRockError(#[from] reqwest::Error);

pub async fn download_src_rock(
    package: &RemotePackage,
    config: &Config,
    progress: &Progress<ProgressBar>,
) -> Result<DownloadedSrcRockBytes, DownloadSrcRockError> {
    progress.map(|p| p.set_message(format!("📥 Downloading {}", package)));

    download_src_rock_impl(package, config).await
}

pub async fn download_to_file(
    package_req: &PackageReq,
    destination_dir: Option<PathBuf>,
    manifest: &ManifestMetadata,
    config: &Config,
    progress: &Progress<ProgressBar>,
) -> Result<DownloadedSrcRock, SearchAndDownloadError> {
    progress.map(|p| p.set_message(format!("📥 Downloading {}", package_req)));

    let rock = search_and_download_src_rock(package_req, manifest, config, progress).await?;
    let full_rock_name = full_rock_name(&rock.name, &rock.version);
    tokio::fs::write(
        destination_dir
            .map(|dest| dest.join(&full_rock_name))
            .unwrap_or_else(|| full_rock_name.clone().into()),
        &rock.bytes,
    )
    .await?;

    Ok(DownloadedSrcRock {
        name: rock.name.to_owned(),
        version: rock.version.to_owned(),
        path: full_rock_name.into(),
    })
}

#[derive(Error, Debug)]
pub enum SearchManifestError {
    #[error(transparent)]
    Mlua(#[from] mlua::Error),
    #[error("rock '{name}' does not exist on {server}'s manifest")]
    RockNotFound { name: PackageName, server: String },
    #[error("error when pulling manifest: {0}")]
    Manifest(#[from] ManifestError),
}

async fn search_manifest(
    package_req: &PackageReq,
    manifest: &ManifestMetadata,
    config: &Config,
    progress: &Progress<ProgressBar>,
) -> Result<RemotePackage, SearchManifestError> {
    progress.map(|p| p.set_message("🔎 Searching manifest..."));

    search_manifest_impl(package_req, manifest, config).await
}

async fn search_manifest_impl(
    package_req: &PackageReq,
    manifest: &ManifestMetadata,
    config: &Config,
) -> Result<RemotePackage, SearchManifestError> {
    if !manifest.has_rock(package_req.name()) {
        return Err(SearchManifestError::RockNotFound {
            name: package_req.name().clone(),
            server: config.server().clone(),
        });
    }
    Ok(manifest.latest_match(package_req).unwrap())
}

async fn download_rockspec_impl(
    package: RemotePackage,
    config: &Config,
) -> Result<Rockspec, SearchAndDownloadError> {
    let rockspec_name = format!("{}-{}.rockspec", package.name(), package.version());
    let bytes = reqwest::get(format!("{}/{}", config.server(), rockspec_name))
        .await
        .map_err(DownloadRockspecError::Request)?
        .bytes()
        .await
        .map_err(DownloadRockspecError::Request)?;
    let content = String::from_utf8(bytes.into())?;
    Ok(Rockspec::new(&content)?)
}

async fn download_src_rock_impl(
    package: &RemotePackage,
    config: &Config,
) -> Result<DownloadedSrcRockBytes, DownloadSrcRockError> {
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
