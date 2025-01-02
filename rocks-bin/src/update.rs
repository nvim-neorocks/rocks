use clap::Args;
use eyre::Result;
use rocks_lib::config::LuaVersion;
use rocks_lib::lockfile::PinnedState;
use rocks_lib::progress::{MultiProgress, ProgressBar};
use rocks_lib::{config::Config, operations, tree::Tree};

#[derive(Args)]
pub struct Update {}

pub async fn update(config: Config) -> Result<()> {
    let progress = MultiProgress::new_arc();
    progress.map(|p| p.add(ProgressBar::from("🔎 Looking for updates...".to_string())));

    let tree = Tree::new(config.tree().clone(), LuaVersion::from(&config)?)?;
    let lockfile = tree.lockfile()?;

    operations::Update::new(&config)
        .packages(
            lockfile
                .rocks()
                .values()
                .filter(|package| package.pinned() == PinnedState::Unpinned)
                .map(|package| (package.clone(), package.to_package().into_package_req())),
        )
        .progress(progress)
        .update()
        .await?;

    Ok(())
}
