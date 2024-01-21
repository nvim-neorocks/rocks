use eyre::Result;
use std::{
    fs::File,
    path::{Path, PathBuf},
};

pub fn unpack(rock_path: &PathBuf, destination: Option<&PathBuf>) -> Result<PathBuf> {
    let file = File::open(rock_path)?;

    let mut zip = zip::ZipArchive::new(file)?;
    zip.extract(
        destination
            .map(|dest| dest.as_path())
            .unwrap_or_else(|| Path::new(rock_path.file_stem().unwrap())),
    )?;

    Ok(rock_path.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    pub fn unpack_rock() {
        let test_rock_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("test")
            .join("luatest-0.2-1.src.rock");

        unpack(&test_rock_path, None).unwrap();
    }
}
