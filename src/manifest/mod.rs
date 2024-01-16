mod metadata;
mod pull_manifest;

pub use metadata::*;
pub use pull_manifest::*;

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
