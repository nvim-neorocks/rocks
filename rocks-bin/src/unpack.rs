use std::path::PathBuf;

use clap::Args;
use eyre::Result;
use rocks_lib::{config::Config, lua_package::PackageName};

#[derive(Args)]
pub struct Unpack {
    /// A path to a .src.rock file. Usually obtained via `rocks download`.
    path: PathBuf,
    /// Where to unpack the rock.
    destination: Option<PathBuf>,
}

#[derive(Args)]
pub struct UnpackRemote {
    /// The name of the rock to unpack
    name: PackageName,
    /// The version of the rock to download (defaults to latest version)
    version: Option<String>,
    /// The directory to unpack to
    path: Option<PathBuf>,

    /// Whether to keep the .src.rock file after pulling it.
    #[arg(long)]
    keep_rockspec: bool,
}

pub async fn unpack(data: Unpack) -> Result<()> {
    let unpack_path = rocks_lib::operations::unpack_src_rock(data.path, data.destination)?;

    println!("Done. You may now enter the following directory:");
    println!("{}", unpack_path.display());
    println!("and type `rocks make` to build.");

    Ok(())
}

pub async fn unpack_remote(data: UnpackRemote, config: &Config) -> Result<()> {
    println!("Downloading {}...", data.name);

    let rock =
        rocks_lib::operations::download(&data.name, data.version.as_ref(), None, config).await?;

    println!("Unpacking {}...", rock.path.display());

    let unpack_path = rocks_lib::operations::unpack_src_rock(rock.path.clone(), data.path)?;

    println!("Done. You may now enter the following directory:");
    println!("{}", unpack_path.display());
    println!("and type `rocks build` to build.");

    if !data.keep_rockspec {
        std::fs::remove_file(rock.path)?;
    }

    Ok(())
}
