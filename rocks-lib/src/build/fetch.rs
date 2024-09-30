use eyre::eyre;
use eyre::Result;
use flate2::read::GzDecoder;
use git2::Repository;
use indicatif::MultiProgress;
use std::io::Cursor;
use std::path::Path;

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
                Ok(Repository::clone(url, dest_dir)?)
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
            let file_type = infer::get(cursor.get_ref()).unwrap();
            with_spinner(progress, format!("📦 Unpacking {}", file_name), || async {
                match file_type.mime_type() {
                    "application/zip" => {
                        let mut archive = zip::ZipArchive::new(cursor)?;
                        archive.extract(dest_dir)?;
                    }
                    "application/x-tar" => {
                        let mut archive = tar::Archive::new(cursor);
                        archive.unpack(dest_dir)?;
                    }
                    "application/gzip" => {
                        let tar = GzDecoder::new(cursor);
                        let mut archive = tar::Archive::new(tar);
                        archive.unpack(dest_dir)?;
                    }
                    other => {
                        return Err(eyre!("Rockspec source has unsupported file type {}", other));
                    }
                }
                Ok(())
            })
            .await?
        }
        RockSourceSpec::File(_) => unimplemented!(),
        RockSourceSpec::Cvs(_) => unimplemented!(),
        RockSourceSpec::Mercurial(_) => unimplemented!(),
        RockSourceSpec::Sscm(_) => unimplemented!(),
        RockSourceSpec::Svn(_) => unimplemented!(),
    }
    Ok(())
}
