use clap::Args;
use eyre::Result;
use indicatif::MultiProgress;
use rocks_lib::{config::Config, remote_package::PackageReq};

#[derive(Args)]
pub struct Download {
    #[clap(flatten)]
    package_req: PackageReq,
}

pub async fn download(dl_data: Download, config: Config) -> Result<()> {
    println!("Downloading {}...", dl_data.package_req.name());

    let rock =
        rocks_lib::operations::download(&MultiProgress::new(), &dl_data.package_req, None, &config)
            .await?;

    println!("Succesfully downloaded {}@{}", rock.name, rock.version);

    Ok(())
}
