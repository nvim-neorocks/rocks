use clap::Args;
use eyre::Result;
use rocks_lib::{
    config::Config, manifest::ManifestMetadata, package::PackageReq, progress::MultiProgress,
};

#[derive(Args)]
pub struct Download {
    package_req: PackageReq,
}

pub async fn download(dl_data: Download, config: Config) -> Result<()> {
    let manifest = ManifestMetadata::from_config(&config).await?;
    let progress = MultiProgress::new();
    let bar = progress.new_bar();

    let rock = rocks_lib::operations::download_to_file(
        &bar,
        &dl_data.package_req,
        None,
        &manifest,
        &config,
    )
    .await?;

    bar.finish_with_message(format!(
        "Succesfully downloaded {}@{}",
        rock.name, rock.version
    ));

    Ok(())
}
