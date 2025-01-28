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

// TODO: Make `rocks add thing --build lots of stuff here --test more stuff here` work

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

    project
        .add(
            rocks_lib::project::DependencyType::Regular(data.package_req.clone()),
            &db,
        )
        .await?;
    if let Some(build) = &data.build {
        project
            .add(
                rocks_lib::project::DependencyType::Build(build.clone()),
                &db,
            )
            .await?;
    }
    if let Some(test) = &data.test {
        project
            .add(rocks_lib::project::DependencyType::Test(test.clone()), &db)
            .await?;
    }

    operations::Install::new(&config)
        .packages(apply_build_behaviour(
            data.package_req,
            pin,
            data.force,
            &tree,
        ))
        .packages(apply_build_behaviour(
            data.build.unwrap_or_default(),
            pin,
            data.force,
            &tree,
        ))
        .packages(apply_build_behaviour(
            data.test.unwrap_or_default(),
            pin,
            data.force,
            &tree,
        ))
        .pin(pin)
        .progress(MultiProgress::new_arc())
        .install()
        .await?;

    Ok(())
}
