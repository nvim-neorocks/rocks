use bon::Builder;
use git2::build::RepoBuilder;
use git2::FetchOptions;
use ssri::Integrity;
use std::fs::File;
use std::io;
use std::io::Cursor;
use std::io::Read;
use std::path::Path;
use thiserror::Error;

use crate::config::Config;
use crate::hash::HasIntegrity;
use crate::lua_rockspec::RockSourceSpec;
use crate::operations;
use crate::package::PackageSpec;
use crate::progress::Progress;
use crate::progress::ProgressBar;
use crate::rockspec::Rockspec;

use super::DownloadSrcRockError;
use super::UnpackError;

/// A rocks package source fetcher, providing fine-grained control
/// over how a package should be fetched.
#[derive(Builder)]
#[builder(start_fn = new, finish_fn(name = _build, vis = ""))]
pub struct FetchSrc<'a, R: Rockspec> {
    #[builder(start_fn)]
    dest_dir: &'a Path,
    #[builder(start_fn)]
    rockspec: &'a R,
    #[builder(start_fn)]
    config: &'a Config,
    #[builder(start_fn)]
    progress: &'a Progress<ProgressBar>,
}

impl<R: Rockspec, State> FetchSrcBuilder<'_, R, State>
where
    State: fetch_src_builder::State + fetch_src_builder::IsComplete,
{
    /// Fetch and unpack the source into the `dest_dir`,
    /// returning the source `Integrity`.
    pub async fn fetch(self) -> Result<Integrity, FetchSrcError> {
        let fetch = self._build();
        match do_fetch_src(&fetch).await {
            Err(err) => {
                let package = PackageSpec::new(
                    fetch.rockspec.package().clone(),
                    fetch.rockspec.version().clone(),
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
                let integrity =
                    FetchSrcRock::new(&package, fetch.dest_dir, fetch.config, fetch.progress)
                        .fetch()
                        .await?;
                Ok(integrity)
            }
            Ok(integrity) => Ok(integrity),
        }
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
    pub async fn fetch(self) -> Result<Integrity, FetchSrcRockError> {
        do_fetch_src_rock(self._build()).await
    }
}

#[derive(Error, Debug)]
#[error(transparent)]
pub enum FetchSrcRockError {
    DownloadSrcRock(#[from] DownloadSrcRockError),
    Unpack(#[from] UnpackError),
    Io(#[from] io::Error),
}

async fn do_fetch_src<R: Rockspec>(fetch: &FetchSrc<'_, R>) -> Result<Integrity, FetchSrcError> {
    let rockspec = fetch.rockspec;
    let rock_source = rockspec.source().current_platform();
    let progress = fetch.progress;
    let dest_dir = fetch.dest_dir;
    let integrity = match &rock_source.source_spec {
        RockSourceSpec::Git(git) => {
            let url = &git.url.to_string();
            progress.map(|p| p.set_message(format!("🦠 Cloning {}", url)));

            let mut fetch_options = FetchOptions::new();
            fetch_options.update_fetchhead(false);
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
            repo.hash()?
        }
        RockSourceSpec::Url(url) => {
            progress.map(|p| p.set_message(format!("📥 Downloading {}", url.to_owned())));

            let response = reqwest::get(url.to_owned()).await?.bytes().await?;
            let hash = response.hash()?;
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
            operations::unpack::unpack(
                mime_type,
                cursor,
                rock_source.unpack_dir.is_none(),
                file_name,
                dest_dir,
                progress,
            )
            .await?;
            hash
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
                operations::unpack::unpack(
                    mime_type,
                    file,
                    rock_source.unpack_dir.is_none(),
                    file_name,
                    dest_dir,
                    progress,
                )
                .await?
            }
            path.hash()?
        }
        RockSourceSpec::Cvs(_) => unimplemented!(),
        RockSourceSpec::Mercurial(_) => unimplemented!(),
        RockSourceSpec::Sscm(_) => unimplemented!(),
        RockSourceSpec::Svn(_) => unimplemented!(),
    };
    Ok(integrity)
}

async fn do_fetch_src_rock(fetch: FetchSrcRock<'_>) -> Result<Integrity, FetchSrcRockError> {
    let package = fetch.package;
    let dest_dir = fetch.dest_dir;
    let config = fetch.config;
    let progress = fetch.progress;
    let src_rock = operations::download_src_rock(package, config.server(), progress).await?;
    let integrity = src_rock.bytes.hash()?;
    let cursor = Cursor::new(src_rock.bytes);
    let mime_type = infer::get(cursor.get_ref()).map(|file_type| file_type.mime_type());
    operations::unpack::unpack(
        mime_type,
        cursor,
        true,
        src_rock.file_name,
        dest_dir,
        progress,
    )
    .await?;
    Ok(integrity)
}
