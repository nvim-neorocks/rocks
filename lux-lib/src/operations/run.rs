use std::{io, process::Command};

use crate::{
    build::BuildBehaviour,
    config::{Config, LuaVersion, LuaVersionUnset},
    lua_rockspec::LuaVersionError,
    operations::Install,
    package::{PackageReq, PackageVersionReqError},
    path::Paths,
    project::{Project, ProjectTreeError},
    remote_package_db::RemotePackageDBError,
};
use bon::Builder;
use itertools::Itertools;
use thiserror::Error;

use super::InstallError;

/// Rocks package runner, providing fine-grained control
/// over how a package should be run.
#[derive(Builder)]
#[builder(start_fn = new, finish_fn(name = _run, vis = ""))]
pub struct Run<'a> {
    #[builder(start_fn)]
    command: &'a str,
    #[builder(start_fn)]
    project: Option<&'a Project>,
    #[builder(start_fn)]
    config: &'a Config,

    #[builder(field)]
    args: Vec<String>,
}

impl<State: run_builder::State> RunBuilder<'_, State> {
    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    pub fn args(mut self, args: impl IntoIterator<Item: Into<String>>) -> Self {
        self.args.extend(args.into_iter().map_into());
        self
    }

    pub async fn run(self) -> Result<(), RunError>
    where
        State: run_builder::IsComplete,
    {
        run(self._run()).await
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
    #[error(transparent)]
    LuaVersionError(#[from] LuaVersionError),
    #[error(transparent)]
    ProjectTreeError(#[from] ProjectTreeError),
}

async fn run(run: Run<'_>) -> Result<(), RunError> {
    let lua_version = run
        .project
        .map(|project| project.lua_version(run.config))
        .transpose()?
        .unwrap_or(LuaVersion::from(run.config)?);

    let user_tree = run.config.tree(lua_version)?;
    let mut paths = Paths::new(user_tree)?;

    if let Some(project) = run.project {
        paths.prepend(&Paths::new(project.tree(run.config)?)?);
    }

    let status = match Command::new(run.command)
        .args(run.args)
        .env("PATH", paths.path_prepended().joined())
        .env("LUA_PATH", paths.package_path().joined())
        .env("LUA_CPATH", paths.package_cpath().joined())
        .status()
    {
        Ok(status) => Ok(status),
        Err(err) => Err(RunError::RunCommandFailure(run.command.to_string(), err)),
    }?;

    if status.success() {
        Ok(())
    } else {
        Err(RunError::RunFailure(run.command.to_string()))
    }
}

#[derive(Error, Debug)]
#[error(transparent)]
pub enum InstallCmdError {
    InstallError(#[from] InstallError),
    PackageVersionReqError(#[from] PackageVersionReqError),
    RemotePackageDBError(#[from] RemotePackageDBError),
    Io(#[from] io::Error),
    LuaVersionUnset(#[from] LuaVersionUnset),
}

/// Ensure that a command is installed.
/// This defaults to the local project tree if cwd is a project root.
pub async fn install_command(command: &str, config: &Config) -> Result<(), InstallCmdError> {
    let package_req = PackageReq::new(command.into(), None)?;
    Install::new(&config.tree(LuaVersion::from(config)?)?, config)
        .package(BuildBehaviour::NoForce, package_req)
        .install()
        .await?;
    Ok(())
}
