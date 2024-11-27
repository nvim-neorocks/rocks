mod metadata;
mod pull_manifest;

pub use metadata::*;
pub use pull_manifest::*;

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use crate::package::PackageReq;

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
        test_manifest_path.push("resources/test/manifest-5.1");
        let manifest = String::from_utf8(fs::read(&test_manifest_path).unwrap()).unwrap();
        metadata::ManifestMetadata::new(&manifest).unwrap();
    }

    #[tokio::test]
    pub async fn latest_match_regression() {
        let mut test_manifest_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        test_manifest_path.push("resources/test/manifest-5.1");
        let manifest = String::from_utf8(fs::read(&test_manifest_path).unwrap()).unwrap();
        let metadata = metadata::ManifestMetadata::new(&manifest).unwrap();

        let package_req: PackageReq = "30log > 1.3.0".parse().unwrap();
        assert!(metadata.latest_match(&package_req).is_none());
    }
}
