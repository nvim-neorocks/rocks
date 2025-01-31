use std::sync::Arc;

use async_recursion::async_recursion;
use futures::future::join_all;
use itertools::Itertools;
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    build::BuildBehaviour,
    config::Config,
    lockfile::{
        LocalPackageId, LocalPackageSpec, LockConstraint, Lockfile, LockfilePermissions,
        PinnedState,
    },
    package::PackageReq,
    progress::{MultiProgress, Progress},
    remote_package_db::RemotePackageDB,
    rockspec::Rockspec,
};

use super::{Download, RemoteRockDownload, SearchAndDownloadError};

#[derive(Clone, Debug)]
pub(crate) struct PackageInstallSpec {
    pub build_behaviour: BuildBehaviour,
    pub downloaded_rock: RemoteRockDownload,
    pub spec: LocalPackageSpec,
}

#[async_recursion]
pub(crate) async fn get_all_dependencies<P>(
    tx: UnboundedSender<PackageInstallSpec>,
    packages: Vec<(BuildBehaviour, PackageReq)>,
    pin: PinnedState,
    package_db: Arc<RemotePackageDB>,
    lockfile: Arc<Lockfile<P>>,
    config: &Config,
    progress: Arc<Progress<MultiProgress>>,
) -> Result<Vec<LocalPackageId>, SearchAndDownloadError>
where
    P: LockfilePermissions + Send + Sync + 'static,
{
    join_all(
        packages
            .into_iter()
            // Exclude packages that are already installed
            .filter(|(build_behaviour, package)| {
                build_behaviour == &BuildBehaviour::Force
                    || lockfile.has_rock(package, None).is_none()
            })
            .map(|(build_behaviour, package)| {
                let config = config.clone();
                let tx = tx.clone();
                let package_db = Arc::clone(&package_db);
                let progress = Arc::clone(&progress);
                let lockfile = Arc::clone(&lockfile);

                tokio::spawn(async move {
                    let bar = progress.map(|p| p.new_bar());

                    let downloaded_rock = Download::new(&package, &config, &bar)
                        .package_db(&package_db)
                        .download_remote_rock()
                        .await?;

                    let constraint = if let Some(package_req) = package.version_req() {
                        LockConstraint::Constrained(package_req.clone())
                    } else {
                        LockConstraint::Unconstrained
                    };

                    let dependencies = downloaded_rock
                        .rockspec()
                        .dependencies()
                        .current_platform()
                        .iter()
                        .filter(|dep| !dep.name().eq(&"lua".into()))
                        .map(|dep| (build_behaviour, dep.clone()))
                        .collect_vec();

                    let dependencies = get_all_dependencies(
                        tx.clone(),
                        dependencies,
                        pin,
                        package_db,
                        lockfile,
                        &config,
                        progress,
                    )
                    .await?;

                    let rockspec = downloaded_rock.rockspec();
                    let local_spec = LocalPackageSpec::new(
                        rockspec.package(),
                        rockspec.version(),
                        constraint,
                        dependencies,
                        &pin,
                        rockspec.binaries(),
                    );

                    let install_spec = PackageInstallSpec {
                        build_behaviour,
                        spec: local_spec.clone(),
                        downloaded_rock,
                    };

                    tx.send(install_spec).unwrap();

                    Ok::<_, SearchAndDownloadError>(local_spec.id())
                })
            }),
    )
    .await
    .into_iter()
    .flatten()
    .try_collect()
}
