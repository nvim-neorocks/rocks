use eyre::eyre;
use eyre::Result;
use flate2::read::GzDecoder;
use git2::build::RepoBuilder;
use git2::FetchOptions;
use indicatif::MultiProgress;
use itertools::Itertools;
use std::fs::File;
use std::io::BufReader;
use std::io::Cursor;
use std::io::Read;
use std::io::Seek;
use std::path::Path;
use std::path::PathBuf;

use crate::config::Config;
use crate::operations;
use crate::package::RemotePackage;
use crate::{progress::with_spinner, rockspec::RockSource, rockspec::RockSourceSpec};

pub async fn fetch_src(
    progress: &MultiProgress,
    dest_dir: &Path,
    rock_source: &RockSource,
) -> Result<()> {
    match &rock_source.source_spec {
        RockSourceSpec::Git(git) => {
            let url = &git.url.to_string();
            let repo = with_spinner(progress, format!("🦠 Cloning {}", url), || async {
                let mut fetch_options = FetchOptions::new();
                if git.checkout_ref.is_none() {
                    fetch_options.depth(1);
                };
                let mut repo_builder = RepoBuilder::new();
                repo_builder.fetch_options(fetch_options);
                Ok(repo_builder.clone(url, dest_dir)?)
            })
            .await?;

            if let Some(commit_hash) = &git.checkout_ref {
                let (object, _) = repo.revparse_ext(commit_hash)?;
                repo.checkout_tree(&object, None)?;
            }
        }
        RockSourceSpec::Url(url) => {
            let response = with_spinner(
                progress,
                format!("📥 Downloading {}", url.to_owned()),
                || async { Ok(reqwest::get(url.to_owned()).await?.bytes().await?) },
            )
            .await?;
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
                progress,
                mime_type,
                cursor,
                rock_source.unpack_dir.is_none(),
                file_name,
                dest_dir,
            )
            .await?
        }
        RockSourceSpec::File(path) => {
            if path.is_dir() {
                with_spinner(
                    progress,
                    format!("📋 Copying {}", path.display()),
                    || async {
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
                        Ok(())
                    },
                )
                .await?;
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
                    progress,
                    mime_type,
                    file,
                    rock_source.unpack_dir.is_none(),
                    file_name,
                    dest_dir,
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

pub async fn fetch_src_rock(
    progress: &MultiProgress,
    package: &RemotePackage,
    dest_dir: &Path,
    config: &Config,
) -> Result<()> {
    let src_rock = operations::download_src_rock(progress, package, config).await?;
    let cursor = Cursor::new(src_rock.bytes);
    let mime_type = infer::get(cursor.get_ref()).map(|file_type| file_type.mime_type());
    unpack(
        progress,
        mime_type,
        cursor,
        false,
        src_rock.file_name,
        dest_dir,
    )
    .await?;
    Ok(())
}

fn is_single_directory<R: Read + Seek + Send>(reader: R) -> Result<bool> {
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

async fn unpack<R: Read + Seek + Send>(
    progress: &MultiProgress,
    mime_type: Option<&str>,
    reader: R,
    auto_find_lua_sources: bool,
    file_name: String,
    dest_dir: &Path,
) -> Result<()> {
    with_spinner(progress, format!("📦 Unpacking {}", file_name), || async {
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

                        Ok::<_, eyre::Report>(())
                    })?;
                } else {
                    archive.entries()?.try_for_each(|entry| {
                        entry?.unpack_in(dest_dir)?;
                        Ok::<_, eyre::Report>(())
                    })?;
                }
            }
            Some(other) => {
                return Err(eyre!("Rockspec source has unsupported file type {}", other));
            }
            None => {
                return Err(eyre!("Could not determine mimetype of rockspec source."));
            }
        }
        Ok(())
    })
    .await
}
