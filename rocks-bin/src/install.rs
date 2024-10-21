use eyre::Result;
use indicatif::MultiProgress;
use rocks_lib::{config::Config, package::PackageReq};

#[derive(clap::Args)]
pub struct Install {
    package_req: PackageReq,

    #[arg(long)]
    pin: bool,
}

pub async fn install(install_data: Install, config: Config) -> Result<()> {
    // TODO(vhyrro): If the tree doesn't exist then error out.
    rocks_lib::operations::install(
        &MultiProgress::new(),
        install_data.package_req,
        install_data.pin,
        &config,
    )
    .await?;

    Ok(())
}
