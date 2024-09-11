use eyre::Result;
use rocks_lib::{config::Config, lua_package::PackageName};

#[derive(clap::Args)]
pub struct Install {
    /// Name of the rock to install.
    name: PackageName,
    /// Rocks version to install.
    version: Option<String>,
}

pub async fn install(install_data: Install, config: &Config) -> Result<()> {
    // TODO(vhyrro): If the tree doesn't exist then error out.
    rocks_lib::operations::install(install_data.name, install_data.version, config).await
}
