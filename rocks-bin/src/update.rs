use clap::Args;
use eyre::Result;
use rocks_lib::config::LuaVersion;
use rocks_lib::lockfile::PinnedState;
use rocks_lib::progress::{MultiProgress, ProgressBar};
use rocks_lib::remote_package_db::RemotePackageDB;
use rocks_lib::{config::Config, operations, package::PackageReq, tree::Tree};

#[derive(Args)]
pub struct Update {}

pub async fn update(config: Config) -> Result<()> {
    let progress = MultiProgress::new_arc();
    progress.map(|p| p.add(ProgressBar::from("🔎 Looking for updates...".to_string())));

    let tree = Tree::new(config.tree().clone(), LuaVersion::from(&config)?)?;

    let lockfile = tree.lockfile()?;
    let rocks = lockfile.rocks();
    let package_db = RemotePackageDB::from_config(&config).await?;

    for package in rocks.values() {
        if package.pinned() == PinnedState::Unpinned {
            operations::update(
                package.clone(),
                PackageReq::new(
                    package.name().to_string(),
                    package.constraint().to_string_opt(),
                )?,
                &package_db,
                &config,
                progress.clone(),
            )
            .await?;
        }
    }

    Ok(())
}
