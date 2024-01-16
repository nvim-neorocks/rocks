use anyhow::Result;
use clap::Args;
use text_trees::{StringTreeNode, FormatCharacters, TreeFormatting};

#[derive(Args)]
pub struct Search {
    /// Name of the rock to search for.
    name: String,
    /// Rocks version to search for.
    version: Option<String>,

    // TODO(vhyrro): Add options.
}

pub fn search(data: Search) -> Result<()> {
    let mut tree = StringTreeNode::new("Root Manifest".into());

    tree.push("Something cool".into());

    println!("{}", tree.to_string_with_format(&TreeFormatting::dir_tree(FormatCharacters::box_chars()))?);

    Ok(())
}
