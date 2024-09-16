use eyre::eyre;
use itertools::Itertools;
use std::path::PathBuf;

use clap::Args;
use eyre::Result;
use rocks_lib::{config::Config, rockspec::Rockspec};

#[derive(Args)]
pub struct Build {
    rockspec_path: Option<PathBuf>,
}

pub async fn build(data: Build, config: Config) -> Result<()> {
    let rockspec_path = data.rockspec_path.map_or_else(|| {
        // Try to infer the rockspec the user meant.

        let cwd = std::env::current_dir()?;

        let rockspec_paths = walkdir::WalkDir::new(cwd)
            .max_depth(1)
            .same_file_system(true)
            .into_iter()
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry.file_type().is_file()
                    && entry.path().extension().map(|ext| ext.to_str()) == Some(Some("rockspec"))
            })
            .collect_vec();

        let rockspec_count = rockspec_paths.len();

        match rockspec_count {
            0 => Err(eyre!("No rockspec files found in the current directory!")),
            1 => Ok(rockspec_paths.first().unwrap().clone().into_path()),
            _ => Err(eyre!("Could not infer the rockspec to use! There are {} rockspecs in the current directory, please provide a path to the one you'd like to use.", rockspec_count)),
        }
    }, Ok)?;

    if rockspec_path
        .extension()
        .map(|ext| ext != "rockspec")
        .unwrap_or(true)
    {
        return Err(eyre!("Provided path is not a valid rockspec!"));
    }

    let rockspec = std::fs::read_to_string(rockspec_path)?;
    let rockspec = Rockspec::new(&rockspec)?;

    rocks_lib::build::build(rockspec, &config).await?;

    Ok(())
}
