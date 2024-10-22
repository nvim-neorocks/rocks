use clap::Args;
use eyre::Result;
use indicatif::MultiProgress;
use rocks_lib::config::LuaVersion;
use rocks_lib::{
    config::Config,
    manifest::{manifest_from_server, ManifestMetadata},
    operations,
    package::PackageReq,
    tree::Tree,
};

#[derive(Args)]
pub struct Update {}

pub async fn update(config: Config) -> Result<()> {
    let tree = Tree::new(config.tree().clone(), LuaVersion::from(&config)?)?;

    let lockfile = tree.lockfile()?;
    let rocks = lockfile.rocks();
    let manifest =
        ManifestMetadata::new(&manifest_from_server(config.server().clone(), &config).await?)?;

    let progress = MultiProgress::new();

    for package in rocks.values() {
        if !package.pinned {
            operations::update(
                &progress,
                package.clone(),
                PackageReq::new(
                    package.name.to_string(),
                    package.constraint.as_ref().map(|str| str.to_string()),
                )?,
                &manifest,
                &config,
            )
            .await?;
        }
    }

    Ok(())
}
