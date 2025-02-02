use eyre::{Context, OptionExt};
use itertools::Itertools;
use std::sync::Arc;

use clap::Args;
use eyre::Result;
use rocks_lib::{
    build::{self, BuildBehaviour},
    config::Config,
    lockfile::PinnedState,
    operations::{Install, Sync},
    package::PackageName,
    progress::MultiProgress,
    project::Project,
    rockspec::Rockspec,
};

#[derive(Args, Default)]
pub struct Build {
    /// Whether to pin the dependencies.
    #[arg(long)]
    pin: bool,

    /// Ignore the project's existing lockfile.
    #[arg(long)]
    ignore_lockfile: bool,
}

pub async fn build(data: Build, config: Config) -> Result<()> {
    let project = Project::current()?.ok_or_eyre("Not in a project!")?;
    let pin = PinnedState::from(data.pin);
    let progress_arc = MultiProgress::new_arc();
    let progress = Arc::clone(&progress_arc);

    let tree = project.tree(&config)?;
    let rocks = project.new_local_rockspec()?;

    let lockfile = match project.try_lockfile()? {
        None => None,
        Some(_) if data.ignore_lockfile => None,
        Some(lockfile) => Some(lockfile),
    };

    let dependencies = rocks
        .dependencies()
        .current_platform()
        .iter()
        .filter(|package| !package.name().eq(&PackageName::new("lua".into())))
        .cloned()
        .collect_vec();

    match lockfile {
        Some(mut project_lockfile) => {
            Sync::new(&tree, &mut project_lockfile, &config)
                .progress(progress.clone())
                .packages(dependencies)
                .sync()
                .await
                .wrap_err(
                    "
syncing with the project lockfile failed.
Use --ignore-lockfile to force a new build.
",
                )?;
        }
        None => {
            let dependencies_to_install = dependencies
                .into_iter()
                .filter(|req| {
                    tree.match_rocks(req)
                        .is_ok_and(|rock_match| !rock_match.is_found())
                })
                .map(|dep| (BuildBehaviour::NoForce, dep));

            Install::new(&tree, &config)
                .packages(dependencies_to_install)
                .project(&project)
                .pin(pin)
                .progress(progress.clone())
                .install()
                .await?;
        }
    }

    build::Build::new(&rocks, &tree, &config, &progress.map(|p| p.new_bar()))
        .pin(pin)
        .behaviour(BuildBehaviour::Force)
        .build()
        .await?;

    Ok(())
}
