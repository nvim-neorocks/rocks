use clap::Args;
use eyre::Result;
use itertools::Itertools as _;
use rocks_lib::{
    config::{Config, LuaVersion},
    lockfile::PinnedState,
    tree::Tree,
};
use text_trees::{FormatCharacters, StringTreeNode, TreeFormatting};

#[derive(Args)]
pub struct ListCmd {
    #[arg(long)]
    porcelain: bool,
}

pub fn list_installed(list_data: ListCmd, config: Config) -> Result<()> {
    let tree = Tree::new(config.tree().clone(), LuaVersion::from(&config)?)?;
    let available_rocks = tree.list()?;

    if list_data.porcelain {
        println!("{}", serde_json::to_string(&available_rocks)?);
    } else {
        let formatting = TreeFormatting::dir_tree(FormatCharacters::box_chars());
        for (name, packages) in available_rocks.into_iter().sorted() {
            let mut tree = StringTreeNode::new(name.to_string());

            for package in packages {
                tree.push(format!(
                    "{}{}",
                    package.version(),
                    if package.pinned() == PinnedState::Pinned {
                        " (pinned)"
                    } else {
                        ""
                    }
                ));
            }

            println!("{}", tree.to_string_with_format(&formatting)?);
        }
    }

    Ok(())
}
