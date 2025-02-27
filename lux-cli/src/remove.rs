use clap::Args;
use eyre::{Context, OptionExt, Result};
use lux_lib::{
    config::Config, luarocks::luarocks_installation::LuaRocksInstallation, operations::Sync,
    package::PackageName, progress::MultiProgress, project::Project, rockspec::Rockspec,
};

#[derive(Args)]
pub struct Remove {
    /// Package or list of packages to remove from the dependencies.
    package: Vec<PackageName>,

    /// Remove a development dependency.
    /// Also called `dev`.
    #[arg(short, long, alias = "dev", visible_short_aliases = ['d', 'b'])]
    build: Option<Vec<PackageName>>,

    /// Remove a test dependency.
    #[arg(short, long)]
    test: Option<Vec<PackageName>>,
}

pub async fn remove(data: Remove, config: Config) -> Result<()> {
    let mut project = Project::current()?.ok_or_eyre("No project found")?;
    let tree = project.tree(&config)?;
    let progress = MultiProgress::new_arc();

    if !data.package.is_empty() {
        project
            .remove(lux_lib::project::DependencyType::Regular(data.package))
            .await?;
        // NOTE: We only update the lockfile if one exists.
        // Otherwise, the next `lx build` will remove the packages.
        if let Some(lockfile) = project.try_lockfile()? {
            let mut lockfile = lockfile.write_guard();
            let packages = project
                .toml()
                .into_validated()?
                .dependencies()
                .current_platform()
                .clone();
            Sync::new(&tree, &mut lockfile, &config)
                .packages(packages)
                .progress(progress.clone())
                .sync_dependencies()
                .await
                .wrap_err("syncing dependencies with the project lockfile failed.")?;
        }
    }

    let build_packages = data.build.unwrap_or_default();
    if !build_packages.is_empty() {
        project
            .remove(lux_lib::project::DependencyType::Build(build_packages))
            .await?;
        if let Some(lockfile) = project.try_lockfile()? {
            let luarocks = LuaRocksInstallation::new(&config)?;
            let mut lockfile = lockfile.write_guard();
            let packages = project
                .toml()
                .into_validated()?
                .build_dependencies()
                .current_platform()
                .clone();
            Sync::new(luarocks.tree(), &mut lockfile, luarocks.config())
                .packages(packages)
                .progress(progress.clone())
                .sync_build_dependencies()
                .await
                .wrap_err("syncing build dependencies with the project lockfile failed.")?;
        }
    }

    let test_packages = data.test.unwrap_or_default();
    if !test_packages.is_empty() {
        project
            .remove(lux_lib::project::DependencyType::Test(test_packages))
            .await?;
        if let Some(lockfile) = project.try_lockfile()? {
            let mut lockfile = lockfile.write_guard();
            let packages = project
                .toml()
                .into_validated()?
                .test_dependencies()
                .current_platform()
                .clone();
            Sync::new(&tree, &mut lockfile, &config)
                .packages(packages)
                .progress(progress.clone())
                .sync_test_dependencies()
                .await
                .wrap_err("syncing test dependencies with the project lockfile failed.")?;
        }
    }

    Ok(())
}
