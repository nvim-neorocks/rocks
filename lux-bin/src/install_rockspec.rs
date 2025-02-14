use eyre::{eyre, Context};
use itertools::Itertools;
use std::{path::PathBuf, sync::Arc};

use clap::Args;
use eyre::Result;
use lux_lib::{
    build::{self, BuildBehaviour},
    config::Config,
    lockfile::PinnedState,
    lua_rockspec::LuaRockspec,
    operations::Install,
    package::PackageName,
    progress::MultiProgress,
    project::Project,
    rockspec::{LuaVersionCompatibility, Rockspec},
};

#[derive(Args, Default)]
pub struct InstallRockspec {
    /// The path to the RockSpec file to install
    rockspec_path: PathBuf,

    /// Whether to pin the installed package and dependencies.
    #[arg(long)]
    pin: bool,
}

pub async fn install_rockspec(data: InstallRockspec, config: Config) -> Result<()> {
    let pin = PinnedState::from(data.pin);
    let project_opt = Project::current()?;
    let path = data.rockspec_path;

    if path
        .extension()
        .map(|ext| ext != "rockspec")
        .unwrap_or(true)
    {
        return Err(eyre!("Provided path is not a valid rockspec!"));
    }
    let content = std::fs::read_to_string(path)?;
    let rockspec = LuaRockspec::new(&content)?;
    let lua_version = rockspec.lua_version_matches(&config)?;
    let tree = config.tree(lua_version)?;

    // Ensure all dependencies are installed first
    let dependencies = rockspec
        .dependencies()
        .current_platform()
        .iter()
        .filter(|package| !package.name().eq(&PackageName::new("lua".into())))
        .collect_vec();

    let progress_arc = MultiProgress::new_arc();
    let progress = Arc::clone(&progress_arc);

    let dependencies_to_install = dependencies
        .into_iter()
        .filter(|req| {
            tree.match_rocks(req)
                .is_ok_and(|rock_match| rock_match.is_found())
        })
        .map(|dep| (BuildBehaviour::NoForce, dep.to_owned()));

    Install::new(&tree, &config)
        .packages(dependencies_to_install)
        .pin(pin)
        .progress(progress_arc)
        .install()
        .await?;

    if let Some(project) = project_opt {
        std::fs::copy(tree.lockfile_path(), project.lockfile_path())
            .wrap_err("error creating project lockfile.")?;
    }

    build::Build::new(&rockspec, &tree, &config, &progress.map(|p| p.new_bar()))
        .pin(pin)
        .behaviour(BuildBehaviour::Force)
        .build()
        .await?;

    Ok(())
}
