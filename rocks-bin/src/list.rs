use clap::Args;
use eyre::{OptionExt as _, Result};
use itertools::Itertools as _;
use rocks_lib::{config::Config, tree::Tree};
use text_trees::{FormatCharacters, StringTreeNode, TreeFormatting};

#[derive(Args)]
pub struct ListCmd {
    #[arg(long)]
    outdated: bool,
    #[arg(long)]
    porcelain: bool,
}

pub fn list_installed(list_data: ListCmd, config: &Config) -> Result<()> {
    let tree = Tree::new(
        &config.tree,
        config
            .lua_version
            .as_ref()
            .ok_or_eyre("lua version not supplied!")?,
    )?;
    let available_rocks = tree.list();

    // TODO(vhyrro): Add `outdated` support.

    if list_data.porcelain {
        println!("{}", serde_json::to_string(&available_rocks)?);
    } else {
        let formatting = TreeFormatting::dir_tree(FormatCharacters::box_chars());
        for (name, versions) in available_rocks.into_iter().sorted() {
            let mut tree = StringTreeNode::new(name.to_owned());

            for version in versions {
                tree.push(version);
            }

            println!("{}", tree.to_string_with_format(&formatting)?);
        }
    }

    Ok(())
}
