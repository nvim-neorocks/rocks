use eyre::{Context, OptionExt};
use itertools::Itertools;
use std::sync::Arc;

use clap::Args;
use eyre::Result;
use rocks_lib::{
    build::{self, BuildBehaviour},
    config::Config,
    lockfile::PinnedState,
    operations::Install,
    package::PackageName,
    progress::MultiProgress,
    project::Project,
    remote_package_db::RemotePackageDB,
    rockspec::{LuaVersionCompatibility, Rockspec},
};

#[derive(Args, Default)]
pub struct Build {
    /// Whether to pin the dependencies.
    #[arg(long)]
    pin: bool,

    /// Ignore the project's existing lockfile.
    #[arg(long)]
    ignore_lockfile: bool,

    /// Do not create a lockfile.
    #[arg(long)]
    no_lock: bool,
}

pub async fn build(data: Build, config: Config) -> Result<()> {
    let project = Project::current()?.ok_or_eyre("Not in a project!")?;
    let pin = PinnedState::from(data.pin);
    let progress_arc = MultiProgress::new_arc();
    let progress = Arc::clone(&progress_arc);

    let bar = progress.map(|p| p.new_bar());
    let package_db = match project.lockfile()? {
        None => RemotePackageDB::from_config(&config, &bar).await?,
        Some(_) if data.ignore_lockfile => RemotePackageDB::from_config(&config, &bar).await?,
        Some(lockfile) => lockfile.into(),
    };

    bar.map(|b| b.finish_and_clear());
    let rocks = project.new_local_rockspec()?;
    let lua_version = rocks.lua_version_matches(&config)?;
    let tree = project.tree(lua_version)?;

    // Ensure all dependencies are installed first
    let dependencies = rocks
        .dependencies()
        .current_platform()
        .iter()
        .filter(|package| !package.name().eq(&PackageName::new("lua".into())))
        .collect_vec();

    let dependencies_to_install = dependencies
        .into_iter()
        .filter(|req| {
            tree.match_rocks(req)
                .is_ok_and(|rock_match| !rock_match.is_found())
        })
        .map(|dep| (BuildBehaviour::NoForce, dep.to_owned()));

    Install::new(&config)
        .packages(dependencies_to_install)
        .pin(pin)
        .package_db(package_db)
        .progress(progress_arc)
        .install()
        .await?;

    if !data.no_lock {
        std::fs::copy(tree.lockfile_path(), project.lockfile_path())
            .wrap_err("error copying the project lockfile")?;
    }

    build::Build::new(&rocks, &config, &progress.map(|p| p.new_bar()))
        .pin(pin)
        .behaviour(BuildBehaviour::Force)
        .build()
        .await?;

    Ok(())
}
