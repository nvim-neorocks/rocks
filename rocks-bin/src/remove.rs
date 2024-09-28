use clap::Args;
use eyre::Result;
use indicatif::MultiProgress;
use rocks_lib::{
    config::Config,
    lua_package::{LuaPackage, PackageName, PackageVersion},
    manifest::{manifest_from_server, ManifestMetadata},
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

    let package = LuaPackage::new(
        remove_args.name.clone(),
        remove_args
            .version
            .or(metadata.latest_version(&remove_args.name).cloned())
            .unwrap(),
    );

    rocks_lib::operations::remove(&MultiProgress::new(), package, &config).await
}
