use clap::Args;
use eyre::Result;
use rocks_lib::{
    config::Config,
    operations,
    package::PackageReq,
    progress::{MultiProgress, Progress},
};

#[derive(Args)]
pub struct Download {
    package_req: PackageReq,
}

pub async fn download(dl_data: Download, config: Config) -> Result<()> {
    let progress = MultiProgress::new();
    let bar = Progress::Progress(progress.new_bar());

    let rock = operations::Download::new(&dl_data.package_req, &config, &bar)
        .download_src_rock_to_file(None)
        .await?;

    bar.map(|b| {
        b.finish_with_message(format!(
            "Succesfully downloaded {}@{}",
            rock.name, rock.version
        ))
    });

    Ok(())
}
