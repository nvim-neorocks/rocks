use reqwest::{header::ToStrError, Client};
use std::time::SystemTime;
use thiserror::Error;
use tokio::{fs, io};

use crate::config::{Config, LuaVersion};

#[derive(Error, Debug)]
pub enum ManifestFromServerError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("failed to pull manifest: {0}")]
    Request(#[from] reqwest::Error),
    #[error("invalidate date received from server: {0}")]
    InvalidDate(#[from] httpdate::Error),
    #[error("non-ASCII characters returned in response header: {0}")]
    InvalidHeader(#[from] ToStrError),
}

pub async fn manifest_from_server(
    url: String,
    config: &Config,
) -> Result<String, ManifestFromServerError> {
    let manifest_filename = "manifest".to_string()
        + &config
            .lua_version()
            .filter(|lua_version| {
                // There's no manifest-luajit
                matches!(
                    lua_version,
                    LuaVersion::Lua51 | LuaVersion::Lua52 | LuaVersion::Lua53 | LuaVersion::Lua54
                )
            })
            .or(config
                .lua_version()
                .and_then(|lua_version| match lua_version {
                    LuaVersion::LuaJIT => Some(&LuaVersion::Lua51),
                    LuaVersion::LuaJIT52 => Some(&LuaVersion::Lua52),
                    _ => None,
                }))
            .map(|s| format!("-{}", s))
            .unwrap_or_default();
    let url = url.trim_end_matches('/').to_string() + "/" + &manifest_filename;

    // Stores a path to the manifest cache (this allows us to operate on a manifest without
    // needing to pull it from the luarocks servers each time).
    let cache = config.cache_dir().join(&manifest_filename);

    // Ensure all intermediate directories for the cache file are created (e.g. `~/.cache/rocks/manifest`)
    fs::create_dir_all(cache.parent().unwrap()).await?;

    let client = Client::new();

    // Read the metadata of the local cache and attempt to get the last modified date.
    if let Ok(metadata) = fs::metadata(&cache).await {
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
                fs::write(&cache, &new_manifest_content).await?;

                return Ok(new_manifest_content);
            }

            // Else return the cached manifest.
            return Ok(fs::read_to_string(&cache).await?);
        }
    }

    // If our cache file does not exist then pull the whole manifest.

    let new_manifest = client.get(url).send().await?.text().await?;

    fs::write(&cache, &new_manifest).await?;

    Ok(new_manifest)
}

#[cfg(test)]
mod tests {
    use httptest::{matchers::*, responders::*, Expectation, Server};
    use serial_test::serial;

    use crate::config::ConfigBuilder;

    use super::*;

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
    pub async fn get_manifest_luajit() {
        let cache_dir = assert_fs::TempDir::new().unwrap().to_path_buf();
        let server = start_test_server("manifest-5.1".into());
        let mut url_str = server.url_str(""); // Remove trailing "/"
        url_str.pop();
        let config = ConfigBuilder::new()
            .cache_dir(Some(cache_dir))
            .lua_version(Some(crate::config::LuaVersion::LuaJIT))
            .build()
            .unwrap();
        manifest_from_server(url_str, &config).await.unwrap();
    }

    #[tokio::test]
    #[serial]
    pub async fn get_manifest_for_5_1() {
        let cache_dir = assert_fs::TempDir::new().unwrap().to_path_buf();
        let server = start_test_server("manifest-5.1".into());
        let mut url_str = server.url_str(""); // Remove trailing "/"
        url_str.pop();

        let config = ConfigBuilder::new()
            .cache_dir(Some(cache_dir))
            .lua_version(Some(crate::config::LuaVersion::Lua51))
            .build()
            .unwrap();

        manifest_from_server(url_str, &config).await.unwrap();
    }

    #[tokio::test]
    #[serial]
    pub async fn get_cached_manifest() {
        let server = start_test_server("manifest-5.1".into());
        let mut url_str = server.url_str(""); // Remove trailing "/"
        url_str.pop();
        let manifest_content = "dummy data";
        let cache_dir = assert_fs::TempDir::new().unwrap();
        let cache = cache_dir.join("manifest-5.1");
        fs::write(&cache, manifest_content).await.unwrap();
        let _metadata = fs::metadata(&cache).await.unwrap();
        let config = ConfigBuilder::new()
            .cache_dir(Some(cache_dir.to_path_buf()))
            .lua_version(Some(crate::config::LuaVersion::Lua51))
            .build()
            .unwrap();
        let result = manifest_from_server(url_str, &config).await.unwrap();
        assert_eq!(result, manifest_content);
    }
}
