use std::path::PathBuf;

use eyre::Result;
use indicatif::MultiProgress;
use rocks_lib::config::Config;

use crate::unpack::UnpackRemote;

pub async fn fetch_remote(data: UnpackRemote, config: Config) -> Result<()> {
    let package_req = data.package_req;
    let progress = MultiProgress::new();
    let rockspec =
        rocks_lib::operations::download_rockspec(&progress, &package_req, &config).await?;

    let destination = data
        .path
        .unwrap_or_else(|| PathBuf::from(format!("{}-{}", &rockspec.package, &rockspec.version)));
    let rock_source = rockspec.source.current_platform();
    rocks_lib::operations::fetch_src(&progress, destination.clone().as_path(), rock_source).await?;

    let build_dir = rock_source
        .unpack_dir
        .as_ref()
        .map(|path| destination.join(path))
        .unwrap_or_else(|| destination);

    println!("You may now enter the following directory:");
    println!("{}", build_dir.as_path().display());
    println!("and type `rocks build` to build.");

    Ok(())
}
