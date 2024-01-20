mod metadata;
mod pull_manifest;

pub use metadata::*;
pub use pull_manifest::*;

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use super::*;

    #[tokio::test]
    pub async fn parse_metadata_from_empty_manifest() {
        let manifest = "
            commands = {}\n
            modules = {}\n
            repository = {}\n
            "
        .to_string();
        metadata::ManifestMetadata::new(&manifest).unwrap();
    }

    #[tokio::test]
    pub async fn parse_metadata_from_test_manifest() {
        let mut test_manifest_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        test_manifest_path.push("resources/test/manifest");
        let manifest = String::from_utf8(fs::read(&test_manifest_path).unwrap()).unwrap();
        metadata::ManifestMetadata::new(&manifest).unwrap();
    }
}
