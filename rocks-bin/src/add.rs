use eyre::{OptionExt, Result};
use rocks_lib::{
    config::{Config, LuaVersion},
    lockfile::PinnedState,
    operations,
    package::PackageReq,
    progress::{MultiProgress, Progress, ProgressBar},
    project::Project,
    remote_package_db::RemotePackageDB,
};

use crate::utils::install::apply_build_behaviour;

#[derive(clap::Args)]
pub struct Add {
    /// Package or list of packages to install.
    package_req: Vec<PackageReq>,

    /// Pin the package so that it doesn't get updated.
    #[arg(long)]
    pin: bool,

    /// Reinstall without prompt if a package is already installed.
    #[arg(long)]
    force: bool,

    /// Install the package as a development dependency.
    /// Also called `dev`.
    #[arg(short, long, alias = "dev", visible_short_aliases = ['d', 'b'])]
    build: Option<Vec<PackageReq>>,

    /// Install the package as a test dependency.
    #[arg(short, long)]
    test: Option<Vec<PackageReq>>,
}

pub async fn add(data: Add, config: Config) -> Result<()> {
    let mut project = Project::current()?.ok_or_eyre("No project found")?;

    let pin = PinnedState::from(data.pin);
    let lua_version = LuaVersion::from(&config)?;
    let tree = project.tree(lua_version)?;
    let db = RemotePackageDB::from_config(&config, &Progress::Progress(ProgressBar::new())).await?;

    let regular_packages = apply_build_behaviour(data.package_req, pin, data.force, &tree);
    if !regular_packages.is_empty() {
        project
            .add(
                rocks_lib::project::DependencyType::Regular(
                    regular_packages
                        .iter()
                        .map(|(_, req)| req.clone())
                        .collect(),
                ),
                &db,
            )
            .await?;
    }

    let build_packages =
        apply_build_behaviour(data.build.unwrap_or_default(), pin, data.force, &tree);
    if !build_packages.is_empty() {
        project
            .add(
                rocks_lib::project::DependencyType::Build(
                    build_packages.iter().map(|(_, req)| req.clone()).collect(),
                ),
                &db,
            )
            .await?;
    }

    let test_packages =
        apply_build_behaviour(data.test.unwrap_or_default(), pin, data.force, &tree);
    if !test_packages.is_empty() {
        project
            .add(
                rocks_lib::project::DependencyType::Test(
                    test_packages.iter().map(|(_, req)| req.clone()).collect(),
                ),
                &db,
            )
            .await?;
    }

    operations::Install::new(&config)
        .packages(regular_packages)
        .packages(build_packages)
        .packages(test_packages)
        .pin(pin)
        .progress(MultiProgress::new_arc())
        .install()
        .await?;

    Ok(())
}
