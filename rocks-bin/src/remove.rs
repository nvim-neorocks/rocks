use std::io;

use clap::Args;
use eyre::{eyre, Result};
use itertools::Itertools;
use rocks_lib::{
    config::{Config, LuaVersion},
    operations,
    package::PackageReq,
    tree::RockMatches,
};

// NOTE: This is currently functionally equivalent
// to `rocks uninstall`, but that will change
// when we can use it to edit projects' rocks.toml files.

#[derive(Args)]
pub struct Remove {
    /// The package or packages to remove.
    packages: Vec<PackageReq>,
}

pub async fn remove(remove_args: Remove, config: Config) -> Result<()> {
    let tree = config.tree(LuaVersion::from(&config)?)?;

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
        return Err(eyre!(
            "
Multiple packages satisfying your version requirements were found:
{:#?}

Please specify the exact package to uninstall:
> rocks remove '<name>@<version>'
",
            duplicate_packages,
        ));
    }

    operations::Remove::new(&config)
        .packages(packages)
        .remove()
        .await?;

    Ok(())
}
