use bon::Builder;
use futures::future::join_all;
use itertools::Itertools;
use std::{collections::HashMap, io, sync::Arc};
use thiserror::Error;

use crate::{
    build::BuildBehaviour,
    config::Config,
    hash::HasIntegrity,
    lockfile::{LocalPackage, LocalPackageHashes, LocalPackageId, Lockfile, PinnedState, ReadOnly},
    operations::get_all_dependencies,
    package::{PackageReq, PackageSpec},
    progress::{MultiProgress, Progress},
    remote_package_db::{RemotePackageDB, RemotePackageDBError},
    rockspec::Rockspec,
};

use super::{FetchSrc, FetchSrcError, SearchAndDownloadError};

/// A rocks lockfile updater.
#[derive(Builder)]
#[builder(start_fn = new, finish_fn(name = _update, vis = ""))]
pub struct LockfileUpdate<'a> {
    #[builder(start_fn)]
    lockfile: &'a mut Lockfile<ReadOnly>,

    #[builder(start_fn)]
    config: &'a Config,

    #[builder(field)]
    packages: Vec<PackageReq>,

    package_db: Option<RemotePackageDB>,

    #[builder(default)]
    pin: PinnedState,

    #[builder(default = MultiProgress::new_arc())]
    progress: Arc<Progress<MultiProgress>>,
}

impl<State: lockfile_update_builder::State> LockfileUpdateBuilder<'_, State> {
    pub fn package(mut self, package: PackageReq) -> Self {
        self.packages.push(package);
        self
    }

    pub fn packages(mut self, packages: impl IntoIterator<Item = PackageReq>) -> Self {
        self.packages.extend(packages);
        self
    }

    /// Add packages that are not already present in the lockfile to the lockfile
    /// This downloads the RockSpecs and sources to determine their hashes.
    pub async fn add_missing_packages(self) -> Result<(), LockfileUpdateError> {
        do_add_missing_packages(self._update()).await
    }
}

#[derive(Error, Debug)]
pub enum LockfileUpdateError {
    #[error("error initialising remote package DB: {0}")]
    RemotePackageDB(#[from] RemotePackageDBError),
    #[error(transparent)]
    SearchAndDownload(#[from] SearchAndDownloadError),
    #[error("failed to fetch rock source: {0}")]
    FetchSrcError(#[from] FetchSrcError),
    #[error(transparent)]
    Io(#[from] io::Error),
}

async fn do_add_missing_packages(update: LockfileUpdate<'_>) -> Result<(), LockfileUpdateError> {
    let bar = update.progress.map(|p| p.new_bar());
    let package_db = match update.package_db {
        Some(db) => db,
        None => RemotePackageDB::from_config(update.config, &bar).await?,
    };
    let lockfile = update.lockfile;
    let packages_to_add = update
        .packages
        .iter()
        .filter(|pkg| lockfile.has_rock(pkg, None).is_none())
        .map(|pkg| (BuildBehaviour::NoForce, pkg.clone()))
        .collect_vec();

    if packages_to_add.is_empty() {
        return Ok(());
    }

    bar.map(|b| b.set_message("🔐 Updating lockfile..."));

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    get_all_dependencies(
        tx,
        packages_to_add,
        update.pin,
        Arc::new(package_db),
        Arc::new(lockfile.clone()),
        update.config,
        update.progress.clone(),
    )
    .await?;

    let mut all_packages = HashMap::with_capacity(rx.len());
    while let Some(dep) = rx.recv().await {
        all_packages.insert(dep.spec.id(), dep);
    }
    let packages_to_add = join_all(all_packages.clone().into_values().map(|install_spec| {
        let config = update.config.clone();
        tokio::spawn(async move {
            let downloaded_rock = install_spec.downloaded_rock;
            let rockspec = downloaded_rock.rockspec();
            let temp_dir =
                tempdir::TempDir::new(&format!("lockfile_update-{}", &rockspec.package()))?;
            let source_hash =
                FetchSrc::new(temp_dir.path(), rockspec, &config, &Progress::NoProgress)
                    .fetch()
                    .await?;
            let hashes = LocalPackageHashes {
                rockspec: rockspec.hash()?,
                source: source_hash,
            };
            let pkg = LocalPackage::from(
                &PackageSpec::new(rockspec.package().clone(), rockspec.version().clone()),
                install_spec.spec.constraint(),
                rockspec.binaries(),
                downloaded_rock.rockspec_download().source.clone(),
                hashes,
            );
            Ok::<_, LockfileUpdateError>((pkg.id(), pkg))
        })
    }))
    .await
    .into_iter()
    .flatten()
    .try_collect::<_, HashMap<LocalPackageId, LocalPackage>, _>()?;

    lockfile.map_then_flush(|lockfile| {
        packages_to_add.iter().for_each(|(id, pkg)| {
            lockfile.add(pkg);

            all_packages
                .get(id)
                .map(|pkg| pkg.spec.dependencies())
                .unwrap_or_default()
                .into_iter()
                .for_each(|dependency_id| {
                    if let Some(dependency) = packages_to_add.get(dependency_id) {
                        lockfile.add_dependency(pkg, dependency)
                    }
                });
        });

        Ok::<_, io::Error>(())
    })?;

    bar.map(|b| b.finish_and_clear());

    Ok(())
}
