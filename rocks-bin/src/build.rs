use eyre::eyre;
use std::path::PathBuf;

use clap::Args;
use eyre::Result;
use rocks_lib::{config::Config, rockspec::Rockspec};

#[derive(Args)]
pub struct Build {
    rockspec_path: PathBuf,
}

pub fn build(data: Build, config: &Config) -> Result<()> {
    if data
        .rockspec_path
        .extension()
        .map(|ext| ext != "rockspec")
        .unwrap_or(true)
    {
        return Err(eyre!("Provided path is not a valid rockspec!"));
    }

    let rockspec = String::from_utf8(std::fs::read(data.rockspec_path)?)?;
    let rockspec = Rockspec::new(&rockspec)?;

    rocks_lib::build::build(rockspec, config)?;

    Ok(())
}
