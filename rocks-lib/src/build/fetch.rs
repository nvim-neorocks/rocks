use eyre::eyre;
use eyre::Result;
use flate2::read::GzDecoder;
use git2::Repository;
use std::io::Cursor;
use std::path::Path;

use crate::rockspec::RockSource;
use crate::rockspec::RockSourceSpec;

pub async fn fetch_src(dest_dir: &Path, rock_source: &RockSource) -> Result<()> {
    match &rock_source.source_spec {
        RockSourceSpec::Git(git) => {
            let repo = Repository::clone(&git.url.to_string(), dest_dir)?;

            if let Some(commit_hash) = &git.checkout_ref {
                let (object, _) = repo.revparse_ext(commit_hash)?;
                repo.checkout_tree(&object, None)?;
            }
        }
        RockSourceSpec::Url(url) => {
            let response = reqwest::get(url.to_owned()).await?.bytes().await?;
            let cursor = Cursor::new(response);
            let file_type = infer::get(cursor.get_ref()).unwrap();
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
        }
        RockSourceSpec::File(_) => unimplemented!(),
        RockSourceSpec::Cvs(_) => unimplemented!(),
        RockSourceSpec::Mercurial(_) => unimplemented!(),
        RockSourceSpec::Sscm(_) => unimplemented!(),
        RockSourceSpec::Svn(_) => unimplemented!(),
    }
    Ok(())
}
