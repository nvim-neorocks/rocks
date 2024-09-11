use std::collections::HashMap;

use clap::Args;
use eyre::Result;
use itertools::Itertools;
use text_trees::{FormatCharacters, StringTreeNode, TreeFormatting};

use rocks_lib::{
    config::Config,
    lua_package::PackageName,
    manifest::{manifest_from_server, ManifestMetadata},
};

#[derive(Args)]
pub struct Search {
    /// Name of the rock to search for.
    name: PackageName,
    /// Rocks version to search for.
    version: Option<String>,
    // TODO(vhyrro): Add options.
    /// Return a machine readable format.
    #[arg(long)]
    porcelain: bool,
}

pub async fn search(data: Search, config: &Config) -> Result<()> {
    let formatting = TreeFormatting::dir_tree(FormatCharacters::box_chars());

    let manifest = manifest_from_server(config.server.to_owned(), config).await?;

    let metadata = ManifestMetadata::new(&manifest)?;

    let rock_to_version_map: HashMap<&PackageName, Vec<&String>> = metadata
        .repository
        .iter()
        .filter_map(|(key, value)| {
            if key.to_string().contains(&data.name.to_string()) {
                Some((
                    key,
                    value.keys().sorted_by(|a, b| Ord::cmp(b, a)).collect_vec(),
                ))
            } else {
                None
            }
        })
        .collect();

    if data.porcelain {
        println!("{}", serde_json::to_string(&rock_to_version_map)?);
    } else {
        for (key, versions) in rock_to_version_map.into_iter().sorted() {
            let mut tree = StringTreeNode::new(key.to_string().to_owned());

            for version in versions {
                tree.push(version.to_owned());
            }

            println!("{}", tree.to_string_with_format(&formatting).unwrap());
        }
    }

    Ok(())
}
