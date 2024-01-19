use eyre::Result;
use reqwest::Client;

use crate::config::Config;
use std::{fs, time::SystemTime};

// TODO(vhyrro): Perhaps cache the manifest somewhere on disk?
pub async fn manifest_from_server(url: String, config: &Config) -> Result<String> {
    let manifest_filename = "manifest".to_string()
        + &config
            .lua_version
            .as_ref()
            .map(|s| format!("-{}", s))
            .unwrap_or_default();
    let url = url + "/" + &manifest_filename;

    // Stores a path to the manifest cache (this allows us to operate on a manifest without
    // needing to pull it from the luarocks servers each time).
    let cache = config.get_default_cache_path()?.join(&manifest_filename);

    // Ensure all intermediate directories for the cache file are created (e.g. `~/.cache/rocks/manifest`)
    fs::create_dir_all(cache.parent().unwrap())?;

    let client = Client::new();

    // Read the metadata of the local cache and attempt to get the last modified date.
    if let Ok(metadata) = fs::metadata(&cache) {
        let last_modified_local: SystemTime = metadata.modified()?;

        // Ask the server for the last modified date of its manifest.
        let response = client.head(&url).send().await?;

        if let Some(last_modified_header) = response.headers().get("Last-Modified") {
            let server_last_modified = httpdate::parse_http_date(last_modified_header.to_str()?)?;

            // If the server's version of the manifest is newer than ours then update out manifest.
            if server_last_modified > last_modified_local {
                let new_manifest_content = response.text().await?;
                fs::write(&cache, &new_manifest_content)?;

                return Ok(new_manifest_content);
            }

            // Else return the cached manifest.
            return Ok(String::from_utf8(fs::read(&cache)?)?);
        }
    }

    // If our cache file does not exist then pull the whole manifest.

    let new_manifest = reqwest::get(url).await?.text().await?;

    fs::write(&cache, &new_manifest)?;

    Ok(new_manifest)
}

#[cfg(test)]
mod tests {
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
    pub async fn get_manifest() {
        reset_cache();
        let config = Config::default();
        manifest_from_server("https://luarocks.org".into(), &config)
            .await
            .unwrap();
    }

    #[tokio::test]
    #[serial]
    pub async fn get_manifest_for_5_1() {
        reset_cache();
        let mut config = Config::default();

        config.lua_version = Some("5.1".into());

        manifest_from_server("https://luarocks.org".into(), &config)
            .await
            .unwrap();
    }

    #[tokio::test]
    #[serial]
    pub async fn get_cached_manifest() {
        reset_cache();
        let manifest_content = "dummy content";
        let config = Config::default();
        let cache_dir = config.get_default_cache_path().unwrap();
        let cache = cache_dir.join("manifest");
        fs::write(&cache, manifest_content).unwrap();
        let _metadata = fs::metadata(&cache).unwrap();
        let result = manifest_from_server("https://luarocks.org".into(), &config)
            .await
            .unwrap();
        assert_eq!(result, manifest_content);
    }
}
