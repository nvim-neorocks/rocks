use std::{collections::HashMap, io};

use crate::{
    build::{BuildBehaviour, BuildError},
    config::{Config, LuaVersion, LuaVersionUnset},
    lockfile::{LocalPackage, LocalPackageId, LockConstraint, Lockfile, PinnedState},
    operations::download_rockspec,
    package::{PackageReq, PackageVersionReq},
    progress::{MultiProgress, ProgressBar},
    rockspec::{LuaVersionError, Rockspec},
    tree::Tree,
};

use async_recursion::async_recursion;
use futures::future::join_all;
use itertools::Itertools;
use semver::VersionReq;
use thiserror::Error;
use tokio::sync::mpsc::UnboundedSender;

use super::SearchAndDownloadError;

#[derive(Error, Debug)]
#[error(transparent)]
pub enum InstallError {
    SearchAndDownloadError(#[from] SearchAndDownloadError),
    LuaVersionError(#[from] LuaVersionError),
    LuaVersionUnset(#[from] LuaVersionUnset),
    Io(#[from] io::Error),
    BuildError(#[from] BuildError),
}

#[derive(Debug)]
struct PackageInstallSpec {
    build_behaviour: BuildBehaviour,
    rockspec: Rockspec,
    constraint: LockConstraint,
}

pub async fn install(
    progress: &MultiProgress,
    packages: Vec<(BuildBehaviour, PackageReq)>,
    pin: PinnedState,
    config: &Config,
) -> Result<Vec<LocalPackage>, InstallError>
where
{
    let lua_version = LuaVersion::from(config)?;
    let tree = Tree::new(config.tree().clone(), lua_version)?;
    let mut lockfile = tree.lockfile()?;
    let result = install_impl(progress, packages, pin, config, &mut lockfile).await;
    lockfile.flush()?;
    result
}

async fn install_impl(
    progress: &MultiProgress,
    packages: Vec<(BuildBehaviour, PackageReq)>,
    pin: PinnedState,
    config: &Config,
    lockfile: &mut Lockfile,
) -> Result<Vec<LocalPackage>, InstallError> {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    get_all_dependencies(tx, progress.clone(), packages, config).await?;

    let mut all_packages = Vec::with_capacity(rx.len());

    while let Some(dep) = rx.recv().await {
        all_packages.push(dep);
    }

    let installed_packages = join_all(all_packages.into_iter().map(|install_spec| {
        let bar = progress.add(ProgressBar::from(format!(
            "💻 Installing {}",
            install_spec.rockspec.package,
        )));
        let config = config.clone();

        tokio::spawn(async move {
            let pkg = crate::build::build(
                &bar,
                install_spec.rockspec,
                pin,
                install_spec.constraint,
                install_spec.build_behaviour,
                &config,
            )
            .await?;

            bar.finish_and_clear();

            Ok::<_, BuildError>((pkg.id(), pkg))
        })
    }))
    .await
    .into_iter()
    .flatten()
    .try_collect::<_, HashMap<LocalPackageId, LocalPackage>, _>()?;

    installed_packages.iter().for_each(|(_, pkg)| {
        lockfile.add(pkg);
        pkg.dependencies()
            .iter()
            .filter_map(|id| installed_packages.get(id))
            .for_each(|dependency| lockfile.add_dependency(pkg, dependency))
    });

    Ok(installed_packages.into_values().collect_vec())
}

#[async_recursion]
async fn get_all_dependencies(
    tx: UnboundedSender<PackageInstallSpec>,
    progress: MultiProgress,
    packages: Vec<(BuildBehaviour, PackageReq)>,
    config: &Config,
) -> Result<(), SearchAndDownloadError> {
    for (build_behaviour, package) in packages {
        let config = config.clone();
        let tx = tx.clone();
        let progress = progress.clone();

        tokio::spawn(async move {
            let bar = progress.new_bar();

            let rockspec = download_rockspec(&bar, &package, &config).await.unwrap();

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

            tx.send(PackageInstallSpec {
                build_behaviour,
                rockspec,
                constraint,
            })
            .unwrap();

            get_all_dependencies(tx, progress, dependencies, &config).await?;

            Ok::<_, SearchAndDownloadError>(())
        });
    }

    Ok(())
}
