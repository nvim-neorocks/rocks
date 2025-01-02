use std::{io, path::PathBuf, string::FromUtf8Error};

use bytes::Bytes;
use thiserror::Error;

use crate::{
    config::Config,
    package::{PackageName, PackageReq, PackageVersion, RemotePackage},
    progress::{Progress, ProgressBar},
    remote_package_db::{RemotePackageDB, RemotePackageDBError, SearchError},
    rockspec::{Rockspec, RockspecError},
};

pub struct Download<'a> {
    package_req: &'a PackageReq,
    package_db: Option<&'a RemotePackageDB>,
    config: &'a Config,
    progress: &'a Progress<ProgressBar>,
}

/// Builder for a rock downloader.
impl<'a> Download<'a> {
    /// Construct a new `.src.rock` downloader.
    pub fn new(
        package_req: &'a PackageReq,
        config: &'a Config,
        progress: &'a Progress<ProgressBar>,
    ) -> Self {
        Self {
            package_req,
            package_db: None,
            config,
            progress,
        }
    }

    /// Sets the package database to use for searching for packages.
    /// Instantiated from the config if not set.
    pub fn package_db(self, package_db: &'a RemotePackageDB) -> Self {
        Self {
            package_db: Some(package_db),
            ..self
        }
    }

    /// Download the package's Rockspec.
    pub async fn download_rockspec(self) -> Result<Rockspec, SearchAndDownloadError> {
        match self.package_db {
            Some(db) => download_rockspec(self.package_req, db, self.progress).await,
            None => {
                let db = RemotePackageDB::from_config(self.config).await?;
                download_rockspec(self.package_req, &db, self.progress).await
            }
        }
    }

    /// Download a `.src.rock` to a file.
    /// `destination_dir` defaults to the current working directory if not set.
    pub async fn download_src_rock_to_file(
        self,
        destination_dir: Option<PathBuf>,
    ) -> Result<DownloadedSrcRock, SearchAndDownloadError> {
        match self.package_db {
            Some(db) => {
                download_src_rock_to_file(self.package_req, destination_dir, db, self.progress)
                    .await
            }
            None => {
                let db = RemotePackageDB::from_config(self.config).await?;
                download_src_rock_to_file(self.package_req, destination_dir, &db, self.progress)
                    .await
            }
        }
    }

    /// Search for a `.src.rock` and download it to memory.
    pub async fn search_and_download_src_rock(
        self,
    ) -> Result<DownloadedSrcRockBytes, SearchAndDownloadError> {
        match self.package_db {
            Some(db) => search_and_download_src_rock(self.package_req, db, self.progress).await,
            None => {
                let db = RemotePackageDB::from_config(self.config).await?;
                search_and_download_src_rock(self.package_req, &db, self.progress).await
            }
        }
    }
}

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
    #[error("error initialising remote package DB: {0}")]
    RemotePackageDB(#[from] RemotePackageDBError),
}

async fn download_rockspec(
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
    #[error("error initialising remote package DB: {0}")]
    RemotePackageDB(#[from] RemotePackageDBError),
}

async fn search_and_download_src_rock(
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

async fn download_src_rock_to_file(
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
