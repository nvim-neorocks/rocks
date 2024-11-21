use std::io;

use crate::config::{LuaVersion, LuaVersionUnset};
use crate::lockfile::LocalPackage;
use crate::progress::ProgressBar;
use crate::{config::Config, tree::Tree};
use thiserror::Error;

#[derive(Error, Debug)]
#[error(transparent)]
pub enum RemoveError {
    LuaVersionUnset(#[from] LuaVersionUnset),
    Io(#[from] io::Error),
}

// TODO: Remove dependencies recursively too!
pub async fn remove(
    progress: &ProgressBar,
    package: LocalPackage,
    config: &Config,
) -> Result<(), RemoveError> {
    progress.set_message(format!(
        "🗑️ Removing {}@{}",
        package.name(),
        package.version()
    ));

    remove_impl(package, config).await
}

async fn remove_impl(package: LocalPackage, config: &Config) -> Result<(), RemoveError> {
    let tree = Tree::new(config.tree().clone(), LuaVersion::from(config)?)?;

    tree.lockfile()?.remove(&package);

    std::fs::remove_dir_all(tree.root_for(&package))?;

    Ok(())
}
