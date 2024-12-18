use std::io;

use crate::config::{LuaVersion, LuaVersionUnset};
use crate::lockfile::LocalPackage;
use crate::progress::{MultiProgress, Progress, ProgressBar};
use crate::{config::Config, tree::Tree};
use futures::future::join_all;
use thiserror::Error;

#[derive(Error, Debug)]
#[error(transparent)]
pub enum RemoveError {
    LuaVersionUnset(#[from] LuaVersionUnset),
    Io(#[from] io::Error),
}

// TODO: Remove dependencies recursively too!
pub async fn remove(
    packages: Vec<LocalPackage>,
    config: &Config,
    progress: &Progress<MultiProgress>,
) -> Result<(), RemoveError> {
    join_all(packages.into_iter().map(|package| {
        let bar = progress.map(|p| {
            p.add(ProgressBar::from(format!(
                "🗑️ Removing {}@{}",
                package.name(),
                package.version()
            )))
        });

        let config = config.clone();

        tokio::spawn(remove_package(package, bar, config))
    }))
    .await;

    Ok(())
}

async fn remove_package(
    package: LocalPackage,
    _bar: Progress<ProgressBar>,
    config: Config,
) -> Result<(), RemoveError> {
    let tree = Tree::new(config.tree().clone(), LuaVersion::from(&config)?)?;

    tree.lockfile()?.remove(&package);

    tokio::fs::remove_dir_all(tree.root_for(&package)).await?;

    Ok(())
}
