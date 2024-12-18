use std::io;

use clap::Args;
use eyre::{eyre, Result};
use itertools::Itertools;
use rocks_lib::{
    config::{Config, LuaVersion},
    package::PackageReq,
    progress::{MultiProgress, Progress},
    tree::{RockMatches, Tree},
};

#[derive(Args)]
pub struct Remove {
    /// The name of the rock to remove.
    packages: Vec<PackageReq>,
}

// TODO(vhyrro): Add `all:xyz>=2.0` JJ-like syntax.
pub async fn remove(remove_args: Remove, config: Config) -> Result<()> {
    let tree = Tree::new(config.tree().clone(), LuaVersion::from(&config)?)?;

    let package_matches = remove_args
        .packages
        .iter()
        .map(|package_req| tree.match_rocks(package_req))
        .try_collect::<_, Vec<_>, io::Error>()?;

    let (packages, nonexistent_packages, duplicate_packages) = package_matches.into_iter().fold(
        (Vec::new(), Vec::new(), Vec::new()),
        |(mut p, mut n, mut d), rock_match| {
            match rock_match {
                RockMatches::NotFound(req) => n.push(req),
                RockMatches::Single(package) => p.push(package),
                RockMatches::Many(packages) => d.extend(packages),
            };

            (p, n, d)
        },
    );

    if !nonexistent_packages.is_empty() {
        // TODO(vhyrro): Render this in the form of a tree.
        return Err(eyre!(
            "The following packages were not found: {:#?}",
            nonexistent_packages
        ));
    }

    if !duplicate_packages.is_empty() {
        // TODO(vhyrro): Greatly expand on this error message, notifying the user how it works and
        // how to fix it.
        return Err(eyre!(
            "Multiple packages satisfying your version requirements were found. {:#?}",
            duplicate_packages,
        ));
    }

    rocks_lib::operations::remove(packages, &config, &Progress::Progress(MultiProgress::new()))
        .await?;

    Ok(())
}
