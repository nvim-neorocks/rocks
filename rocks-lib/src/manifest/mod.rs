mod metadata;
mod pull_manifest;

pub use metadata::*;
pub use pull_manifest::*;

#[cfg(test)]
mod tests {
    use crate::config::Config;

    use super::*;

    #[tokio::test]
    pub async fn parse_metadata() {
        let config = Config::default();

        let manifest = pull_manifest::manifest_from_server("https://luarocks.org/".into(), &config)
            .await
            .unwrap();

        metadata::ManifestMetadata::new(&manifest).unwrap();
    }
}
