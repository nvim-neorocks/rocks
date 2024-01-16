mod metadata;
mod pull_manifest;

pub use pull_manifest::manifest_from_server;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    pub async fn parse_metadata() {
        let manifest =
            pull_manifest::manifest_from_server("https://luarocks.org/manifest".into(), None)
                .await
                .unwrap();

        metadata::ManifestMetadata::new(&manifest).unwrap();
    }
}
