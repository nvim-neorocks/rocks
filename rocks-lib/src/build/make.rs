use itertools::Itertools;
use std::{
    io,
    path::Path,
    process::{Command, ExitStatus},
};
use thiserror::Error;

use crate::{
    build::variables::HasVariables,
    config::Config,
    lua_installation::LuaInstallation,
    progress::ProgressBar,
    rockspec::{Build, MakeBuildSpec},
    tree::RockLayout,
};

use super::BuildError;

#[derive(Error, Debug)]
pub enum MakeError {
    #[error("make step failed.\nstatus: {status}\nstdout: {stdout}\nstderr: {stderr}")]
    CommandFailure {
        status: ExitStatus,
        stdout: String,
        stderr: String,
    },
    #[error("failed to run `make` step: {0}")]
    Io(io::Error),
    #[error("failed to run `make` step: `{0}` command not found!")]
    CommandNotFound(String),
}

impl Build for MakeBuildSpec {
    type Err = BuildError;

    async fn run(
        self,
        _progress: &ProgressBar,
        output_paths: &RockLayout,
        no_install: bool,
        lua: &LuaInstallation,
        config: &Config,
        build_dir: &Path,
    ) -> Result<(), Self::Err> {
        // Build step
        if self.build_pass {
            let build_args = self
                .build_variables
                .into_iter()
                .map(|(key, value)| {
                    let mut substituted_value = output_paths.substitute_variables(value);
                    substituted_value = lua.substitute_variables(substituted_value);
                    substituted_value = config.substitute_variables(substituted_value);
                    format!("{key}={substituted_value}")
                })
                .collect_vec();
            match Command::new(config.make_cmd())
                .current_dir(build_dir)
                .arg(self.build_target)
                .args(["-f", self.makefile.to_str().unwrap()])
                .args(build_args)
                .spawn()
            {
                Ok(child) => match child.wait_with_output() {
                    Ok(output) if output.status.success() => {}
                    Ok(output) => {
                        return Err(MakeError::CommandFailure {
                            status: output.status,
                            stdout: String::from_utf8_lossy(&output.stdout).into(),
                            stderr: String::from_utf8_lossy(&output.stderr).into(),
                        }
                        .into());
                    }
                    Err(err) => return Err(MakeError::Io(err).into()),
                },
                Err(_) => return Err(MakeError::CommandNotFound(config.make_cmd().clone()).into()),
            }
        };

        // Install step
        if self.install_pass && !no_install {
            let install_args = self
                .install_variables
                .into_iter()
                .map(|(key, value)| {
                    let mut substituted_value = output_paths.substitute_variables(value);
                    substituted_value = lua.substitute_variables(substituted_value);
                    substituted_value = config.substitute_variables(substituted_value);
                    format!("{key}={substituted_value}")
                })
                .collect_vec();
            match Command::new(config.make_cmd())
                .current_dir(build_dir)
                .arg(self.install_target)
                .args(["-f", self.makefile.to_str().unwrap()])
                .args(install_args)
                .output()
            {
                Ok(output) if output.status.success() => {}
                Ok(output) => {
                    return Err(MakeError::CommandFailure {
                        status: output.status,
                        stdout: String::from_utf8_lossy(&output.stdout).into(),
                        stderr: String::from_utf8_lossy(&output.stderr).into(),
                    }
                    .into())
                }
                Err(err) => return Err(MakeError::Io(err).into()),
            }
        };

        Ok(())
    }
}
