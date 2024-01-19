use eyre::Result;
use clap::Args;
use itertools::Itertools;
use text_trees::{FormatCharacters, StringTreeNode, TreeFormatting};

use rocks_lib::{
    config::Config,
    manifest::{manifest_from_server, ManifestMetadata},
};

#[derive(Args)]
pub struct Search {
    /// Name of the rock to search for.
    name: String,
    /// Rocks version to search for.
    version: Option<String>,
    // TODO(vhyrro): Add options.
    /// Return a machine readable format.
    #[arg(long)]
    porcelain: bool,
}

pub async fn search(data: Search, config: &Config) -> Result<()> {
    let formatting = TreeFormatting::dir_tree(FormatCharacters::box_chars());

    // TODO(vhyrro): Pull in global configuration in the form of a second parameter (including which server to use for the manifest).

    let manifest = manifest_from_server(config.server.to_owned(), &config).await?;

    let metadata = ManifestMetadata::new(&manifest)?;

    for key in metadata
        .repository
        .keys()
        .sorted()
        .collect::<Vec<&String>>()
    {
        // TODO(vhyrro): Use fuzzy matching here instead.
        if key.contains(&data.name) {
            let versions = metadata.repository[key]
                .keys()
                .sorted_by(|a, b| Ord::cmp(b, a));

            if data.porcelain {
                versions.for_each(|version| {
                    println!("{} {} {} {}", key, version, "src|rockspec", config.server)
                });
            } else {
                let mut tree = StringTreeNode::new(key.to_owned());

                versions.for_each(|version| tree.push(version.to_owned()));

                println!("{}", tree.to_string_with_format(&formatting)?);
            }
        }
    }

    Ok(())
}
