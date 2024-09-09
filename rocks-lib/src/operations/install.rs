use eyre::Result;
use tempdir::TempDir;

use crate::{config::Config, rockspec::Rockspec};

pub async fn install(
    rock_name: String,
    rock_version: Option<String>,
    config: &Config,
) -> Result<()> {
    let temp = TempDir::new(&rock_name)?;

    let rock = super::download(
        &rock_name,
        rock_version.as_ref(),
        Some(temp.path().to_path_buf()),
        config,
    )
    .await?;

    super::unpack_src_rock(temp.path().join(rock.path), Some(temp.path().to_path_buf()))?;

    let rockspec_path = walkdir::WalkDir::new(&temp)
        .max_depth(1)
        .same_file_system(true)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .find(|entry| {
            entry.file_type().is_file()
                && entry.path().extension().map(|ext| ext.to_str()) == Some(Some("rockspec"))
        })
        .expect("could not find rockspec in source directory. this is a bug, please report it.")
        .into_path();

    crate::build::build(
        Rockspec::new(&std::fs::read_to_string(rockspec_path)?)?,
        config,
    )?;

    Ok(())
}
