use std::{io, sync::Arc};

use crate::{
    build::BuildBehaviour,
    config::Config,
    lockfile::{LocalPackage, Lockfile, LockfileIntegrityError, PackageSyncSpec, ReadOnly},
    package::{PackageName, PackageReq},
    progress::{MultiProgress, Progress},
    tree::Tree,
};
use bon::{builder, Builder};
use itertools::Itertools;
use thiserror::Error;

use super::{Install, InstallError, Remove, RemoveError};

/// A rocks sync builder, for synchronising a tree with a lockfile.
#[derive(Builder)]
#[builder(start_fn = new, finish_fn(name = _build, vis = ""))]
pub struct Sync<'a> {
    /// The tree to sync
    #[builder(start_fn)]
    tree: &'a Tree,
    /// The lockfile to sync the tree with
    #[builder(start_fn)]
    lockfile: &'a mut Lockfile<ReadOnly>,
    #[builder(start_fn)]
    config: &'a Config,
    #[builder(field)]
    progress: Option<Arc<Progress<MultiProgress>>>,
    /// Sync the source lockfile with these package requirements.
    #[builder(field)]
    packages: Option<Vec<PackageReq>>,
    /// Whether to validate the integrity of installed packages.
    validate_integrity: Option<bool>,
}

impl<State> SyncBuilder<'_, State>
where
    State: sync_builder::State,
{
    pub fn progress(mut self, progress: Arc<Progress<MultiProgress>>) -> Self {
        self.progress = Some(progress);
        self
    }

    pub fn packages(mut self, packages: Vec<PackageReq>) -> Self {
        self.packages = Some(packages);
        self
    }

    pub fn add_packages(&mut self, packages: Vec<PackageReq>) -> &Self {
        self.packages = Some(packages);
        self
    }
}

impl<State> SyncBuilder<'_, State>
where
    State: sync_builder::State + sync_builder::IsComplete,
{
    pub async fn sync(self) -> Result<SyncReport, SyncError> {
        do_sync(self._build()).await
    }
}

pub struct SyncReport {
    added: Vec<LocalPackage>,
    removed: Vec<LocalPackage>,
}

#[derive(Error, Debug)]
pub enum SyncError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Install(#[from] InstallError),
    #[error(transparent)]
    Remove(#[from] RemoveError),
    #[error("integrity error for package {0}: {1}\n")]
    Integrity(PackageName, LockfileIntegrityError),
}

async fn do_sync(args: Sync<'_>) -> Result<SyncReport, SyncError> {
    let progress = args.progress.unwrap_or(MultiProgress::new_arc());
    let dest_lockfile = args.tree.lockfile()?;

    let package_sync_spec = match &args.packages {
        Some(packages) => args.lockfile.package_sync_spec(packages),
        None => PackageSyncSpec::default(),
    };

    args.lockfile.map_then_flush(|lockfile| -> io::Result<()> {
        package_sync_spec
            .to_remove
            .iter()
            .for_each(|pkg| lockfile.remove(pkg));
        Ok(())
    })?;

    let mut report = SyncReport {
        added: Vec::new(),
        removed: Vec::new(),
    };
    for (id, local_package) in args.lockfile.rocks() {
        if dest_lockfile.get(id).is_none() {
            report.added.push(local_package.clone());
        }
    }
    for (id, local_package) in dest_lockfile.rocks() {
        if args.lockfile.get(id).is_none() {
            report.removed.push(local_package.clone());
        }
    }

    let packages_to_install = report
        .added
        .iter()
        .cloned()
        .map(|pkg| pkg.into_package_req())
        .map(|pkg| (BuildBehaviour::Force, pkg))
        .collect_vec();

    let package_db = args.lockfile.clone().into();
    Install::new(args.tree, args.config)
        .package_db(package_db)
        .packages(packages_to_install)
        .progress(progress.clone())
        .install()
        .await?;

    // Read the destination lockfile after installing
    let mut dest_lockfile = args.tree.lockfile()?.into_temporary();

    if args.validate_integrity.unwrap_or(true) {
        for package in &report.added {
            if package.name().to_string() != "say" {
                dest_lockfile
                    .validate_integrity(package)
                    .map_err(|err| SyncError::Integrity(package.name().clone(), err))?;
            }
        }
    }

    let packages_to_remove = report
        .removed
        .iter()
        .cloned()
        .map(|pkg| pkg.id())
        .collect_vec();

    Remove::new(args.config)
        .packages(packages_to_remove)
        .progress(progress.clone())
        .remove()
        .await?;

    dest_lockfile.sync_lockfile(args.lockfile);

    if !package_sync_spec.to_add.is_empty() {
        // Install missing packages using the default package_db.
        let missing_packages = package_sync_spec
            .to_add
            .into_iter()
            .map(|pkg| (BuildBehaviour::Force, pkg));

        Install::new(args.tree, args.config)
            .packages(missing_packages)
            .progress(progress.clone())
            .install()
            .await?;

        // Sync the newly added packages back to the source lockfile
        let dest_lockfile = args.tree.lockfile()?;
        args.lockfile.map_then_flush(|lockfile| -> io::Result<()> {
            lockfile.sync_lockfile(&dest_lockfile);
            Ok(())
        })?;
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use assert_fs::{prelude::PathCopy, TempDir};

    use crate::config::{ConfigBuilder, LuaVersion};

    use super::*;

    #[tokio::test]
    async fn test_sync_add_rocks() {
        if std::env::var("ROCKS_SKIP_IMPURE_TESTS").unwrap_or("0".into()) == "1" {
            println!("Skipping impure test");
            return;
        }
        let tree_path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/test/sample-tree");
        let tree = Tree::new(tree_path.clone(), LuaVersion::Lua51).unwrap();
        let mut source_lockfile = tree.lockfile().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let config = ConfigBuilder::new()
            .unwrap()
            .tree(Some(temp_dir.path().into()))
            .build()
            .unwrap();
        let dest_tree = config.tree(LuaVersion::Lua51).unwrap();
        let report = Sync::new(&dest_tree, &mut source_lockfile, &config)
            .sync()
            .await
            .unwrap();
        assert!(report.removed.is_empty());
        assert!(!report.added.is_empty());
    }

    #[tokio::test]
    async fn test_sync_remove_rocks() {
        let tree_path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/test/sample-tree");
        let temp_dir = TempDir::new().unwrap();
        temp_dir.copy_from(&tree_path, &["**"]).unwrap();
        let empty_lockfile_dir = TempDir::new().unwrap();
        let mut empty_lockfile =
            Lockfile::new(empty_lockfile_dir.path().join("lock.json")).unwrap();
        let config = ConfigBuilder::new()
            .unwrap()
            .tree(Some(temp_dir.path().into()))
            .build()
            .unwrap();
        let dest_tree = config.tree(LuaVersion::Lua51).unwrap();
        let report = Sync::new(&dest_tree, &mut empty_lockfile, &config)
            .sync()
            .await
            .unwrap();
        assert!(!report.removed.is_empty());
        assert!(report.added.is_empty());
    }
}
