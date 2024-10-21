use crate::{
    config::{Config, DefaultFromConfig},
    lockfile::{LocalPackage, LockConstraint},
    package::{PackageName, PackageReq},
    progress::with_spinner,
    tree::Tree,
};

use async_recursion::async_recursion;
use eyre::Result;
use indicatif::{MultiProgress, ProgressBar};
use itertools::Itertools;

#[async_recursion]
pub async fn install(
    progress: &MultiProgress,
    package_req: PackageReq,
    pin: bool,
    config: &Config,
) -> Result<LocalPackage> {
    with_spinner(
        progress,
        format!("💻 Installing {}", package_req),
        || async { install_impl(progress, package_req, pin, config).await },
    )
    .await
}

async fn install_impl(
    progress: &MultiProgress,
    package_req: PackageReq,
    pin: bool,
    config: &Config,
) -> Result<LocalPackage> {
    let rockspec = super::download_rockspec(progress, &package_req, config).await?;

    let lua_version = rockspec.lua_version().or_default_from(config)?;

    let tree = Tree::new(config.tree().clone(), lua_version)?;
    let mut lockfile = tree.lockfile()?;

    let constraint = LockConstraint::Constrained(package_req.version_req().clone());

    // Recursively build all dependencies.
    let dependencies = rockspec
        .dependencies
        .current_platform()
        .iter()
        .filter(|package| !package.name().eq(&PackageName::new("lua".into())))
        .collect_vec();
    let bar = progress
        .add(ProgressBar::new(dependencies.len() as u64))
        .with_message("Installing dependencies...");
    let mut installed_dependencies = Vec::new();
    for (index, dependency_req) in dependencies
        .into_iter()
        .filter(|req| tree.has_rock(req).is_none())
        .enumerate()
    {
        let dependency =
            crate::operations::install(progress, dependency_req.clone(), pin, config).await?;

        installed_dependencies.push(dependency);
        bar.set_position(index as u64);
    }

    let package = crate::build::build(progress, rockspec, pin, constraint, config).await?;

    lockfile.add(&package);
    for dependency in installed_dependencies {
        lockfile.add_dependency(&package, &dependency);
    }

    lockfile.flush()?;

    Ok(package)
}
