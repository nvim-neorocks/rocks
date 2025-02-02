use std::{io, process::Command, sync::Arc};

use crate::{
    build::BuildBehaviour,
    config::Config,
    lua_rockspec::RockspecType,
    package::{PackageName, PackageReq, PackageVersionReqError},
    path::Paths,
    progress::{MultiProgress, Progress},
    project::{rocks_toml::RocksTomlValidationError, Project},
    rockspec::{LuaVersionCompatibility, Rockspec},
    tree::Tree,
};
use bon::Builder;
use itertools::Itertools;
use thiserror::Error;

use super::{Install, InstallError};

#[derive(Builder)]
#[builder(start_fn = new, finish_fn(name = _run, vis = ""))]
pub struct Test<'a> {
    #[builder(start_fn)]
    project: Project,
    #[builder(start_fn)]
    config: &'a Config,

    #[builder(field)]
    args: Vec<String>,

    #[builder(default)]
    env: TestEnv,
    #[builder(default = MultiProgress::new_arc())]
    progress: Arc<Progress<MultiProgress>>,
}

impl<State: test_builder::State> TestBuilder<'_, State> {
    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    pub fn args(mut self, args: impl IntoIterator<Item: Into<String>>) -> Self {
        self.args.extend(args.into_iter().map_into());
        self
    }

    pub async fn run(self) -> Result<(), RunTestsError>
    where
        State: test_builder::IsComplete,
    {
        run_tests(self._run()).await
    }
}

pub enum TestEnv {
    /// An environment that is isolated from `HOME` and `XDG` base directories (default).
    Pure,
    /// An impure environment in which `HOME` and `XDG` base directories can influence
    /// the test results.
    Impure,
}

impl Default for TestEnv {
    fn default() -> Self {
        Self::Pure
    }
}

#[derive(Error, Debug)]
pub enum RunTestsError {
    #[error(transparent)]
    InstallTestDependencies(#[from] InstallTestDependenciesError),
    #[error("tests failed!")]
    TestFailure,
    #[error("failed to execute `{0}`: {1}")]
    RunCommandFailure(String, io::Error),
    #[error("lua version not set! Please provide a version through `--lua-version <ver>` or add it to your rockspec's dependencies.")]
    LuaVersionUnset,
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    RocksTomlValidation(#[from] RocksTomlValidationError),
}

async fn run_tests(test: Test<'_>) -> Result<(), RunTestsError> {
    let rocks = test.project.rocks().into_validated_rocks_toml()?;
    let lua_version = match rocks.lua_version_matches(test.config) {
        Ok(lua_version) => Ok(lua_version),
        Err(_) => rocks
            .test_lua_version()
            .ok_or(RunTestsError::LuaVersionUnset),
    }?;
    let tree = test.config.tree(lua_version)?;
    // TODO(#204): Only ensure busted if running with busted (e.g. a .busted directory exists)
    ensure_busted(&tree, test.config, test.progress.clone()).await?;
    ensure_dependencies(&rocks, &tree, test.config, test.progress).await?;
    let tree_root = &tree.root().clone();
    let paths = Paths::from_tree(tree)?;
    let mut command = Command::new("busted");
    let mut command = command
        .current_dir(test.project.root())
        .args(test.args)
        .env("PATH", paths.path_prepended().joined())
        .env("LUA_PATH", paths.package_path().joined())
        .env("LUA_CPATH", paths.package_cpath().joined());
    if let TestEnv::Pure = test.env {
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
#[error("error installing test dependencies: {0}")]
pub enum InstallTestDependenciesError {
    IoError(#[from] io::Error),
    InstallError(#[from] InstallError),
    PackageVersionReqError(#[from] PackageVersionReqError),
}

/// Ensure that busted is installed.
/// This defaults to the local project tree if cwd is a project root.
pub async fn ensure_busted(
    tree: &Tree,
    config: &Config,
    progress: Arc<Progress<MultiProgress>>,
) -> Result<(), InstallTestDependenciesError> {
    let busted_req = PackageReq::new("busted".into(), None)?;

    if !tree.match_rocks(&busted_req)?.is_found() {
        Install::new(tree, config)
            .package(BuildBehaviour::NoForce, busted_req)
            .progress(progress)
            .install()
            .await?;
    }

    Ok(())
}

/// Ensure dependencies and test dependencies are installed
/// This defaults to the local project tree if cwd is a project root.
async fn ensure_dependencies<T: RockspecType>(
    rockspec: &impl Rockspec<RType = T>,
    tree: &Tree,
    config: &Config,
    progress: Arc<Progress<MultiProgress>>,
) -> Result<(), InstallTestDependenciesError> {
    let dependencies = rockspec
        .test_dependencies()
        .current_platform()
        .iter()
        .chain(rockspec.dependencies().current_platform())
        .filter(|req| !req.name().eq(&PackageName::new("lua".into())))
        .filter_map(|req| {
            let build_behaviour = if tree
                .match_rocks(req)
                .is_ok_and(|matches| matches.is_found())
            {
                Some(BuildBehaviour::Force)
            } else {
                None
            };
            build_behaviour.map(|it| (it, req.to_owned()))
        });

    Install::new(tree, config)
        .packages(dependencies)
        .progress(progress)
        .install()
        .await?;

    Ok(())
}
