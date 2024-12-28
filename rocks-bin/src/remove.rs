use clap::Args;
use eyre::Result;
use rocks_lib::{
    config::{Config, LuaVersion},
    package::{PackageName, PackageSpec, PackageVersion},
    progress::{MultiProgress, Progress},
    remote_package_db::RemotePackageDB,
    tree::Tree,
};

#[derive(Args)]
pub struct Remove {
    /// The name of the rock to remove.
    name: PackageName,
    /// The name of the version to remove.
    version: Option<PackageVersion>,
}

pub async fn remove(remove_args: Remove, config: Config) -> Result<()> {
    let package_db = RemotePackageDB::from_config(&config).await?;

    let target_version = remove_args
        .version
        .or(package_db.latest_version(&remove_args.name).cloned())
        .unwrap();

    let tree = Tree::new(config.tree().clone(), LuaVersion::from(&config)?)?;

    match tree.has_rock(
        &PackageSpec::new(remove_args.name.clone(), target_version.clone()).into_package_req(),
    ) {
        Some(package) => Ok(rocks_lib::operations::remove(
            package,
            &config,
            &Progress::Progress(MultiProgress::new().new_bar()),
        )
        .await?),
        None => {
            eprintln!("Could not find {}@{}", remove_args.name, target_version);
            Ok(())
        }
    }
}
