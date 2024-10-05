use clap::Args;
use eyre::OptionExt;
use eyre::Result;
use indicatif::MultiProgress;
use rocks_lib::{
    config::Config,
    lua_package::{LuaPackage, LuaPackageReq},
    manifest::{manifest_from_server, ManifestMetadata},
    operations,
    tree::Tree,
};

#[derive(Args)]
pub struct Update {}

pub async fn update(config: Config) -> Result<()> {
    let tree = Tree::new(
        config.tree().clone(),
        config
            .lua_version()
            .cloned()
            .ok_or_eyre("lua version not supplied!")?,
    )?;

    let lockfile = tree.lockfile()?;
    let rocks = lockfile.rocks();
    let manifest =
        ManifestMetadata::new(&manifest_from_server(config.server().clone(), &config).await?)?;

    let progress = MultiProgress::new();

    for locked_package in rocks.values() {
        if !locked_package.pinned {
            let package =
                LuaPackage::new(locked_package.name.clone(), locked_package.version.clone());
            operations::update(
                &progress,
                package,
                LuaPackageReq::new(
                    locked_package.name.to_string(),
                    locked_package
                        .constraint
                        .as_ref()
                        .map(|str| str.to_string()),
                )?,
                &manifest,
                &config,
            )
            .await?;
        }
    }

    Ok(())
}
