use anyhow::Result;
use reqwest::Client;
use std::{fs, path::PathBuf};

// TODO(vhyrro): Perhaps cache the manifest somewhere on disk?
pub async fn manifest_from_server(url: String, lua_version: Option<&String>, cache: Option<PathBuf>) -> Result<String> {
    let manifest_filename = "manifest".to_string() + &lua_version.map(|s| format!("-{}", s)).unwrap_or_default();
    let url = url + "/" + &manifest_filename;

    // Stores a path to the manifest cache (this allows us to operate on a manifest without
    // needing to pull it from the luarocks servers each time).
    let cache = cache.unwrap_or_else(|| directories::ProjectDirs::from("org", "neorocks", "rocks").unwrap().cache_dir().join(&manifest_filename));

    // Ensure all intermediate directories for the cache file are created (e.g. `~/.cache/rocks/manifest`)
    fs::create_dir_all(cache.parent().unwrap())?;

    let client = Client::new();

    // Read the metadata of the local cache and attempt to get the last modified date.
    if let Ok(metadata) = fs::metadata(&cache) {
        let last_modified_local = metadata.modified()?;

        // Ask the server for the last modified date of its manifest.
        let request = client.head(&url).send().await?;

        if let Some(last_modified) = request.headers().get("Last-Modified") {
            let server_last_modified = httpdate::parse_http_date(last_modified.to_str()?)?;

            // If the server's version of the manifest is newer than ours then update out manifest.
            if server_last_modified > last_modified_local {
                let new_manifest = request.text().await?;
                fs::write(&cache, &new_manifest)?;

                return Ok(new_manifest);
            }

            // Else return the cached manifest.
            return Ok(String::from_utf8(fs::read(&cache)?)?);
        }
    }

    // If our cache file does not exist then pull the whole manifest.

    let new_manifest = 
        reqwest::get(url)
            .await?
            .text()
            .await?;

    fs::write(&cache, &new_manifest)?;

    Ok(new_manifest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    pub async fn get_manifest() {
        manifest_from_server("https://luarocks.org".into(), None, None)
            .await
            .unwrap();
    }

    #[tokio::test]
    pub async fn get_manifest_for_5_1() {
        manifest_from_server("https://luarocks.org".into(), Some(&"5.1".into()), None)
            .await
            .unwrap();
    }
}
