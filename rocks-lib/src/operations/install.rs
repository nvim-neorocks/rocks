use std::io;

use crate::{
    build::{BuildBehaviour, BuildError},
    config::{Config, LuaVersion, LuaVersionUnset},
    lockfile::{LocalPackage, LockConstraint, Lockfile, PinnedState},
    package::{PackageName, PackageReq, PackageVersionReq},
    progress::with_spinner,
    rockspec::LuaVersionError,
    tree::Tree,
};

use async_recursion::async_recursion;
use indicatif::{MultiProgress, ProgressBar};
use itertools::Itertools;
use semver::VersionReq;
use thiserror::Error;

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

pub async fn install(
    progress: &MultiProgress,
    package_req: PackageReq,
    pin: PinnedState,
    build_behaviour: BuildBehaviour,
    config: &Config,
) -> Result<LocalPackage, InstallError> {
    let lua_version = LuaVersion::from(config)?;
    let tree = Tree::new(config.tree().clone(), lua_version)?;
    let mut lockfile = tree.lockfile()?;
    let result = install_impl(
        progress,
        package_req,
        pin,
        build_behaviour,
        config,
        &mut lockfile,
    )
    .await;
    lockfile.flush()?;
    result
}

#[async_recursion]
async fn install_impl(
    progress: &MultiProgress,
    package_req: PackageReq,
    pin: PinnedState,
    build_behaviour: BuildBehaviour,
    config: &Config,
    lockfile: &mut Lockfile,
) -> Result<LocalPackage, InstallError> {
    with_spinner(
        progress,
        format!("💻 Installing {}", package_req),
        || async {
            go(
                progress,
                package_req,
                pin,
                build_behaviour,
                config,
                lockfile,
            )
            .await
        },
    )
    .await
}

async fn go(
    progress: &MultiProgress,
    package_req: PackageReq,
    pin: PinnedState,
    build_behaviour: BuildBehaviour,
    config: &Config,
    lockfile: &mut Lockfile,
) -> Result<LocalPackage, InstallError> {
    let rockspec = super::download_rockspec(progress, &package_req, config).await?;

    let lua_version = rockspec.lua_version_from_config(config)?;

    let tree = Tree::new(config.tree().clone(), lua_version)?;

    let constraint = if *package_req.version_req() == PackageVersionReq::SemVer(VersionReq::STAR) {
        LockConstraint::Unconstrained
    } else {
        LockConstraint::Constrained(package_req.version_req().clone())
    };

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
        let dependency = install_impl(
            progress,
            dependency_req.clone(),
            pin,
            build_behaviour,
            config,
            lockfile,
        )
        .await?;

        installed_dependencies.push(dependency);
        bar.set_position(index as u64);
    }

    let package =
        crate::build::build(progress, rockspec, pin, constraint, build_behaviour, config).await?;

    lockfile.add(&package);
    for dependency in installed_dependencies {
        lockfile.add_dependency(&package, &dependency);
    }

    Ok(package)
}
