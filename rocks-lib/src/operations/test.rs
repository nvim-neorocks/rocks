use std::{io, process::Command};

use crate::{
    build::BuildBehaviour,
    config::Config,
    lockfile::PinnedState,
    manifest::ManifestMetadata,
    package::{PackageName, PackageReq, PackageVersionReqError},
    path::Paths,
    progress::MultiProgress,
    project::Project,
    rockspec::Rockspec,
    tree::Tree,
};
use itertools::Itertools;
use thiserror::Error;

use super::{install, InstallError};

pub enum TestEnv {
    Pure,
    Impure,
}

#[derive(Error, Debug)]
pub enum RunTestsError {
    #[error("tests failed!")]
    TestFailure,
    #[error("failed to execute `{0}`: {1}")]
    RunCommandFailure(String, io::Error),
    #[error("lua version not set! Please provide a version through `--lua-version <ver>` or add it to your rockspec's dependencies.")]
    LuaVersionUnset,
    #[error(transparent)]
    Io(#[from] io::Error),
}

pub async fn run_tests<I>(
    project: Project,
    test_args: I,
    env: TestEnv,
    config: Config,
) -> Result<(), RunTestsError>
where
    I: IntoIterator<Item = String> + Send,
{
    let rockspec = project.rockspec();
    let lua_version = match rockspec.lua_version_from_config(&config) {
        Ok(lua_version) => Ok(lua_version),
        Err(_) => rockspec
            .test_lua_version()
            .ok_or(RunTestsError::LuaVersionUnset),
    }?;
    let tree = Tree::new(config.tree().clone(), lua_version)?;
    let tree_root = &tree.root().clone();
    let paths = Paths::from_tree(tree)?;
    let mut command = Command::new("busted");
    let mut command = command
        .current_dir(project.root())
        .args(test_args)
        .env("PATH", paths.path_appended().joined())
        .env("LUA_PATH", paths.package_path().joined())
        .env("LUA_CPATH", paths.package_cpath().joined());
    if let TestEnv::Pure = env {
        // isolate the test runner from the user's own config/data files
        // by initialising empty HOME and XDG base directory paths
        let home = tree_root.join("home");
        let xdg = home.join("xdg");
        let _ = std::fs::remove_dir_all(&home);
        let xdg_config_home = xdg.join("config");
        std::fs::create_dir_all(&xdg_config_home)?;
        let xdg_state_home = xdg.join("local").join("state");
        std::fs::create_dir_all(&xdg_state_home)?;
        let xdg_data_home = xdg.join("local").join("share");
        std::fs::create_dir_all(&xdg_data_home)?;
        command = command
            .env("HOME", home)
            .env("XDG_CONFIG_HOME", xdg_config_home)
            .env("XDG_STATE_HOME", xdg_state_home)
            .env("XDG_DATA_HOME", xdg_data_home);
    }
    let status = match command.status() {
        Ok(status) => Ok(status),
        Err(err) => Err(RunTestsError::RunCommandFailure("busted".into(), err)),
    }?;
    if status.success() {
        Ok(())
    } else {
        Err(RunTestsError::TestFailure)
    }
}

#[derive(Error, Debug)]
pub enum InstallTestDependenciesError {
    #[error(transparent)]
    InstallError(#[from] InstallError),
    #[error(transparent)]
    PackageVersionReqError(#[from] PackageVersionReqError),
}

/// Ensure that busted is installed.
/// This defaults to the local project tree if cwd is a project root.
pub async fn ensure_busted(
    progress: &MultiProgress,
    tree: &Tree,
    manifest: &ManifestMetadata,
    config: &Config,
) -> Result<(), InstallTestDependenciesError> {
    let busted_req = PackageReq::new("busted".into(), None)?;

    if tree.has_rock(&busted_req).is_none() {
        install(
            progress,
            vec![(BuildBehaviour::NoForce, busted_req)],
            PinnedState::Unpinned,
            manifest,
            config,
        )
        .await?;
    }

    Ok(())
}

/// Ensure dependencies and test dependencies are installed
/// This defaults to the local project tree if cwd is a project root.
pub async fn ensure_dependencies(
    progress: &MultiProgress,
    rockspec: &Rockspec,
    tree: &Tree,
    manifest: &ManifestMetadata,
    config: &Config,
) -> Result<(), InstallTestDependenciesError> {
    let dependencies = rockspec
        .test_dependencies
        .current_platform()
        .iter()
        .chain(rockspec.dependencies.current_platform())
        .filter(|req| !req.name().eq(&PackageName::new("lua".into())))
        .filter_map(|req| {
            let build_behaviour = if tree.has_rock(req).is_some() {
                Some(BuildBehaviour::Force)
            } else {
                None
            };
            build_behaviour.map(|it| (it, req.to_owned()))
        })
        .collect_vec();

    install(
        progress,
        dependencies,
        PinnedState::Unpinned,
        manifest,
        config,
    )
    .await?;

    Ok(())
}
