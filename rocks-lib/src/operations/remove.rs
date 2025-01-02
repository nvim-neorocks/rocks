use std::io;
use std::sync::Arc;

use crate::config::{LuaVersion, LuaVersionUnset};
use crate::lockfile::LocalPackage;
use crate::progress::{MultiProgress, Progress, ProgressBar};
use crate::{config::Config, tree::Tree};
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
    packages: Vec<LocalPackage>,
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
        I: IntoIterator<Item = LocalPackage>,
    {
        Self {
            packages: self.packages.into_iter().chain(packages).collect_vec(),
            ..self
        }
    }

    /// Add a package to the set of packages to remove.
    pub fn package(self, package: LocalPackage) -> Self {
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
        remove(self.packages, self.config, &Arc::clone(&progress)).await
    }
}

// TODO: Remove dependencies recursively too!
async fn remove(
    packages: Vec<LocalPackage>,
    config: &Config,
    progress: &Progress<MultiProgress>,
) -> Result<(), RemoveError> {
    join_all(packages.into_iter().map(|package| {
        let _bar = progress.map(|p| {
            p.add(ProgressBar::from(format!(
                "🗑️ Removing {}@{}",
                package.name(),
                package.version()
            )))
        });

        let config = config.clone();

        tokio::spawn(remove_package(package, config))
    }))
    .await;

    Ok(())
}

async fn remove_package(package: LocalPackage, config: Config) -> Result<(), RemoveError> {
    let tree = Tree::new(config.tree().clone(), LuaVersion::from(&config)?)?;

    tree.lockfile()?.remove(&package);

    std::fs::remove_dir_all(tree.root_for(&package))?;
    tokio::fs::remove_dir_all(tree.root_for(&package)).await?;

    Ok(())
}
