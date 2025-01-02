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
use thiserror::Error;

use super::InstallError;

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

pub async fn run(command: &str, args: Vec<String>, config: Config) -> Result<(), RunError> {
    let lua_version = LuaVersion::from(&config)?;
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
