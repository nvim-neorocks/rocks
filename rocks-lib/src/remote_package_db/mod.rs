use crate::{
    config::Config,
    manifest::{Manifest, ManifestError},
    package::{PackageName, PackageReq, PackageSpec, PackageVersion, RemotePackage},
    progress::{Progress, ProgressBar},
};
use itertools::Itertools as _;
use thiserror::Error;

#[derive(Clone)]
pub struct RemotePackageDB(Vec<Manifest>);

#[derive(Error, Debug)]
pub enum RemotePackageDBError {
    #[error(transparent)]
    ManifestError(#[from] ManifestError),
}

#[derive(Error, Debug)]
pub enum SearchError {
    #[error(transparent)]
    Mlua(#[from] mlua::Error),
    #[error("no rock that matches '{0}' found")]
    RockNotFound(PackageReq),
    #[error("error when pulling manifest: {0}")]
    Manifest(#[from] ManifestError),
}

impl RemotePackageDB {
    pub async fn from_config(config: &Config) -> Result<Self, RemotePackageDBError> {
        let mut manifests = Vec::new();
        for server in config.extra_servers() {
            let manifest = Manifest::from_config(server.clone(), config).await?;
            manifests.push(manifest);
        }
        manifests.push(Manifest::from_config(config.server().clone(), config).await?);
        Ok(Self(manifests))
    }

    /// Find a package that matches the requirement
    pub(crate) fn find(
        &self,
        package_req: &PackageReq,
        progress: &Progress<ProgressBar>,
    ) -> Result<RemotePackage, SearchError> {
        let result = self.0.iter().find_map(|manifest| {
            progress.map(|p| p.set_message(format!("🔎 Searching {}", &manifest.server_url())));
            manifest.search(package_req)
        });
        match result {
            Some(package) => Ok(package),
            None => Err(SearchError::RockNotFound(package_req.clone())),
        }
    }

    /// Search for all packages that match the requirement
    pub fn search(&self, package_req: &PackageReq) -> Vec<(&PackageName, Vec<&PackageVersion>)> {
        self.0
            .iter()
            .flat_map(|manifest| {
                manifest
                    .metadata()
                    .repository
                    .iter()
                    .filter_map(|(name, elements)| {
                        if name.to_string().contains(&package_req.name().to_string()) {
                            Some((
                                name,
                                elements
                                    .keys()
                                    .filter(|version| package_req.version_req().matches(version))
                                    .sorted_by(|a, b| Ord::cmp(b, a))
                                    .collect_vec(),
                            ))
                        } else {
                            None
                        }
                    })
            })
            .collect()
    }

    pub fn latest_version(&self, rock_name: &PackageName) -> Option<&PackageVersion> {
        self.0
            .iter()
            .filter_map(|manifest| manifest.metadata().latest_version(rock_name))
            .sorted()
            .last()
    }

    pub fn latest_match(&self, package_req: &PackageReq) -> Option<PackageSpec> {
        self.0
            .iter()
            .filter_map(|manifest| manifest.metadata().latest_match(package_req))
            .last()
    }
}

impl From<Manifest> for RemotePackageDB {
    fn from(manifest: Manifest) -> Self {
        RemotePackageDB(vec![manifest])
    }
}
