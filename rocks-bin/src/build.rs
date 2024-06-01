use eyre::eyre;
use std::path::PathBuf;

use clap::Args;
use eyre::Result;
use rocks_lib::rockspec::Rockspec;

#[derive(Args)]
pub struct Build {
    directory: PathBuf,
}

pub fn build(data: Build) -> Result<()> {
    if data
        .directory
        .extension()
        .map(|ext| ext != "rockspec")
        .unwrap_or(true)
    {
        return Err(eyre!("Provided path is not a valid rockspec!"));
    }

    let rockspec = String::from_utf8(std::fs::read(data.directory)?)?;
    let rockspec = Rockspec::new(&rockspec)?;

    rocks_lib::build::build(rockspec, false)?;

    Ok(())
}
