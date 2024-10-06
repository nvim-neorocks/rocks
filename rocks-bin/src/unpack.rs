use std::path::PathBuf;

use clap::Args;
use eyre::Result;
use indicatif::MultiProgress;
use rocks_lib::{config::Config, remote_package::PackageReq};

#[derive(Args)]
pub struct Unpack {
    /// A path to a .src.rock file. Usually obtained via `rocks download`.
    path: PathBuf,
    /// Where to unpack the rock.
    destination: Option<PathBuf>,
}

#[derive(Args)]
pub struct UnpackRemote {
    #[clap(flatten)]
    package_req: PackageReq,
    /// The directory to unpack to
    path: Option<PathBuf>,

    /// Whether to keep the .src.rock file after pulling it.
    #[arg(long)]
    keep_rockspec: bool,
}

pub async fn unpack(data: Unpack) -> Result<()> {
    let unpack_path =
        rocks_lib::operations::unpack_src_rock(&MultiProgress::new(), data.path, data.destination)
            .await?;

    println!("You may now enter the following directory:");
    println!("{}", unpack_path.display());
    println!("and type `rocks make` to build.");

    Ok(())
}

pub async fn unpack_remote(data: UnpackRemote, config: Config) -> Result<()> {
    let package_req = data.package_req;
    let progress = MultiProgress::new();
    let rock = rocks_lib::operations::download(&progress, &package_req, None, &config).await?;

    let unpack_path =
        rocks_lib::operations::unpack_src_rock(&progress, rock.path.clone(), data.path).await?;

    println!("You may now enter the following directory:");
    println!("{}", unpack_path.display());
    println!("and type `rocks build` to build.");

    if !data.keep_rockspec {
        std::fs::remove_file(rock.path)?;
    }

    Ok(())
}
