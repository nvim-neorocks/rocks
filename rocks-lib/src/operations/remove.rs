use crate::lockfile::LockedPackage;
use crate::{config::Config, lua_package::LuaPackage, progress::with_spinner, tree::Tree};
use eyre::Result;
use indicatif::MultiProgress;

pub async fn remove(progress: &MultiProgress, package: LuaPackage, config: &Config) -> Result<()> {
    with_spinner(progress, format!("🗑️ Removing {}", package), || async {
        remove_impl(package, config).await
    })
    .await
}

async fn remove_impl(package: LuaPackage, config: &Config) -> Result<()> {
    let tree = Tree::new(
        config.tree().clone(),
        config.lua_version().cloned().unwrap(),
    )?;

    let package = tree.has_rock(&package.as_package_req()).unwrap();

    tree.lockfile()?.remove(&LockedPackage::from(&package));

    std::fs::remove_dir_all(tree.root_for(package.name(), package.version()))?;

    Ok(())
}
