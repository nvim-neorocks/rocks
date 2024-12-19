use clap::Args;
use eyre::Result;
use itertools::Itertools;
use rocks_lib::config::LuaVersion;
use rocks_lib::lockfile::PinnedState;
use rocks_lib::progress::{MultiProgress, Progress, ProgressBar};
use rocks_lib::{config::Config, manifest::ManifestMetadata, operations, tree::Tree};

#[derive(Args)]
pub struct Update {}

pub async fn update(config: Config) -> Result<()> {
    let progress = Progress::Progress(MultiProgress::new());
    let _bar = progress.map(|p| p.add(ProgressBar::from("🔎 Looking for updates...".to_string())));

    let tree = Tree::new(config.tree().clone(), LuaVersion::from(&config)?)?;

    let lockfile = tree.lockfile()?;
    let rocks = lockfile.rocks();
    let manifest = ManifestMetadata::from_config(&config).await?;

    let rocks = rocks
        .values()
        .filter(|package| package.pinned() == PinnedState::Unpinned)
        .map(|package| (package.clone(), package.to_package().into_package_req()))
        .collect_vec();

    operations::update(rocks, &manifest, &config, &progress).await?;

    Ok(())
}
