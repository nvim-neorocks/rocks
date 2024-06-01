use crate::{
    config::Config,
    rockspec::{Build, BuildBackendSpec, RockSourceSpec, Rockspec},
};
use eyre::Result;
use git2::Repository;

pub fn build(rockspec: Rockspec, config: &Config, no_install: bool) -> Result<()> {
    // TODO(vhyrro): Use a more serious isolation strategy here.
    let temp_dir = tempdir::TempDir::new(&rockspec.package)?;

    let previous_dir = std::env::current_dir()?;

    // Install the source in order to build.
    match &rockspec.source.current_platform().source_spec {
        RockSourceSpec::Git(git) => {
            let repo = Repository::clone(&git.url.to_string(), &temp_dir)?;

            if let Some(commit_hash) = &git.checkout_ref {
                let (object, _) = repo.revparse_ext(commit_hash)?;
                repo.checkout_tree(&object, None)?;
            }
        }
        RockSourceSpec::Url(_) => todo!(),
        RockSourceSpec::File(_) => todo!(),
        RockSourceSpec::Cvs(_) => unimplemented!(),
        RockSourceSpec::Mercurial(_) => unimplemented!(),
        RockSourceSpec::Sscm(_) => unimplemented!(),
        RockSourceSpec::Svn(_) => unimplemented!(),
    };

    std::env::set_current_dir(&temp_dir)?;

    // TODO: Ensure dependencies and build dependencies.
    match rockspec.build.default.build_backend.as_ref().cloned() {
        Some(BuildBackendSpec::Builtin(spec)) => spec.run(rockspec, config, no_install)?,
        _ => unimplemented!(),
    };

    std::env::set_current_dir(previous_dir)?;

    Ok(())
}
