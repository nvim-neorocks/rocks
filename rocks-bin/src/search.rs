use std::collections::HashMap;

use clap::Args;
use eyre::Result;
use itertools::Itertools;
use text_trees::{FormatCharacters, StringTreeNode, TreeFormatting};

use rocks_lib::{
    config::Config,
    lua_package::{LuaPackageReq, PackageName, PackageVersion},
    manifest::{manifest_from_server, ManifestMetadata},
};

#[derive(Args)]
pub struct Search {
    #[clap(flatten)]
    lua_package_req: LuaPackageReq,
    // TODO(vhyrro): Add options.
    /// Return a machine readable format.
    #[arg(long)]
    porcelain: bool,
}

pub async fn search(data: Search, config: &Config) -> Result<()> {
    let formatting = TreeFormatting::dir_tree(FormatCharacters::box_chars());

    let manifest = manifest_from_server(config.server.to_owned(), config).await?;

    let metadata = ManifestMetadata::new(&manifest)?;

    let lua_package_req = data.lua_package_req;

    let rock_to_version_map: HashMap<&PackageName, Vec<&PackageVersion>> = metadata
        .repository
        .iter()
        .filter_map(|(name, elements)| {
            if name
                .to_string()
                .contains(&lua_package_req.name().to_string())
            {
                Some((
                    name,
                    elements
                        .keys()
                        .filter(|version| lua_package_req.version_req().matches(version))
                        .sorted_by(|a, b| Ord::cmp(b, a))
                        .collect_vec(),
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
                tree.push(version.to_string());
            }

            println!("{}", tree.to_string_with_format(&formatting).unwrap());
        }
    }

    Ok(())
}
