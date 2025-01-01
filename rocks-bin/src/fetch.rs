use std::path::PathBuf;

use eyre::Result;
use rocks_lib::{
    config::Config,
    operations::Download,
    progress::{MultiProgress, Progress},
};

use crate::unpack::UnpackRemote;

pub async fn fetch_remote(data: UnpackRemote, config: Config) -> Result<()> {
    let package_req = data.package_req;
    let progress = MultiProgress::new();
    let bar = Progress::Progress(progress.new_bar());

    let rockspec = Download::new(&package_req, &config, &bar)
        .download_rockspec()
        .await?
        .rockspec;

    let destination = data
        .path
        .unwrap_or_else(|| PathBuf::from(format!("{}-{}", &rockspec.package, &rockspec.version)));
    let rock_source = rockspec.source.current_platform();
    rocks_lib::operations::fetch_src(destination.clone().as_path(), rock_source, &bar).await?;

    let build_dir = rock_source
        .unpack_dir
        .as_ref()
        .map(|path| destination.join(path))
        .unwrap_or_else(|| destination);

    bar.map(|b| {
        b.finish_with_message(format!(
            "
You may now enter the following directory:
{}
and type `rocks build` to build.
    ",
            build_dir.as_path().display()
        ))
    });

    Ok(())
}
