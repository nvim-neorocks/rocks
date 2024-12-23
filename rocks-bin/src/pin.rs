use clap::Args;
use eyre::eyre;
use eyre::Result;
use rocks_lib::lockfile::PinnedState;
use rocks_lib::operations;
use rocks_lib::package::PackageSpec;
use rocks_lib::{
    config::{Config, LuaVersion},
    tree::Tree,
};

#[derive(Args)]
pub struct ChangePin {
    package: PackageSpec,
}

pub fn set_pinned_state(data: ChangePin, config: Config, pin: PinnedState) -> Result<()> {
    let tree = Tree::new(config.tree().clone(), LuaVersion::from(&config)?)?;

    if let Some(mut rock) = tree.has_rock_and(&data.package.clone().into_package_req(), |package| {
        pin != package.pinned()
    }) {
        Ok(operations::set_pinned_state(&mut rock, &tree, pin)?)
    } else {
        Err(eyre!("Rock {} not found!", data.package))
    }
}
