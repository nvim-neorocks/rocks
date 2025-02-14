use clap::Args;
use eyre::Result;
use lux_lib::config::LuaVersion;
use lux_lib::lockfile::PinnedState;
use lux_lib::progress::{MultiProgress, ProgressBar};
use lux_lib::{config::Config, operations};

#[derive(Args)]
pub struct Update {}

pub async fn update(config: Config) -> Result<()> {
    let progress = MultiProgress::new_arc();
    progress.map(|p| p.add(ProgressBar::from("🔎 Looking for updates...".to_string())));

    let tree = config.tree(LuaVersion::from(&config)?)?;
    let lockfile = tree.lockfile()?;

    operations::Update::new(&tree, &config)
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
