use crate::{
    config::{Config, DefaultFromConfig},
    lockfile::{LocalPackage, LockConstraint},
    progress::with_spinner,
    remote_package::{PackageReq, RemotePackage},
    tree::Tree,
};

use async_recursion::async_recursion;
use eyre::Result;
use indicatif::MultiProgress;

#[async_recursion]
pub async fn install(
    progress: &MultiProgress,
    package_req: PackageReq,
    config: &Config,
) -> Result<LocalPackage> {
    with_spinner(
        progress,
        format!("💻 Installing {}", package_req),
        || async { install_impl(progress, package_req, config).await },
    )
    .await
}

async fn install_impl(
    progress: &MultiProgress,
    package_req: PackageReq,
    config: &Config,
) -> Result<LocalPackage> {
    let rockspec = super::download_rockspec(progress, &package_req, config).await?;

    let lua_version = rockspec.lua_version().or_default_from(config)?;

    let tree = Tree::new(config.tree().clone(), lua_version)?;
    let mut lockfile = tree.lockfile()?;

    let constraint = LockConstraint::Constrained(package_req.version_req().clone());
    let pinned = false;

    let package = lockfile.add(
        &RemotePackage::new(rockspec.package.clone(), rockspec.version.clone()),
        constraint.clone(),
        pinned,
    );

    // Recursively build all dependencies.
    // TODO: Handle regular dependencies as well.
    for dependency_req in rockspec
        .build_dependencies
        .current_platform()
        .iter()
        .filter(|req| tree.has_rock(req).is_none())
    {
        let dependency =
            crate::operations::install(progress, dependency_req.clone(), config).await?;

        lockfile.add_dependency(&package, &dependency);
    }

    crate::build::build(progress, rockspec, pinned, constraint, config).await?;

    lockfile.flush()?;

    Ok(package)
}
