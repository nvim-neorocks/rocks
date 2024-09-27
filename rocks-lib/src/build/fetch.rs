use git2::Repository;
use eyre::Result;

use crate::rockspec::RockSourceSpec;

pub async fn fetch_src(temp_dir: tempdir::TempDir, source_spec: &RockSourceSpec) -> Result<()> {
    match source_spec {
        RockSourceSpec::Git(git) => {
            let repo = Repository::clone(&git.url.to_string(), &temp_dir)?;

            if let Some(commit_hash) = &git.checkout_ref {
                let (object, _) = repo.revparse_ext(commit_hash)?;
                repo.checkout_tree(&object, None)?;
            }
        }
        RockSourceSpec::Url(_) => unimplemented!(),
        RockSourceSpec::File(_) => unimplemented!(),
        RockSourceSpec::Cvs(_) => unimplemented!(),
        RockSourceSpec::Mercurial(_) => unimplemented!(),
        RockSourceSpec::Sscm(_) => unimplemented!(),
        RockSourceSpec::Svn(_) => unimplemented!(),
    }
    Ok(())
}
