use crate::lockfile::LocalPackage;
use crate::{config::Config, progress::with_spinner, tree::Tree};
use eyre::Result;
use indicatif::MultiProgress;

// TODO: Remove dependencies recursively too!
pub async fn remove(progress: &MultiProgress, package: LocalPackage, config: &Config) -> Result<()> {
    with_spinner(
        progress,
        format!("🗑️ Removing {}@{}", package.name, package.version),
        || async { remove_impl(package, config).await },
    )
    .await
}

async fn remove_impl(package: LocalPackage, config: &Config) -> Result<()> {
    let tree = Tree::new(
        config.tree().clone(),
        config.lua_version().cloned().unwrap(),
    )?;

    tree.lockfile()?.remove(&package);

    std::fs::remove_dir_all(tree.root_for(&package))?;

    Ok(())
}
