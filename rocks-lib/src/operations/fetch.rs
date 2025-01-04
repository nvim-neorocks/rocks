use bon::Builder;
use flate2::read::GzDecoder;
use git2::build::RepoBuilder;
use git2::FetchOptions;
use itertools::Itertools;
use std::fs::File;
use std::io;
use std::io::BufReader;
use std::io::Cursor;
use std::io::Read;
use std::io::Seek;
use std::path::Path;
use std::path::PathBuf;
use thiserror::Error;

use crate::config::Config;
use crate::operations;
use crate::package::PackageSpec;
use crate::progress::Progress;
use crate::progress::ProgressBar;
use crate::rockspec::RockSourceSpec;
use crate::rockspec::Rockspec;

use super::DownloadSrcRockError;

/// A rocks package source fetcher, providing fine-grained control
/// over how a package should be fetched.
#[derive(Builder)]
#[builder(start_fn = new, finish_fn(name = _build, vis = ""))]
pub struct FetchSrc<'a> {
    #[builder(start_fn)]
    dest_dir: &'a Path,
    #[builder(start_fn)]
    rockspec: &'a Rockspec,
    #[builder(start_fn)]
    config: &'a Config,
    #[builder(start_fn)]
    progress: &'a Progress<ProgressBar>,
}

impl<State> FetchSrcBuilder<'_, State>
where
    State: fetch_src_builder::State + fetch_src_builder::IsComplete,
{
    pub async fn fetch(self) -> Result<(), FetchSrcError> {
        let fetch = self._build();
        if let Err(err) = do_fetch_src(&fetch).await {
            let package = PackageSpec::new(
                fetch.rockspec.package.clone(),
                fetch.rockspec.version.clone(),
            );
            fetch.progress.map(|p| {
                p.println(format!(
                    "⚠️ WARNING: Failed to fetch source for {}: {}",
                    &package, err
                ))
            });
            fetch
                .progress
                .map(|p| p.println("⚠️ Falling back to .src.rock archive"));
            FetchSrcRock::new(&package, fetch.dest_dir, fetch.config, fetch.progress)
                .fetch()
                .await?;
        }
        Ok(())
    }
}

