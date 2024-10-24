use std::io;

use crate::config::{LuaVersion, LuaVersionUnset};
use crate::lockfile::LocalPackage;
use crate::{config::Config, progress::with_spinner, tree::Tree};
use indicatif::MultiProgress;
use thiserror::Error;

#[derive(Error, Debug)]
#[error(transparent)]
pub enum RemoveError {
    LuaVersionUnset(#[from] LuaVersionUnset),
    Io(#[from] io::Error),
}

// TODO: Remove dependencies recursively too!
pub async fn remove(
    progress: &MultiProgress,
    package: LocalPackage,
    config: &Config,
) -> Result<(), RemoveError> {
    with_spinner(
        progress,
        format!("🗑️ Removing {}@{}", package.name, package.version),
        || async { remove_impl(package, config).await },
    )
    .await
}

async fn remove_impl(package: LocalPackage, config: &Config) -> Result<(), RemoveError> {
    let tree = Tree::new(config.tree().clone(), LuaVersion::from(config)?)?;

    tree.lockfile()?.remove(&package);

    std::fs::remove_dir_all(tree.root_for(&package))?;

    Ok(())
}
