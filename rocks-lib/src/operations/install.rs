use std::{collections::HashMap, io};

use crate::{
    build::{BuildBehaviour, BuildError},
    config::{Config, LuaVersion, LuaVersionUnset},
    lockfile::{
        LocalPackage, LocalPackageId, LocalPackageSpec, LockConstraint, Lockfile, PinnedState,
    },
    package::{PackageName, PackageReq, PackageVersionReq},
    progress::{MultiProgress, ProgressBar},
    rockspec::{LuaVersionError, Rockspec},
    tree::Tree,
};

use futures::future::join_all;
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

struct PackageInstallSpec {
    build_behaviour: BuildBehaviour,
    rockspec: Rockspec,
    local_package_spec: LocalPackageSpec,
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
    let bar = progress.add(ProgressBar::from(
        "🔭 Resolving dependencies...".to_string(),
    ));
    let all_packages = join_all(packages.iter().map(|(behaviour, pkg)| {
        flattened_install_specs(&bar, (*behaviour, pkg.clone()), &pin, config)
    }))
    .await
    .into_iter()
    .try_collect::<_, Vec<Vec<PackageInstallSpec>>, InstallError>()?
    .into_iter()
    .flatten()
    .unique_by(|install_spec| install_spec.local_package_spec.id().clone())
    .collect_vec();

    let local_package_specs = all_packages
        .iter()
        .map(|install_spec| install_spec.local_package_spec.clone())
        .collect_vec();

    let installed_packages = join_all(all_packages.into_iter().map(|install_spec| {
        let local_package_spec = install_spec.local_package_spec;
        bar.set_message(format!("💻 Installing {}", local_package_spec.to_package()));
        crate::build::build(
            &bar,
            install_spec.rockspec,
            pin,
            local_package_spec.constraint(),
            install_spec.build_behaviour,
            config,
        )
    }))
    .await
    .into_iter()
    .map(|result| result.map(|pkg| (pkg.id(), pkg)))
    .try_collect::<_, HashMap<LocalPackageId, LocalPackage>, _>()?;

    local_package_specs
        .into_iter()
        .filter_map(|spec| {
            let installed_package_opt = installed_packages.get(&spec.id());
            installed_package_opt.map(|installed_package| (installed_package, spec))
        })
        .for_each(|(pkg, spec)| {
            lockfile.add(pkg);
            spec.dependencies
                .iter()
                .filter_map(|id| installed_packages.get(id))
                .for_each(|dependency| lockfile.add_dependency(pkg, dependency))
        });

    Ok(installed_packages.into_values().collect_vec())
}

/// Get a flattened list of packages to install
async fn flattened_install_specs(
    progress: &ProgressBar,
    (build_behaviour, package): (BuildBehaviour, PackageReq),
    pin: &PinnedState,
    config: &Config,
) -> Result<Vec<PackageInstallSpec>, InstallError> {
    let constraint = if *package.version_req() == PackageVersionReq::SemVer(VersionReq::STAR) {
        LockConstraint::Unconstrained
    } else {
        LockConstraint::Constrained(package.version_req().clone())
    };
    let rockspec = super::download_rockspec(progress, &package, config).await?;
    rockspec.validate_lua_version(config)?;
    let lua_pkg = PackageName::new("lua".into());
    let dependency_install_spec = join_all(
        rockspec
            .dependencies
            .current_platform()
            .iter()
            .filter(|pkg| !pkg.name().eq(&lua_pkg))
            .map(|pkg| {
                flattened_install_specs(progress, (build_behaviour, pkg.clone()), pin, config)
            }),
    )
    .await
    .into_iter()
    .try_collect::<_, Vec<Vec<PackageInstallSpec>>, InstallError>()?
    .into_iter()
    .flatten()
    .collect_vec();

    let dependency_spec = dependency_install_spec
        .iter()
        .map(|info| info.local_package_spec.id().clone())
        .collect_vec();

    let local_package_spec = LocalPackageSpec::new(
        &rockspec.package,
        &rockspec.version,
        constraint,
        dependency_spec,
        pin,
    );
    let install_spec = PackageInstallSpec {
        build_behaviour,
        rockspec,
        local_package_spec,
    };

    Ok(dependency_install_spec
        .into_iter()
        .chain(std::iter::once(install_spec))
        .collect_vec())
}
