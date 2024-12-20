use std::sync::Arc;

use async_recursion::async_recursion;
use futures::future::join_all;
use itertools::Itertools;
use semver::VersionReq;
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    build::BuildBehaviour,
    config::Config,
    lockfile::{LocalPackageId, LocalPackageSpec, LockConstraint, PinnedState},
    manifest::ManifestMetadata,
    package::{PackageReq, PackageVersionReq},
    progress::{MultiProgress, Progress},
    rockspec::Rockspec,
};

use super::{download_rockspec, SearchAndDownloadError};

#[derive(Clone, Debug)]
pub(crate) struct PackageInstallSpec {
    pub build_behaviour: BuildBehaviour,
    pub rockspec: Rockspec,
    pub spec: LocalPackageSpec,
}

#[async_recursion]
pub(crate) async fn get_all_dependencies(
    tx: UnboundedSender<PackageInstallSpec>,
    packages: Vec<(BuildBehaviour, PackageReq)>,
    pin: PinnedState,
    manifest: Arc<ManifestMetadata>,
    config: &Config,
    progress: &Progress<MultiProgress>,
) -> Result<Vec<LocalPackageId>, SearchAndDownloadError> {
    join_all(packages.into_iter().map(|(build_behaviour, package)| {
        let config = config.clone();
        let tx = tx.clone();
        let progress = progress.clone();
        let manifest = Arc::clone(&manifest);

        tokio::spawn(async move {
            let bar = progress.map(|p| p.new_bar());

            let rockspec = download_rockspec(&package, &manifest, &config, &bar)
                .await
                .unwrap();

            let constraint =
                if *package.version_req() == PackageVersionReq::SemVer(VersionReq::STAR) {
                    LockConstraint::Unconstrained
                } else {
                    LockConstraint::Constrained(package.version_req().clone())
                };

            let dependencies = rockspec
                .dependencies
                .current_platform()
                .iter()
                .filter(|dep| !dep.name().eq(&"lua".into()))
                .map(|dep| (build_behaviour, dep.clone()))
                .collect_vec();

            let dependencies =
                get_all_dependencies(tx.clone(), dependencies, pin, manifest, &config, &progress)
                    .await?;

            let local_spec = LocalPackageSpec::new(
                &rockspec.package,
                &rockspec.version,
                constraint,
                dependencies,
                &pin,
            );

            let install_spec = PackageInstallSpec {
                build_behaviour,
                spec: local_spec.clone(),
                rockspec,
            };

            tx.send(install_spec).unwrap();

            Ok::<_, SearchAndDownloadError>(local_spec.id())
        })
    }))
    .await
    .into_iter()
    .flatten()
    .try_collect()
}
