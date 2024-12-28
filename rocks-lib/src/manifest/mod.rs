use itertools::Itertools;
use mlua::{Lua, LuaSerdeExt};
use reqwest::{header::ToStrError, Client};
use std::collections::HashMap;
use std::time::SystemTime;
use thiserror::Error;
use tokio::{fs, io};

use crate::{
    config::{Config, LuaVersion},
    package::{PackageName, PackageReq, PackageSpec, PackageVersion, RemotePackage},
};

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

async fn manifest_from_server(
    url: &str,
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

#[derive(Clone)]
pub(crate) struct ManifestMetadata {
    pub repository: HashMap<PackageName, HashMap<PackageVersion, Vec<ManifestRockEntry>>>,
}

impl<'de> serde::Deserialize<'de> for ManifestMetadata {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let intermediate = IntermediateManifest::deserialize(deserializer)?;
        Ok(from_intermediate(intermediate))
    }
}

#[derive(Error, Debug)]
#[error("failed to parse manifest: {0}")]
pub struct ManifestLuaError(#[from] mlua::Error);

#[derive(Error, Debug)]
#[error("failed to parse manifest from configuration: {0}")]
pub enum ManifestError {
    Lua(#[from] ManifestLuaError),
    Server(#[from] ManifestFromServerError),
}

impl ManifestMetadata {
    pub fn new(manifest: &String) -> Result<Self, ManifestLuaError> {
        let lua = Lua::new();

        lua.load(manifest).exec()?;

        let intermediate = IntermediateManifest {
            repository: lua.from_value(lua.globals().get("repository")?)?,
        };
        let manifest = from_intermediate(intermediate);

        Ok(manifest)
    }

    pub fn has_rock(&self, rock_name: &PackageName) -> bool {
        self.repository.contains_key(rock_name)
    }

    pub fn latest_version(&self, rock_name: &PackageName) -> Option<&PackageVersion> {
        if !self.has_rock(rock_name) {
            return None;
        }

        self.repository[rock_name].keys().sorted().last()
    }

    pub fn latest_match(&self, lua_package_req: &PackageReq) -> Option<PackageSpec> {
        if !self.has_rock(lua_package_req.name()) {
            return None;
        }

        let version = self.repository[lua_package_req.name()]
            .keys()
            .sorted()
            .rev()
            .find(|version| lua_package_req.version_req().matches(version))?;

        Some(PackageSpec::new(
            lua_package_req.name().to_owned(),
            version.to_owned(),
        ))
    }
}

#[derive(Clone)]
pub(crate) struct Manifest {
    server_url: String,
    metadata: ManifestMetadata,
}

impl Manifest {
    pub fn new(server_url: &str, metadata: ManifestMetadata) -> Self {
        Self {
            server_url: server_url.into(),
            metadata,
        }
    }

    pub async fn from_config(server_url: &str, config: &Config) -> Result<Self, ManifestError> {
        let manifest = crate::manifest::manifest_from_server(server_url, config).await?;
        let metadata = ManifestMetadata::new(&manifest)?;
        Ok(Self::new(server_url, metadata))
    }
    pub fn server_url(&self) -> &String {
        &self.server_url
    }
    pub fn metadata(&self) -> &ManifestMetadata {
        &self.metadata
    }
    pub fn search(&self, package_req: &PackageReq) -> Option<RemotePackage> {
        if !self.metadata().has_rock(package_req.name()) {
            None
        } else {
            Some(RemotePackage {
                package: self.metadata().latest_match(package_req).unwrap(),
                server_url: self.server_url().into(),
            })
        }
    }
}

#[derive(Clone, serde::Deserialize)]
pub struct ManifestRockEntry {
    /// e.g. "linux-x86_64", "rockspec", "src", ...
    pub arch: String,
}

/// Intermediate implementation for deserializing
#[derive(serde::Deserialize)]
struct IntermediateManifest {
    /// The key of each package's HashMap is the version string
    repository: HashMap<PackageName, HashMap<String, Vec<ManifestRockEntry>>>,
}

/// Construct a `ManifestMetadata` from an intermediate representation,
/// silently skipping entries for versions we don't know how to parse.
fn from_intermediate(intermediate: IntermediateManifest) -> ManifestMetadata {
    let repository = intermediate
        .repository
        .into_iter()
        .map(|(name, package_map)| {
            (
                name,
                package_map
                    .into_iter()
                    .filter_map(|(version_str, entries)| {
                        let version = PackageVersion::parse(version_str.as_str()).ok()?;
                        Some((version, entries))
                    })
                    .collect(),
            )
        })
        .collect();
    ManifestMetadata { repository }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use httptest::{matchers::request, responders::status_code, Expectation, Server};
    use serial_test::serial;

    use crate::{config::ConfigBuilder, package::PackageReq};

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
        manifest_from_server(&url_str, &config).await.unwrap();
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

        manifest_from_server(&url_str, &config).await.unwrap();
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
        let result = manifest_from_server(&url_str, &config).await.unwrap();
        assert_eq!(result, manifest_content);
    }

    #[tokio::test]
    pub async fn parse_metadata_from_empty_manifest() {
        let manifest = "
            commands = {}\n
            modules = {}\n
            repository = {}\n
            "
        .to_string();
        ManifestMetadata::new(&manifest).unwrap();
    }

    #[tokio::test]
    pub async fn parse_metadata_from_test_manifest() {
        let mut test_manifest_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        test_manifest_path.push("resources/test/manifest-5.1");
        let manifest = String::from_utf8(fs::read(&test_manifest_path).await.unwrap()).unwrap();
        ManifestMetadata::new(&manifest).unwrap();
    }

    #[tokio::test]
    pub async fn latest_match_regression() {
        let mut test_manifest_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        test_manifest_path.push("resources/test/manifest-5.1");
        let manifest = String::from_utf8(fs::read(&test_manifest_path).await.unwrap()).unwrap();
        let metadata = ManifestMetadata::new(&manifest).unwrap();

        let package_req: PackageReq = "30log > 1.3.0".parse().unwrap();
        assert!(metadata.latest_match(&package_req).is_none());
    }
}
