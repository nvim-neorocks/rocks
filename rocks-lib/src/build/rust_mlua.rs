use super::utils::lua_lib_extension;
use crate::config::LuaVersionUnset;
use crate::lua_rockspec::BuildInfo;
use crate::progress::{Progress, ProgressBar};
use crate::{
    config::{Config, LuaVersion},
    lua_installation::LuaInstallation,
    lua_rockspec::{Build, RustMluaBuildSpec},
    tree::RockLayout,
};
use itertools::Itertools;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::{fs, io};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum RustError {
    #[error("`cargo build` failed.\nstatus: {status}\nstdout: {stdout}\nstderr: {stderr}")]
    CargoBuild {
        status: ExitStatus,
        stdout: String,
        stderr: String,
    },
    #[error("failed to run `cargo build`: {0}")]
    RustBuild(#[from] io::Error),
    #[error(transparent)]
    LuaVersionUnset(#[from] LuaVersionUnset),
}

impl Build for RustMluaBuildSpec {
    type Err = RustError;

    async fn run(
        self,
        output_paths: &RockLayout,
        _no_install: bool,
        _lua: &LuaInstallation,
        config: &Config,
        build_dir: &Path,
        progress: &Progress<ProgressBar>,
    ) -> Result<BuildInfo, Self::Err> {
        let lua_version = LuaVersion::from(config)?;
        let lua_feature = match lua_version {
            LuaVersion::Lua51 => "lua51",
            LuaVersion::Lua52 => "lua52",
            LuaVersion::Lua53 => "lua53",
            LuaVersion::Lua54 => "lua54",
            LuaVersion::LuaJIT => "luajit",
            LuaVersion::LuaJIT52 => "luajit",
        };
        let features = self
            .features
            .into_iter()
            .chain(std::iter::once(lua_feature.into()))
            .join(",");
        let target_dir_arg = format!("--target-dir={}", self.target_path.display());
        let mut build_args = vec!["build", "--release", &target_dir_arg];
        if !self.default_features {
            build_args.push("--no-default-features");
        }
        build_args.push("--features");
        build_args.push(&features);
        match Command::new("cargo")
            .current_dir(build_dir)
            .args(build_args)
            .output()
        {
            Ok(output) if output.status.success() => {}
            Ok(output) => {
                return Err(RustError::CargoBuild {
                    status: output.status,
                    stdout: String::from_utf8_lossy(&output.stdout).into(),
                    stderr: String::from_utf8_lossy(&output.stderr).into(),
                });
            }
            Err(err) => return Err(RustError::RustBuild(err)),
        }
        fs::create_dir_all(&output_paths.lib)?;
        if let Err(err) =
            install_rust_libs(self.modules, &self.target_path, build_dir, output_paths)
        {
            cleanup(output_paths, progress).await?;
            return Err(err.into());
        }
        fs::create_dir_all(&output_paths.src)?;
        if let Err(err) = install_lua_libs(self.include, build_dir, output_paths) {
            cleanup(output_paths, progress).await?;
            return Err(err.into());
        }
        Ok(BuildInfo::default())
    }
}

fn install_rust_libs(
    modules: HashMap<String, PathBuf>,
    target_path: &Path,
    build_dir: &Path,
    output_paths: &RockLayout,
) -> io::Result<()> {
    for (module, rust_lib) in modules {
        let src = build_dir.join(target_path).join("release").join(rust_lib);
        let mut dst: PathBuf = output_paths.lib.join(module);
        dst.set_extension(lua_lib_extension());
        fs::copy(src, dst)?;
    }
    Ok(())
}

fn install_lua_libs(
    include: HashMap<PathBuf, PathBuf>,
    build_dir: &Path,
    output_paths: &RockLayout,
) -> io::Result<()> {
    for (from, to) in include {
        let src = build_dir.join(from);
        let dst = output_paths.src.join(to);
        fs::copy(src, dst)?;
    }
    Ok(())
}

async fn cleanup(output_paths: &RockLayout, progress: &Progress<ProgressBar>) -> io::Result<()> {
    let root_dir = &output_paths.rock_path;

    progress.map(|p| p.set_message(format!("🗑️ Cleaning up {}", root_dir.display())));

    match std::fs::remove_dir_all(root_dir) {
        Ok(_) => (),
        Err(err) => {
            progress
                .map(|p| p.println(format!("Error cleaning up {}: {}", root_dir.display(), err)));
        }
    };

    Ok(())
}