#[derive(Error, Debug)]
pub enum FetchSrcError {
    #[error("failed to clone rock source: {0}")]
    GitClone(#[from] git2::Error),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Request(#[from] reqwest::Error),
    #[error(transparent)]
    Unpack(#[from] UnpackError),
    #[error(transparent)]
    FetchSrcRock(#[from] FetchSrcRockError),
}

/// A rocks package source fetcher, providing fine-grained control
/// over how a package should be fetched.
#[derive(Builder)]
#[builder(start_fn = new, finish_fn(name = _build, vis = ""))]
struct FetchSrcRock<'a> {
    #[builder(start_fn)]
    package: &'a PackageSpec,
    #[builder(start_fn)]
    dest_dir: &'a Path,
    #[builder(start_fn)]
    config: &'a Config,
    #[builder(start_fn)]
    progress: &'a Progress<ProgressBar>,
}

impl<State> FetchSrcRockBuilder<'_, State>
where
    State: fetch_src_rock_builder::State + fetch_src_rock_builder::IsComplete,
{
    pub async fn fetch(self) -> Result<(), FetchSrcRockError> {
        do_fetch_src_rock(self._build()).await
    }
}

#[derive(Error, Debug)]
#[error(transparent)]
pub enum FetchSrcRockError {
    DownloadSrcRock(#[from] DownloadSrcRockError),
    Unpack(#[from] UnpackError),
}

async fn do_fetch_src(fetch: &FetchSrc<'_>) -> Result<(), FetchSrcError> {
    let rockspec = fetch.rockspec;
    let rock_source = rockspec.source.current_platform();
    let progress = fetch.progress;
    let dest_dir = fetch.dest_dir;
    match &rock_source.source_spec {
        RockSourceSpec::Git(git) => {
            let url = &git.url.to_string();
            progress.map(|p| p.set_message(format!("🦠 Cloning {}", url)));

            let mut fetch_options = FetchOptions::new();
            if git.checkout_ref.is_none() {
                fetch_options.depth(1);
            };
            let mut repo_builder = RepoBuilder::new();
            repo_builder.fetch_options(fetch_options);
            let repo = repo_builder.clone(url, dest_dir)?;

            if let Some(commit_hash) = &git.checkout_ref {
                let (object, _) = repo.revparse_ext(commit_hash)?;
                repo.checkout_tree(&object, None)?;
            }
        }
        RockSourceSpec::Url(url) => {
            progress.map(|p| p.set_message(format!("📥 Downloading {}", url.to_owned())));

            let response = reqwest::get(url.to_owned()).await?.bytes().await?;
            let file_name = url
                .path_segments()
                .and_then(|segments| segments.last())
                .and_then(|name| {
                    if name.is_empty() {
                        None
                    } else {
                        Some(name.to_string())
                    }
                })
                .unwrap_or(url.to_string());
            let cursor = Cursor::new(response);
            let mime_type = infer::get(cursor.get_ref()).map(|file_type| file_type.mime_type());
            unpack(
                mime_type,
                cursor,
                rock_source.unpack_dir.is_none(),
                file_name,
                dest_dir,
                progress,
            )
            .await?
        }
        RockSourceSpec::File(path) => {
            if path.is_dir() {
                progress.map(|p| p.set_message(format!("📋 Copying {}", path.display())));

                for file in walkdir::WalkDir::new(path).into_iter().flatten() {
                    if file.file_type().is_file() {
                        let filepath = file.path();
                        let relative_path = filepath.strip_prefix(path).unwrap();
                        let target = dest_dir.join(relative_path);
                        let parent = target.parent().unwrap();
                        std::fs::create_dir_all(parent)?;
                        std::fs::copy(filepath, target)?;
                    }
                }
            } else {
                let mut file = File::open(path)?;
                let mut buffer = Vec::new();
                file.read_to_end(&mut buffer)?;
                let mime_type = infer::get(&buffer).map(|file_type| file_type.mime_type());
                let file_name = path
                    .file_name()
                    .map(|os_str| os_str.to_string_lossy())
                    .unwrap_or(path.to_string_lossy())
                    .to_string();
                unpack(
                    mime_type,
                    file,
                    rock_source.unpack_dir.is_none(),
                    file_name,
                    dest_dir,
                    progress,
                )
                .await?
            }
        }
        RockSourceSpec::Cvs(_) => unimplemented!(),
        RockSourceSpec::Mercurial(_) => unimplemented!(),
        RockSourceSpec::Sscm(_) => unimplemented!(),
        RockSourceSpec::Svn(_) => unimplemented!(),
    }
    Ok(())
}

async fn do_fetch_src_rock(fetch: FetchSrcRock<'_>) -> Result<(), FetchSrcRockError> {
    let package = fetch.package;
    let dest_dir = fetch.dest_dir;
    let config = fetch.config;
    let progress = fetch.progress;
    let src_rock = operations::download_src_rock(package, config.server(), progress).await?;
    let cursor = Cursor::new(src_rock.bytes);
    let mime_type = infer::get(cursor.get_ref()).map(|file_type| file_type.mime_type());
    unpack(
        mime_type,
        cursor,
        false,
        src_rock.file_name,
        dest_dir,
        progress,
    )
    .await?;
    Ok(())
}

fn is_single_directory<R: Read + Seek + Send>(reader: R) -> io::Result<bool> {
    let tar = GzDecoder::new(reader);
    let mut archive = tar::Archive::new(tar);

    let entries: Vec<_> = archive
        .entries()?
        .filter_map(|entry| {
            if entry.as_ref().ok()?.path().ok()?.file_name()? != "pax_global_header" {
                Some(entry)
            } else {
                None
            }
        })
        .try_collect()?;

    let directory: PathBuf = entries
        .first()
        .unwrap()
        .path()?
        .components()
        .take(1)
        .collect();

    Ok(entries.into_iter().all(|entry| {
        entry
            .path()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with(directory.to_str().unwrap())
    }))
}

#[derive(Error, Debug)]
pub enum UnpackError {
    #[error(transparent)]
    Zip(#[from] zip::result::ZipError),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("source returned HTML - it may have been moved or deleted")]
    SourceMovedOrDeleted,
    #[error("rockspec source has unsupported file type {0}")]
    UnsupportedFileType(String),
    #[error("could not determine mimetype of rockspec source")]
    UnknownMimeType,
}

async fn unpack<R: Read + Seek + Send>(
    mime_type: Option<&str>,
    reader: R,
    auto_find_lua_sources: bool,
    file_name: String,
    dest_dir: &Path,
    progress: &Progress<ProgressBar>,
) -> Result<(), UnpackError> {
    progress.map(|p| p.set_message(format!("📦 Unpacking {}", file_name)));

    match mime_type {
        Some("application/zip") => {
            let mut archive = zip::ZipArchive::new(reader)?;
            archive.extract(dest_dir)?;
        }
        Some("application/x-tar") => {
            let mut archive = tar::Archive::new(reader);
            archive.unpack(dest_dir)?;
        }
        Some("application/gzip") => {
            let mut bufreader = BufReader::new(reader);

            let extract_subdirectory =
                auto_find_lua_sources && is_single_directory(&mut bufreader)?;

            bufreader.rewind()?;
            let tar = GzDecoder::new(bufreader);
            let mut archive = tar::Archive::new(tar);

            if extract_subdirectory {
                archive.entries()?.try_for_each(|entry| {
                    let mut entry = entry?;

                    let path: PathBuf = entry.path()?.components().skip(1).collect();
                    if path.components().count() > 0 {
                        let dest = dest_dir.join(path);
                        std::fs::create_dir_all(dest.parent().unwrap())?;
                        entry.unpack(dest)?;
                    }

                    Ok::<_, io::Error>(())
                })?;
            } else {
                archive.entries()?.try_for_each(|entry| {
                    entry?.unpack_in(dest_dir)?;
                    Ok::<_, io::Error>(())
                })?;
            }
        }
        Some("text/html") => {
            return Err(UnpackError::SourceMovedOrDeleted);
        }
        Some(other) => {
            return Err(UnpackError::UnsupportedFileType(other.to_string()));
        }
        None => {
            return Err(UnpackError::UnknownMimeType);
        }
    }

    Ok(())
}
