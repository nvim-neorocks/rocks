use std::{io, path::PathBuf, string::FromUtf8Error};

use bytes::Bytes;
use thiserror::Error;

use crate::{
    package::{PackageName, PackageReq, PackageVersion, RemotePackage},
    progress::{Progress, ProgressBar},
    remote_package_db::{RemotePackageDB, SearchError},
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
    package_db: &RemotePackageDB,
    progress: &Progress<ProgressBar>,
) -> Result<Rockspec, SearchAndDownloadError> {
    let package = package_db.find(package_req, progress)?;
    progress.map(|p| p.set_message(format!("📥 Downloading rockspec for {}", package_req)));
    download_rockspec_impl(package).await
}

#[derive(Error, Debug)]
pub enum SearchAndDownloadError {
    #[error(transparent)]
    Search(#[from] SearchError),
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
    package_db: &RemotePackageDB,
    progress: &Progress<ProgressBar>,
) -> Result<DownloadedSrcRockBytes, SearchAndDownloadError> {
    let package = package_db.find(package_req, progress)?;
    Ok(download_src_rock(&package, progress).await?)
}

#[derive(Error, Debug)]
#[error("failed to download source rock: {0}")]
pub struct DownloadSrcRockError(#[from] reqwest::Error);

pub(crate) async fn download_src_rock(
    remote_package: &RemotePackage,
    progress: &Progress<ProgressBar>,
) -> Result<DownloadedSrcRockBytes, DownloadSrcRockError> {
    progress.map(|p| p.set_message(format!("📥 Downloading {}", remote_package.package)));

    download_src_rock_impl(remote_package).await
}

pub async fn download_to_file(
    package_req: &PackageReq,
    destination_dir: Option<PathBuf>,
    package_db: &RemotePackageDB,
    progress: &Progress<ProgressBar>,
) -> Result<DownloadedSrcRock, SearchAndDownloadError> {
    progress.map(|p| p.set_message(format!("📥 Downloading {}", package_req)));

    let rock = search_and_download_src_rock(package_req, package_db, progress).await?;
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

async fn download_rockspec_impl(
    remote_package: RemotePackage,
) -> Result<Rockspec, SearchAndDownloadError> {
    let package = &remote_package.package;
    let rockspec_name = format!("{}-{}.rockspec", package.name(), package.version());
    let bytes = reqwest::get(format!("{}/{}", &remote_package.server_url, rockspec_name))
        .await
        .map_err(DownloadRockspecError::Request)?
        .bytes()
        .await
        .map_err(DownloadRockspecError::Request)?;
    let content = String::from_utf8(bytes.into())?;
    Ok(Rockspec::new(&content)?)
}

async fn download_src_rock_impl(
    remote_package: &RemotePackage,
) -> Result<DownloadedSrcRockBytes, DownloadSrcRockError> {
    let package = &remote_package.package;
    let full_rock_name = full_rock_name(package.name(), package.version());

    let bytes = reqwest::get(format!("{}/{}", remote_package.server_url, full_rock_name))
        .await?
        .bytes()
        .await?;
    Ok(DownloadedSrcRockBytes {
        name: package.name().clone(),
        version: package.version().clone(),
        bytes,
        file_name: full_rock_name,
    })
}

fn full_rock_name(name: &PackageName, version: &PackageVersion) -> String {
    format!("{}-{}.src.rock", name, version)
}
