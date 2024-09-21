use eyre::Result;
use rocks_lib::{config::Config, tree::Tree};

pub fn purge(config: Config) -> Result<()> {
    let tree = Tree::new(
        config.tree().clone(),
        config.lua_version().cloned().unwrap(),
    )?;

    std::fs::remove_dir_all(tree.root())?;

    Ok(())
}
