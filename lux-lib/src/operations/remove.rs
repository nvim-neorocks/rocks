use std::io;
use std::sync::Arc;

use crate::config::{LuaVersion, LuaVersionUnset};
use crate::lockfile::{LocalPackage, LocalPackageId};
use crate::progress::{MultiProgress, Progress, ProgressBar};
use crate::{config::Config, tree::Tree};
use clean_path::Clean;
use futures::future::join_all;
use itertools::Itertools;
use thiserror::Error;

#[derive(Error, Debug)]
#[error(transparent)]
pub enum RemoveError {
    LuaVersionUnset(#[from] LuaVersionUnset),
    Io(#[from] io::Error),
}

pub struct Remove<'a> {
    config: &'a Config,
    packages: Vec<LocalPackageId>,
    progress: Option<Arc<Progress<MultiProgress>>>,
}

/// A rocks package remover.
/// Can remove multiple packages in parallel.
impl<'a> Remove<'a> {
    /// Construct a new rocks package remover.
    pub fn new(config: &'a Config) -> Self {
        Self {
            config,
            packages: Vec::new(),
            progress: None,
        }
    }

    /// Add packages to remove.
    pub fn packages<I>(self, packages: I) -> Self
    where
        I: IntoIterator<Item = LocalPackageId>,
    {
        Self {
            packages: self.packages.into_iter().chain(packages).collect_vec(),
            ..self
        }
    }

    /// Add a package to the set of packages to remove.
    pub fn package(self, package: LocalPackageId) -> Self {
        self.packages(std::iter::once(package))
    }

    /// Pass a `MultiProgress` to this installer.
    /// By default, a new one will be created.
    pub fn progress(self, progress: Arc<Progress<MultiProgress>>) -> Self {
        Self {
            progress: Some(progress),
            ..self
        }
    }

    /// Remove the packages.
    pub async fn remove(self) -> Result<(), RemoveError> {
        let progress = match self.progress {
            Some(p) => p,
            None => MultiProgress::new_arc(),
        };
        let tree = self.config.tree(LuaVersion::from(self.config)?)?;
        remove(self.packages, tree, &Arc::clone(&progress)).await
    }
}

// TODO: Remove dependencies recursively too!
async fn remove(
    package_ids: Vec<LocalPackageId>,
    tree: Tree,
    progress: &Progress<MultiProgress>,
) -> Result<(), RemoveError> {
    let lockfile = tree.lockfile()?;

    let packages = package_ids
        .iter()
        .filter_map(|id| lockfile.get(id))
        .cloned()
        .collect_vec();

    join_all(packages.into_iter().map(|package| {
        let bar = progress.map(|p| p.new_bar());

        let tree = tree.clone();
        tokio::spawn(remove_package(package, tree, bar))
    }))
    .await;

    lockfile.map_then_flush(|lockfile| {
        package_ids
            .iter()
            .for_each(|package| lockfile.remove_by_id(package));

        Ok::<_, io::Error>(())
    })?;

    Ok(())
}

async fn remove_package(
    package: LocalPackage,
    tree: Tree,
    bar: Progress<ProgressBar>,
) -> Result<(), RemoveError> {
    bar.map(|p| {
        p.set_message(format!(
            "🗑️ Removing {}@{}",
            package.name(),
            package.version()
        ))
    });

    tokio::fs::remove_dir_all(tree.root_for(&package)).await?;

    // Delete the corresponding binaries attached to the current package (located under `{LUX_TREE}/bin/`)
    for relative_binary_path in package.spec.binaries() {
        let binary_path = tree.bin().join(
            relative_binary_path
                .clean()
                .file_name()
                .expect("malformed lockfile"),
        );

        if binary_path.is_file() {
            tokio::fs::remove_file(binary_path).await?;
        }
    }

    bar.map(|p| p.finish_and_clear());
    Ok(())
}
