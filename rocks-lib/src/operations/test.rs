use std::{io, process::Command, sync::Arc};

use crate::{
    build::BuildBehaviour,
    config::Config,
    package::{PackageName, PackageReq, PackageVersionReqError},
    path::Paths,
    progress::{MultiProgress, Progress},
    project::Project,
    rockspec::Rockspec,
    tree::Tree,
};
use itertools::Itertools;
use thiserror::Error;

use super::{Install, InstallError};

pub struct Test<'a> {
    project: Project,
    config: &'a Config,
    args: Vec<String>,
    env: TestEnv,
    progress: Option<Arc<Progress<MultiProgress>>>,
}

/// A rocks project test runner, providing fine-grained control
/// over how tests should be run.
impl<'a> Test<'a> {
    /// Construct a new test runner.
    pub fn new(project: Project, config: &'a Config) -> Self {
        Self {
            project,
            config,
            args: Vec::new(),
            env: TestEnv::default(),
            progress: None,
        }
    }

    /// Pass arguments to the test executable.
    pub fn args<I>(self, args: I) -> Self
    where
        I: IntoIterator<Item = String> + Send,
    {
        Self {
            args: self.args.into_iter().chain(args).collect_vec(),
            ..self
        }
    }

    /// Pass an argument to the test executable.
    pub fn arg(self, arg: String) -> Self {
        self.args(std::iter::once(arg))
    }

    /// Define the environment in which to run the tests.
    pub fn env(self, env: TestEnv) -> Self {
        Self { env, ..self }
    }

    /// Pass a `MultiProgress` to this runner.
    /// By default, a new one will be created.
    pub fn progress(self, progress: Arc<Progress<MultiProgress>>) -> Self {
        Self {
            progress: Some(progress),
            ..self
        }
    }

    /// Run the test suite
    pub async fn run(self) -> Result<(), RunTestsError> {
        let progress = match self.progress {
            Some(p) => p,
            None => MultiProgress::new_arc(),
        };
        run_tests(self.project, self.args, self.env, self.config, progress).await
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
}

async fn run_tests<I>(
    project: Project,
    test_args: I,
    env: TestEnv,
    config: &Config,
    progress: Arc<Progress<MultiProgress>>,
) -> Result<(), RunTestsError>
where
    I: IntoIterator<Item = String> + Send,
{
    let rockspec = project.rockspec();
    let lua_version = match rockspec.lua_version_from_config(config) {
        Ok(lua_version) => Ok(lua_version),
        Err(_) => rockspec
            .test_lua_version()
            .ok_or(RunTestsError::LuaVersionUnset),
    }?;
    let tree = Tree::new(config.tree().clone(), lua_version)?;
    // TODO(#204): Only ensure busted if running with busted (e.g. a .busted directory exists)
    ensure_busted(&tree, config, progress.clone()).await?;
    ensure_dependencies(rockspec, &tree, config, progress).await?;
    let tree_root = &tree.root().clone();
    let paths = Paths::from_tree(tree)?;
    let mut command = Command::new("busted");
    let mut command = command
        .current_dir(project.root())
        .args(test_args)
        .env("PATH", paths.path_prepended().joined())
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
        Install::new(config)
            .package(BuildBehaviour::NoForce, busted_req)
            .progress(progress)
            .install()
            .await?;
    }

    Ok(())
}

/// Ensure dependencies and test dependencies are installed
/// This defaults to the local project tree if cwd is a project root.
async fn ensure_dependencies(
    rockspec: &Rockspec,
    tree: &Tree,
    config: &Config,
    progress: Arc<Progress<MultiProgress>>,
) -> Result<(), InstallTestDependenciesError> {
    let dependencies = rockspec
        .test_dependencies
        .current_platform()
        .iter()
        .chain(rockspec.dependencies.current_platform())
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

    Install::new(config)
        .packages(dependencies)
        .progress(progress)
        .install()
        .await?;

    Ok(())
}
