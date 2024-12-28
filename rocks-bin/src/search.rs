use std::collections::HashMap;

use clap::Args;
use eyre::Result;
use itertools::Itertools;
use text_trees::{FormatCharacters, StringTreeNode, TreeFormatting};

use rocks_lib::{
    config::Config,
    package::{PackageName, PackageReq, PackageVersion},
    progress::{MultiProgress, ProgressBar},
    remote_package_db::RemotePackageDB,
};

#[derive(Args)]
pub struct Search {
    lua_package_req: PackageReq,
    // TODO(vhyrro): Add options.
    /// Return a machine readable format.
    #[arg(long)]
    porcelain: bool,
}

pub async fn search(data: Search, config: Config) -> Result<()> {
    let progress = MultiProgress::new();
    let bar = progress.add(ProgressBar::from(format!(
        "🔎 Searching for `{}`...",
        data.lua_package_req
    )));

    let formatting = TreeFormatting::dir_tree(FormatCharacters::box_chars());

    let package_db = RemotePackageDB::from_config(&config).await?;

    let lua_package_req = data.lua_package_req;

    let result = package_db.search(&lua_package_req);

    bar.finish_and_clear();

    if data.porcelain {
        let rock_to_version_map: HashMap<&PackageName, Vec<&PackageVersion>> =
            HashMap::from_iter(result);
        println!("{}", serde_json::to_string(&rock_to_version_map)?);
    } else {
        for (key, versions) in result.into_iter().sorted() {
            let mut tree = StringTreeNode::new(key.to_string().to_owned());

            for version in versions {
                tree.push(version.to_string());
            }

            println!("{}", tree.to_string_with_format(&formatting).unwrap());
        }
    }

    Ok(())
}
