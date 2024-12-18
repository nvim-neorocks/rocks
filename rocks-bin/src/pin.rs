use clap::Args;
use eyre::eyre;
use eyre::Result;
use rocks_lib::lockfile::PinnedState;
use rocks_lib::operations;
use rocks_lib::package::PackageSpec;
use rocks_lib::tree::RockMatches;
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

    match tree.match_rocks_and(&data.package.clone().into_package_req(), |package| {
        pin != package.pinned()
    })? {
        RockMatches::Single(mut rock) => Ok(operations::set_pinned_state(&mut rock, &tree, pin)?),
        RockMatches::Many(_) => {
            panic!("TODO: Add an error here about many conflicting types and to use `all:`")
        }
        RockMatches::NotFound(_) => Err(eyre!("Rock {} not found!", data.package)),
    }
}
