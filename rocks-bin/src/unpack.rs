use std::path::PathBuf;

use clap::Args;
use eyre::Result;

#[derive(Args)]
pub struct Unpack {
    /// A path to a .src.rock file. Usually obtained via `rocks download`.
    path: PathBuf,
    /// Where to unpack the rock.
    destination: Option<PathBuf>,
}

pub async fn unpack(data: Unpack) -> Result<()> {
    let unpack_path = rocks_lib::rocks::unpack(data.path, data.destination)?;

    println!("Done. You may now enter the following directory:");
    println!("{}", unpack_path.display());
    println!("and type `rocks make` to build.");

    Ok(())
}
