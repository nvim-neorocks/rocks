use std::{fs::File, io::Cursor, path::PathBuf};

use clap::Args;
use eyre::Result;
use rocks_lib::{
    config::Config, manifest::ManifestMetadata, package::PackageReq, progress::MultiProgress,
};

#[derive(Args)]
pub struct Unpack {
    /// A path to a .src.rock file. Usually obtained via `rocks download`.
    path: PathBuf,
    /// Where to unpack the rock.
    destination: Option<PathBuf>,
}

#[derive(Args)]
pub struct UnpackRemote {
    pub package_req: PackageReq,
    /// The directory to unpack to
    pub path: Option<PathBuf>,
}

pub async fn unpack(data: Unpack) -> Result<()> {
    let destination = data.destination.unwrap_or_else(|| {
        PathBuf::from(data.path.to_string_lossy().trim_end_matches(".src.rock"))
    });
    let src_file = File::open(data.path)?;
    let progress = MultiProgress::new();
    let bar = progress.new_bar();

    let unpack_path = rocks_lib::operations::unpack_src_rock(&bar, src_file, destination).await?;

    bar.finish_with_message(format!(
        "
You may now enter the following directory:
{}
and type `rocks make` to build.",
        unpack_path.display()
    ));

    Ok(())
}

pub async fn unpack_remote(data: UnpackRemote, config: Config) -> Result<()> {
    let package_req = data.package_req;
    let manifest = ManifestMetadata::from_config(&config).await?;
    let progress = MultiProgress::new();
    let bar = progress.new_bar();
    let rock =
        rocks_lib::operations::search_and_download_src_rock(&bar, &package_req, &manifest, &config)
            .await?;
    let cursor = Cursor::new(rock.bytes);

    let destination = data
        .path
        .unwrap_or_else(|| PathBuf::from(format!("{}-{}", &rock.name, &rock.version)));
    let unpack_path = rocks_lib::operations::unpack_src_rock(&bar, cursor, destination).await?;

    bar.finish_with_message(format!(
        "
You may now enter the following directory:
{}
and type `rocks make` to build.",
        unpack_path.display()
    ));

    Ok(())
}
