use crate::{config::Config, lua_package::LuaPackage, tree::Tree};
use eyre::Result;

pub fn remove(package: LuaPackage, config: &Config) -> Result<()> {
    let tree = Tree::new(
        config.tree().clone(),
        config.lua_version().cloned().unwrap(),
    )?;

    let package = tree.has_rock(&package.as_package_req()).unwrap();

    std::fs::remove_dir_all(tree.root_for(package.name(), package.version()))?;

    Ok(())
}
