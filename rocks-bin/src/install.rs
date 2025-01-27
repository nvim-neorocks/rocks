use eyre::Result;
use rocks_lib::{
    config::{Config, LuaVersion},
    lockfile::PinnedState,
    operations,
    package::PackageReq,
    progress::MultiProgress,
    tree::Tree,
};

use crate::utils::install::apply_build_behaviour;

#[derive(clap::Args)]
pub struct Install {
    /// Package or list of packages to install.
    package_req: Vec<PackageReq>,

    /// Pin the package so that it doesn't get updated.
    #[arg(long)]
    pin: bool,

    /// Reinstall without prompt if a package is already installed.
    #[arg(long)]
    force: bool,
}

pub async fn install(data: Install, config: Config) -> Result<()> {
    let pin = PinnedState::from(data.pin);

    let lua_version = LuaVersion::from(&config)?;
    let tree = Tree::new(config.tree().clone(), lua_version)?;

    let packages = apply_build_behaviour(data.package_req, pin, data.force, &tree);

    // TODO(vhyrro): If the tree doesn't exist then error out.
    operations::Install::new(&config)
        .packages(packages)
        .pin(pin)
        .progress(MultiProgress::new_arc())
        .install()
        .await?;

    Ok(())
}
