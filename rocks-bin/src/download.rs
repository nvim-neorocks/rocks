use clap::Args;
use eyre::Result;
use rocks_lib::{
    config::Config,
    package::PackageReq,
    progress::{MultiProgress, Progress},
    remote_package_db::RemotePackageDB,
};

#[derive(Args)]
pub struct Download {
    package_req: PackageReq,
}

pub async fn download(dl_data: Download, config: Config) -> Result<()> {
    let package_db = RemotePackageDB::from_config(&config).await?;
    let progress = MultiProgress::new();
    let bar = Progress::Progress(progress.new_bar());

    let rock =
        rocks_lib::operations::download_to_file(&dl_data.package_req, None, &package_db, &bar)
            .await?;

    bar.map(|b| {
        b.finish_with_message(format!(
            "Succesfully downloaded {}@{}",
            rock.name, rock.version
        ))
    });

    Ok(())
}
