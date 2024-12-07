use clap::Args;
use eyre::Result;
use rocks_lib::{
    config::{Config, LuaVersion},
    manifest::{manifest_from_server, ManifestMetadata},
    package::{PackageName, PackageVersion, RemotePackage},
    progress::{MultiProgress, Progress},
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
    let manifest = manifest_from_server(config.server().clone(), &config).await?;

    let metadata = ManifestMetadata::new(&manifest)?;

    let target_version = remove_args
        .version
        .or(metadata.latest_version(&remove_args.name).cloned())
        .unwrap();

    let tree = Tree::new(config.tree().clone(), LuaVersion::from(&config)?)?;

    match tree.has_rock(
        &RemotePackage::new(remove_args.name.clone(), target_version.clone()).into_package_req(),
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
