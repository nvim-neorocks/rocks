use eyre::Result;
use reqwest::Client;
use std::{fs, time::SystemTime};

use crate::config::Config;

// TODO(vhyrro): Perhaps cache the manifest somewhere on disk?
pub async fn manifest_from_server(url: String, config: &Config) -> Result<String> {
    let manifest_filename = "manifest".to_string()
        + &config
            .lua_version()
            .map(|s| format!("-{}", s))
            .unwrap_or_default();
    let url = url.trim_end_matches('/').to_string() + "/" + &manifest_filename;

    // Stores a path to the manifest cache (this allows us to operate on a manifest without
    // needing to pull it from the luarocks servers each time).
    let cache = Config::get_default_cache_path()?.join(&manifest_filename);

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
                // Since we only pulled in the headers previously we must now request the entire
                // manifest from scratch.
                let new_manifest_content = client.get(&url).send().await?.text().await?;
                fs::write(&cache, &new_manifest_content)?;

                return Ok(new_manifest_content);
            }

            // Else return the cached manifest.
            return Ok(String::from_utf8(fs::read(&cache)?)?);
        }
    }

    // If our cache file does not exist then pull the whole manifest.

    let new_manifest = client.get(url).send().await?.text().await?;

    fs::write(&cache, &new_manifest)?;

    Ok(new_manifest)
}

#[cfg(test)]
mod tests {
    use httptest::{matchers::*, responders::*, Expectation, Server};
    use serial_test::serial;

    use crate::config::ConfigBuilder;

    use super::*;

    fn reset_cache() {
        let cache_path = Config::get_default_cache_path().unwrap();
        let _ = fs::remove_dir_all(&cache_path);
        fs::create_dir_all(cache_path).unwrap();
    }

    fn start_test_server(manifest_name: String) -> Server {
        let server = Server::run();
        let manifest_path = format!("/{}", manifest_name);
        server.expect(
            Expectation::matching(request::path(manifest_path.to_string()))
                .times(1..)
                .respond_with(
                    status_code(200)
                        .append_header("Last-Modified", "Sat, 20 Jan 2024 13:14:12 GMT")
                        .body("dummy data"),
                ),
        );
        server
    }

    #[tokio::test]
    #[serial]
    pub async fn get_manifest() {
        reset_cache();
        let server = start_test_server("manifest".into());
        let mut url_str = server.url_str(""); // Remove trailing "/"
        url_str.pop();
        let config = ConfigBuilder::new().build().unwrap();
        manifest_from_server(url_str, &config).await.unwrap();
    }

    #[tokio::test]
    #[serial]
    pub async fn get_manifest_for_5_1() {
        reset_cache();
        let server = start_test_server("manifest-5.1".into());
        let mut url_str = server.url_str(""); // Remove trailing "/"
        url_str.pop();

        let config = ConfigBuilder::new()
            .lua_version(Some(crate::config::LuaVersion::Lua51))
            .build()
            .unwrap();

        manifest_from_server(url_str, &config).await.unwrap();
    }

    #[tokio::test]
    #[serial]
    pub async fn get_cached_manifest() {
        reset_cache();
        let server = start_test_server("manifest".into());
        let mut url_str = server.url_str(""); // Remove trailing "/"
        url_str.pop();
        let manifest_content = "dummy data";
        let config = ConfigBuilder::new().build().unwrap();
        let cache_dir = Config::get_default_cache_path().unwrap();
        let cache = cache_dir.join("manifest");
        fs::write(&cache, manifest_content).unwrap();
        let _metadata = fs::metadata(&cache).unwrap();
        let result = manifest_from_server(url_str, &config).await.unwrap();
        assert_eq!(result, manifest_content);
    }
}
