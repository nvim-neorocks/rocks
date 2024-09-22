use eyre::Result;
use inquire::Confirm;
use rocks_lib::{config::Config, tree::Tree};

pub fn purge(config: Config) -> Result<()> {
    let tree = Tree::new(
        config.tree().clone(),
        config.lua_version().cloned().unwrap(),
    )?;

    let len = tree.list().len();

    if Confirm::new(&format!("Are you sure you want to purge all {len} rocks?"))
        .with_default(false)
        .prompt()?
    {
        std::fs::remove_dir_all(tree.root())?;
    }

    Ok(())
}
