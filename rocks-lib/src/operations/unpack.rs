use crate::progress::with_spinner;

use eyre::eyre;
use eyre::Result;
use indicatif::MultiProgress;
use std::{fs::File, path::PathBuf};

pub async fn unpack_src_rock(
    progress: &MultiProgress,
    rock_path: PathBuf,
    destination: Option<PathBuf>,
) -> Result<PathBuf> {
    with_spinner(
        progress,
        format!("📦 Unpacking {}", rock_path.display()),
        || async { unpack_src_rock_impl(rock_path, destination).await },
    )
    .await
}

async fn unpack_src_rock_impl(rock_path: PathBuf, destination: Option<PathBuf>) -> Result<PathBuf> {
    let stringified_rock_path = rock_path.to_str().ok_or_else(|| {
        eyre!(
            "Invalid UTF-8 found within rock path: {}",
            rock_path.display()
        )
    })?;

    if !stringified_rock_path.ends_with(".src.rock") && destination.is_none() {
        return Err(eyre!(
            "Unable to unpack a non-source rock: {}",
            rock_path.display()
        ));
    }

    let file = File::open(&rock_path)?;

    let mut zip = zip::ZipArchive::new(file)?;

    let destination = destination
        .unwrap_or_else(|| PathBuf::from(stringified_rock_path.trim_end_matches(".src.rock")));

    zip.extract(&destination)?;

    Ok(destination)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    pub async fn unpack_rock() {
        let test_rock_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("test");

        unpack_src_rock(
            &MultiProgress::new(),
            test_rock_path.join("luatest-0.2-1.src.rock"),
            None,
        )
        .await
        .unwrap();
    }
}
