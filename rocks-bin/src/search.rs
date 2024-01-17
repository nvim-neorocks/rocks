use anyhow::Result;
use clap::Args;
use itertools::Itertools;
use text_trees::{FormatCharacters, StringTreeNode, TreeFormatting};

use rocks_lib::manifest::{manifest_from_server, ManifestMetadata};

#[derive(Args)]
pub struct Search {
    /// Name of the rock to search for.
    name: String,
    /// Rocks version to search for.
    version: Option<String>,
    // TODO(vhyrro): Add options.
}

pub async fn search(data: Search) -> Result<()> {
    let formatting = TreeFormatting::dir_tree(FormatCharacters::box_chars());

    // TODO(vhyrro): Pull in global configuration in the form of a second parameter (including which server to use for the manifest).

    let manifest = manifest_from_server("https://luarocks.org/manifest".into(), None).await?;

    let metadata = ManifestMetadata::new(&manifest)?;

    for key in metadata.repository.keys().collect::<Vec<&String>>() {
        // TODO(vhyrro): Use fuzzy matching here instead.
        if key.find(&data.name).is_some() {
            let mut tree = StringTreeNode::new(key.to_owned());

            metadata.repository[key]
                .keys()
                .sorted_by(|a, b| Ord::cmp(b, a))
                .for_each(|version| tree.push(version.to_owned()));

            println!("{}", tree.to_string_with_format(&formatting)?);
        }
    }

    Ok(())
}
