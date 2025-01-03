use std::{io, process::Command};

use crate::{
    build::BuildBehaviour,
    config::{Config, LuaVersion, LuaVersionUnset},
    operations::Install,
    package::{PackageReq, PackageVersionReqError},
    path::Paths,
    remote_package_db::RemotePackageDBError,
    tree::Tree,
};
use itertools::Itertools;
use thiserror::Error;

use super::InstallError;

/// Rocks package runner, providing fine-grained control
/// over how a package should be run.
pub struct Run<'a> {
    command: String,
    args: Vec<String>,
    config: &'a Config,
}

impl<'a> Run<'a> {
    /// Construct a new runner.
    pub fn new(command: &str, config: &'a Config) -> Self {
        Self {
            command: command.into(),
            args: Vec::new(),
            config,
        }
    }

    /// Specify packages to install, along with each package's build behaviour.
    pub fn args<I>(self, args: I) -> Self
    where
        I: IntoIterator<Item = String>,
    {
        Self {
            args: self.args.into_iter().chain(args).collect_vec(),
            ..self
        }
    }

    /// Add a package to the set of packages to install.
    pub fn arg(self, arg: &str) -> Self {
        self.args(std::iter::once(arg.into()))
    }

    /// Run the package.
    pub async fn run(self) -> Result<(), RunError> {
        run(&self.command, self.args, self.config).await
    }
}

#[derive(Error, Debug)]
pub enum RunError {
    #[error("Running {0} failed!")]
    RunFailure(String),
    #[error("failed to execute `{0}`: {1}")]
    RunCommandFailure(String, io::Error),
    #[error(transparent)]
    LuaVersionUnset(#[from] LuaVersionUnset),
    #[error(transparent)]
    Io(#[from] io::Error),
}

async fn run(command: &str, args: Vec<String>, config: &Config) -> Result<(), RunError> {
    let lua_version = LuaVersion::from(config)?;
    let tree = Tree::new(config.tree().clone(), lua_version.clone())?;
    let paths = Paths::from_tree(tree)?;
    let status = match Command::new(command)
        .args(args)
        .env("PATH", paths.path_prepended().joined())
        .env("LUA_PATH", paths.package_path().joined())
        .env("LUA_CPATH", paths.package_cpath().joined())
        .status()
    {
        Ok(status) => Ok(status),
        Err(err) => Err(RunError::RunCommandFailure(command.into(), err)),
    }?;
    if status.success() {
        Ok(())
    } else {
        Err(RunError::RunFailure(command.into()))
    }
}

#[derive(Error, Debug)]
#[error(transparent)]
pub enum InstallCmdError {
    InstallError(#[from] InstallError),
    PackageVersionReqError(#[from] PackageVersionReqError),
    RemotePackageDBError(#[from] RemotePackageDBError),
}

/// Ensure that a command is installed.
/// This defaults to the local project tree if cwd is a project root.
pub async fn install_command(command: &str, config: &Config) -> Result<(), InstallCmdError> {
    let package_req = PackageReq::new(command.into(), None)?;
    Install::new(config)
        .package(BuildBehaviour::NoForce, package_req)
        .install()
        .await?;
    Ok(())
}
