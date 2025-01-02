use clap::Args;
use eyre::Result;
use itertools::Itertools;
use rocks_lib::config::LuaVersion;
use rocks_lib::lockfile::PinnedState;
use rocks_lib::progress::{MultiProgress, ProgressBar};
use rocks_lib::{config::Config, operations, remote_package_db::RemotePackageDB, tree::Tree};

#[derive(Args)]
pub struct Update {}

pub async fn update(config: Config) -> Result<()> {
    let progress = MultiProgress::new_arc();
    let _bar = progress.map(|p| p.add(ProgressBar::from("🔎 Looking for updates...".to_string())));

    let tree = Tree::new(config.tree().clone(), LuaVersion::from(&config)?)?;

    let lockfile = tree.lockfile()?;
    let rocks = lockfile.rocks();
    let package_db = RemotePackageDB::from_config(&config).await?;

    let rocks = rocks
        .values()
        .filter(|package| package.pinned() == PinnedState::Unpinned)
        .map(|package| (package.clone(), package.to_package().into_package_req()))
        .collect_vec();

    operations::update(rocks, &package_db, &config, progress).await?;

    Ok(())
}
