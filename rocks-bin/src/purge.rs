use eyre::Result;
use indicatif::MultiProgress;
use inquire::Confirm;
use rocks_lib::{config::Config, progress::with_spinner, tree::Tree};

pub async fn purge(config: Config) -> Result<()> {
    let tree = Tree::new(
        config.tree().clone(),
        config.lua_version().cloned().unwrap(),
    )?;

    let len = tree.list()?.len();

    if Confirm::new(&format!("Are you sure you want to purge all {len} rocks?"))
        .with_default(false)
        .prompt()?
    {
        let root_dir = tree.root();
        with_spinner(
            &MultiProgress::new(),
            format!("🗑️ Purging {}", root_dir.display()),
            || async {
                std::fs::remove_dir_all(tree.root())?;
                Ok(())
            },
        )
        .await?
    }

    Ok(())
}
