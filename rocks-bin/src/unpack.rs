use std::{fs::File, io::Cursor, path::PathBuf};

use clap::Args;
use eyre::Result;
use indicatif::MultiProgress;
use rocks_lib::{config::Config, package::PackageReq};

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
    let unpack_path =
        rocks_lib::operations::unpack_src_rock(&MultiProgress::new(), src_file, destination)
            .await?;

    println!("You may now enter the following directory:");
    println!("{}", unpack_path.display());
    println!("and type `rocks make` to build.");

    Ok(())
}

pub async fn unpack_remote(data: UnpackRemote, config: Config) -> Result<()> {
    let package_req = data.package_req;
    let progress = MultiProgress::new();
    let rock =
        rocks_lib::operations::search_and_download_src_rock(&progress, &package_req, &config)
            .await?;
    let cursor = Cursor::new(rock.bytes);

    let destination = data
        .path
        .unwrap_or_else(|| PathBuf::from(format!("{}-{}", &rock.name, &rock.version)));
    let unpack_path =
        rocks_lib::operations::unpack_src_rock(&progress, cursor, destination).await?;

    println!("You may now enter the following directory:");
    println!("{}", unpack_path.display());
    println!("and type `rocks build` to build.");

    Ok(())
}
