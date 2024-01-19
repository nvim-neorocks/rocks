mod metadata;
mod pull_manifest;

pub use metadata::*;
pub use pull_manifest::*;

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::config::Config;
    use serial_test::serial;

    use super::*;

    fn reset_cache() {
        let config = Config::default();
        let cache_path = config.get_default_cache_path().unwrap();
        let _ = fs::remove_dir_all(&cache_path);
        fs::create_dir_all(cache_path).unwrap();
    }

    #[tokio::test]
    #[serial]
    pub async fn parse_metadata() {
        reset_cache();
        let config = Config::default();

        let manifest = pull_manifest::manifest_from_server("https://luarocks.org/".into(), &config)
            .await
            .unwrap();

        metadata::ManifestMetadata::new(&manifest).unwrap();
    }
}
