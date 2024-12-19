use std::collections::HashMap;

use clap::Args;
use eyre::{eyre, Result};
use itertools::{Either, Itertools};
use rocks_lib::{
    config::{Config, LuaVersion},
    lockfile::LocalPackage,
    package::PackageReq,
    progress::{MultiProgress, Progress},
    tree::Tree,
};

#[derive(Args)]
pub struct Remove {
    /// The name of the rock to remove.
    packages: Vec<PackageReq>,
}

#[derive(PartialEq)]
enum InvalidPackageType {
    Duplicate(Vec<LocalPackage>),
    DoesNotExist(PackageReq),
}

// TODO(vhyrro): Properly handle multiple versions of the same package
pub async fn remove(remove_args: Remove, config: Config) -> Result<()> {
    let tree = Tree::new(config.tree().clone(), LuaVersion::from(&config)?)?;

    let (packages, invalid_packages): (Vec<_>, Vec<_>) = remove_args
        .packages
        .into_iter()
        .partition_map(|package| match tree.match_rocks(&package) {
            Some(local_packages) if local_packages.len() == 1 => Either::Left(local_packages),
            Some(local_packages) => Either::Right(InvalidPackageType::Duplicate(local_packages)),
            _ => Either::Right(InvalidPackageType::DoesNotExist(package)),
        });

    let (nonexistent_packages, duplicate_packages): (Vec<_>, Vec<_>) = invalid_packages
        .into_iter()
        .partition_map(|invalid_package| match invalid_package {
            InvalidPackageType::DoesNotExist(package_req) => Either::Left(package_req),
            InvalidPackageType::Duplicate(vec) => Either::Right(
                vec.into_iter()
                    .into_group_map_by(|package| package.name().to_string()),
            ),
        });

    if !nonexistent_packages.is_empty() {
        // TODO(vhyrro): Render this in the form of a tree.
        return Err(eyre!(
            "The following packages were not found: {:#?}",
            nonexistent_packages
        ));
    }

    if !duplicate_packages.is_empty() {
        // TODO(vhyrro): Display a full list of multiple packages and how to fix the error
        let _duplicate_packages: HashMap<_, _> = duplicate_packages.into_iter().flatten().collect();

        return Err(eyre!(
            "Multiple packages satisfying the following conditions were found."
        ));
    }

    let packages = packages.into_iter().flatten().collect_vec();

    rocks_lib::operations::remove(packages, &config, &Progress::Progress(MultiProgress::new()))
        .await?;

    Ok(())
}
