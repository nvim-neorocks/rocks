use itertools::Itertools;
use std::{
    io,
    path::Path,
    process::{Command, ExitStatus},
};
use thiserror::Error;

use crate::{
    build::utils,
    config::Config,
    lua_installation::LuaInstallation,
    progress::{Progress, ProgressBar},
    rockspec::{Build, BuildInfo, MakeBuildSpec},
    tree::RockLayout,
};

#[derive(Error, Debug)]
pub enum MakeError {
    #[error("{name} step failed.\nstatus: {status}\nstdout: {stdout}\nstderr: {stderr}")]
    CommandFailure {
        name: String,
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
    type Err = MakeError;

    async fn run(
        self,
        output_paths: &RockLayout,
        no_install: bool,
        lua: &LuaInstallation,
        config: &Config,
        build_dir: &Path,
        _progress: &Progress<ProgressBar>,
    ) -> Result<BuildInfo, Self::Err> {
        // Build step
        if self.build_pass {
            let build_args = self
                .variables
                .iter()
                .chain(&self.build_variables)
                .map(|(key, value)| {
                    let substituted_value =
                        utils::substitute_variables(value, output_paths, lua, config);
                    format!("{key}={substituted_value}")
                })
                .collect_vec();
            match Command::new(config.make_cmd())
                .current_dir(build_dir)
                .arg(&self.build_target)
                .args(["-f", self.makefile.to_str().unwrap()])
                .args(build_args)
                .spawn()
            {
                Ok(child) => match child.wait_with_output() {
                    Ok(output) if output.status.success() => {}
                    Ok(output) => {
                        return Err(MakeError::CommandFailure {
                            name: format!("{} {}", config.make_cmd(), self.build_target),
                            status: output.status,
                            stdout: String::from_utf8_lossy(&output.stdout).into(),
                            stderr: String::from_utf8_lossy(&output.stderr).into(),
                        });
                    }
                    Err(err) => return Err(MakeError::Io(err)),
                },
                Err(_) => return Err(MakeError::CommandNotFound(config.make_cmd().clone())),
            }
        };

        // Install step
        if self.install_pass && !no_install {
            let install_args = self
                .variables
                .iter()
                .chain(&self.install_variables)
                .map(|(key, value)| {
                    let substituted_value =
                        utils::substitute_variables(value, output_paths, lua, config);
                    format!("{key}={substituted_value}")
                })
                .collect_vec();
            match Command::new(config.make_cmd())
                .current_dir(build_dir)
                .arg(&self.install_target)
                .args(["-f", self.makefile.to_str().unwrap()])
                .args(install_args)
                .output()
            {
                Ok(output) if output.status.success() => {}
                Ok(output) => {
                    return Err(MakeError::CommandFailure {
                        name: format!("{} {}", config.make_cmd(), self.install_target),
                        status: output.status,
                        stdout: String::from_utf8_lossy(&output.stdout).into(),
                        stderr: String::from_utf8_lossy(&output.stderr).into(),
                    })
                }
                Err(err) => return Err(MakeError::Io(err)),
            }
        };

        Ok(BuildInfo::default())
    }
}
