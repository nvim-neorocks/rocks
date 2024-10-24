use clap::Args;
use eyre::eyre;
use eyre::Result;
use rocks_lib::operations;
use rocks_lib::package::RemotePackage;
use rocks_lib::{
    config::{Config, LuaVersion},
    tree::Tree,
};

#[derive(Args)]
pub struct Pin {
    package: RemotePackage,
}

pub fn pin(data: Pin, config: Config) -> Result<()> {
    let tree = Tree::new(config.tree().clone(), LuaVersion::from(&config)?)?;

    if let Some(mut rock) = tree.has_rock_and(&data.package.clone().into_package_req(), |package| {
        !package.pinned()
    }) {
        Ok(operations::pin(&mut rock, &tree)?)
    } else {
        Err(eyre!("Rock {} not found!", data.package))
    }
}
