use eyre::Result;
use std::{fs::File, path::PathBuf};

pub fn unpack(rock_path: PathBuf, destination: Option<PathBuf>) -> Result<PathBuf> {
    let file = File::open(&rock_path)?;

    let mut zip = zip::ZipArchive::new(file)?;

    let destination = destination.unwrap_or_else(|| PathBuf::from(
        rock_path.to_str().unwrap().trim_end_matches(".src.rock"),
    ));

    zip.extract(&destination)?;

    Ok(destination)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    pub fn unpack_rock() {
        let test_rock_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("test");

        unpack(test_rock_path.join("luatest-0.2-1.src.rock"), None).unwrap();
    }
}
