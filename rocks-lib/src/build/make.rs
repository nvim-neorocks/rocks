use eyre::eyre;
use indicatif::MultiProgress;
use itertools::Itertools;
use std::{path::Path, process::Command};

use crate::{
    build::variables::HasVariables,
    config::Config,
    lua_installation::LuaInstallation,
    rockspec::{Build, MakeBuildSpec},
    tree::RockLayout,
};

impl Build for MakeBuildSpec {
    fn run(
        self,
        _progress: &MultiProgress,
        output_paths: &RockLayout,
        no_install: bool,
        lua: &LuaInstallation,
        config: &Config,
        build_dir: &Path,
    ) -> eyre::Result<()> {
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
                        return Err(eyre!(
                            "`make` build step failed.
status: {}
stdout: {}
stderr: {}",
                            output.status,
                            String::from_utf8_lossy(&output.stdout),
                            String::from_utf8_lossy(&output.stderr),
                        ))
                    }
                    Err(err) => return Err(eyre!("Failed to run `make` build step: {err}")),
                },
                Err(_) => {
                    return Err(eyre!(
                        "Failed to run build step: `{}` command not found!",
                        config.make_cmd()
                    ))
                }
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
                .spawn()
            {
                Ok(child) => match child.wait_with_output() {
                    Ok(output) if output.status.success() => {}
                    Ok(output) => {
                        return Err(eyre!(
                            "`make` install step failed.
status: {}
stdout: {}
stderr: {}",
                            output.status,
                            String::from_utf8_lossy(&output.stdout),
                            String::from_utf8_lossy(&output.stderr),
                        ));
                    }
                    Err(err) => return Err(eyre!("Failed to run `make` install step: {err}")),
                },
                Err(_) => {
                    return Err(eyre!(
                        "Failed to run install step: `{}` command not found!",
                        config.make_cmd()
                    ))
                }
            }
        };

        Ok(())
    }
}
