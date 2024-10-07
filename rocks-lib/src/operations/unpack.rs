use crate::progress::with_spinner;

use eyre::Result;
use indicatif::MultiProgress;
use std::io::Read;
use std::io::Seek;
use std::path::PathBuf;

pub async fn unpack_src_rock<R: Read + Seek + Send>(
    progress: &MultiProgress,
    rock_src: R,
    destination: PathBuf,
) -> Result<PathBuf> {
    with_spinner(
        progress,
        format!("📦 Unpacking src.rock into {}", destination.display()),
        || async { unpack_src_rock_impl(rock_src, destination).await },
    )
    .await
}

async fn unpack_src_rock_impl<R: Read + Seek + Send>(
    rock_src: R,
    destination: PathBuf,
) -> Result<PathBuf> {
    let mut zip = zip::ZipArchive::new(rock_src)?;
    zip.extract(&destination)?;
    Ok(destination)
}

#[cfg(test)]
mod tests {
    use std::fs::File;

    use tempdir::TempDir;

    use super::*;

    #[tokio::test]
    pub async fn unpack_rock() {
        let test_rock_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("test")
            .join("luatest-0.2-1.src.rock");
        let file = File::open(&test_rock_path).unwrap();
        let dest = TempDir::new("rocks-test").unwrap();
        unpack_src_rock(&MultiProgress::new(), file, dest.into_path())
            .await
            .unwrap();
    }
}
