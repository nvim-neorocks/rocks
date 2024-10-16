use eyre::eyre;
use eyre::Result;
use flate2::read::GzDecoder;
use git2::build::RepoBuilder;
use git2::FetchOptions;
use indicatif::MultiProgress;
use std::fs::File;
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
            unpack(progress, mime_type, cursor, file_name, dest_dir).await?
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
                unpack(progress, mime_type, file, file_name, dest_dir).await?
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
    unpack(progress, mime_type, cursor, src_rock.file_name, dest_dir).await?;
    Ok(())
}

async fn unpack<R: Read + Seek + Send>(
    progress: &MultiProgress,
    mime_type: Option<&str>,
    reader: R,
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
                let tar = GzDecoder::new(reader);
                let mut archive = tar::Archive::new(tar);

                if archive.entries()?.count() == 1 {
                    archive.entries()?.try_for_each(|entry| {
                        let mut entry = entry?;
                        entry.unpack(
                            dest_dir.join(entry.path()?.components().skip(1).collect::<PathBuf>()),
                        )?;
                        Ok::<_, eyre::Report>(())
                    })?;
                } else {
                    archive.unpack(dest_dir)?;
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
