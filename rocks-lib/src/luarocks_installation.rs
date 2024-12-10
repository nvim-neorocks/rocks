use itertools::Itertools;
use std::{
    io,
    path::Path,
    process::{Command, ExitStatus},
};
use tempdir::TempDir;
use thiserror::Error;

use crate::{
    build::{build, BuildBehaviour, BuildError},
    config::{Config, LuaVersion, LuaVersionUnset},
    lockfile::{LockConstraint, PinnedState},
    lua_installation::LuaInstallation,
    manifest::{ManifestError, ManifestMetadata},
    operations::download_rockspec,
    package::PackageReq,
    path::Paths,
    progress::{Progress, ProgressBar},
    rockspec::{Rockspec, RockspecFormat},
    tree::Tree,
};

#[derive(Error, Debug)]
pub enum LuaRocksError {
    #[error(transparent)]
    LuaVersionUnset(#[from] LuaVersionUnset),
    #[error(transparent)]
    Io(#[from] io::Error),
}

#[derive(Error, Debug)]
pub enum LuaRocksInstallError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    ManifestError(#[from] ManifestError),
    #[error(transparent)]
    BuildError(#[from] BuildError),
}

#[derive(Error, Debug)]
pub enum InstallBuildDependenciesError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    ManifestError(#[from] ManifestError),
    #[error(transparent)]
    BuildError(#[from] BuildError),
}

#[derive(Error, Debug)]
pub enum ExecLuaRocksError {
    #[error(transparent)]
    LuaVersionUnset(#[from] LuaVersionUnset),
    #[error("could not write luarocks config: {0}")]
    WriteLuarocksConfigError(io::Error),
    #[error("failed to run luarocks: {0}")]
    Io(#[from] io::Error),
    #[error("executing luarocks compatibility layer failed.\nstatus: {status}\nstdout: {stdout}\nstderr: {stderr}")]
    CommandFailure {
        status: ExitStatus,
        stdout: String,
        stderr: String,
    },
}

pub struct LuaRocksInstallation {
    pub tree: Tree,
    pub config: Config,
}

const LUAROCKS_VERSION: &str = "3.11.1-1";

const LUAROCKS_ROCKSPEC: &str = "
rockspec_format = '3.0'
package = 'luarocks'
version = '3.11.1-1'
source = {
   url = 'git+https://github.com/luarocks/luarocks',
   tag = 'v3.11.1'
}
";

impl LuaRocksInstallation {
    pub fn new(config: &Config) -> Result<Self, LuaRocksError> {
        let config = config.clone().with_tree(config.luarocks_tree().clone());
        let luarocks_installation = Self {
            tree: Tree::new(config.luarocks_tree().clone(), LuaVersion::from(&config)?)?,
            config,
        };
        Ok(luarocks_installation)
    }

    pub async fn ensure_installed(
        self: &LuaRocksInstallation,
        progress: &Progress<ProgressBar>,
    ) -> Result<(), LuaRocksInstallError> {
        let mut lockfile = self.tree.lockfile()?;
        let luarocks_req =
            PackageReq::new("luarocks".into(), Some(LUAROCKS_VERSION.into())).unwrap();
        if self.tree.has_rock(&luarocks_req).is_none() {
            let rockspec = Rockspec::new(LUAROCKS_ROCKSPEC).unwrap();
            let pkg = build(
                rockspec,
                PinnedState::Unpinned,
                LockConstraint::Constrained(luarocks_req.version_req().clone()),
                BuildBehaviour::NoForce,
                &self.config,
                progress,
            )
            .await?;
            lockfile.add(&pkg);
        }
        lockfile.flush()?;
        Ok(())
    }

    pub async fn install_build_dependencies(
        self,
        build_backend: &str,
        rockspec: &Rockspec,
        progress: &Progress<ProgressBar>,
    ) -> Result<(), InstallBuildDependenciesError> {
        let mut lockfile = self.tree.lockfile()?;
        let manifest = ManifestMetadata::from_config(&self.config).await?;
        let build_dependencies = match rockspec.rockspec_format {
            Some(RockspecFormat::_1_0 | RockspecFormat::_2_0) => {
                // XXX: rockspec formats < 3.0 don't support `build_dependencies`,
                // so we have to fetch the build backend from the dependencies.
                rockspec
                    .dependencies
                    .current_platform()
                    .iter()
                    .filter(|dep| dep.name().to_string().contains(build_backend))
                    .cloned()
                    .collect_vec()
            }
            _ => rockspec.build_dependencies.current_platform().to_vec(),
        };
        for package in build_dependencies {
            if self.tree.has_rock(&package).is_none() {
                let rockspec = download_rockspec(&package, &manifest, &self.config, progress)
                    .await
                    .unwrap();
                let pkg = build(
                    rockspec,
                    PinnedState::Unpinned,
                    LockConstraint::Constrained(package.version_req().clone()),
                    BuildBehaviour::NoForce,
                    &self.config,
                    progress,
                )
                .await?;
                lockfile.add(&pkg);
            }
        }
        lockfile.flush()?;
        Ok(())
    }

    pub fn make(
        self,
        rockspec_path: &Path,
        build_dir: &Path,
        dest_dir: &Path,
        lua: &LuaInstallation,
    ) -> Result<(), ExecLuaRocksError> {
        let dest_dir_str = dest_dir.to_string_lossy().to_string();
        let rockspec_path_str = rockspec_path.to_string_lossy().to_string();
        let args = vec![
            "make",
            "--deps-mode",
            "none",
            "--tree",
            &dest_dir_str,
            &rockspec_path_str,
        ];
        self.exec(args, build_dir, lua)
    }

    fn exec(
        self,
        args: Vec<&str>,
        cwd: &Path,
        lua: &LuaInstallation,
    ) -> Result<(), ExecLuaRocksError> {
        let luarocks_paths = Paths::from_tree(self.tree)?;
        // Ensure a pure environment so we can do parallel builds
        let temp_dir = TempDir::new("rocks-run-luarocks").unwrap();
        let luarocks_config_content = format!(
            "
variables = {{
    LUA_LIBDIR = \"{0}\",
    LUA_INCDIR = \"{1}\",
    LUA_VERSION = \"{2}\",
    MAKE = \"{3}\",
}}
",
            lua.lib_dir.display(),
            lua.include_dir.display(),
            LuaVersion::from(&self.config)?,
            self.config.make_cmd(),
        );
        let luarocks_config = temp_dir.path().join("luarocks-config.lua");
        std::fs::write(luarocks_config.clone(), luarocks_config_content)
            .map_err(ExecLuaRocksError::WriteLuarocksConfigError)?;
        let output = Command::new("luarocks")
            .current_dir(cwd)
            .args(args)
            .env("PATH", luarocks_paths.path_appended().joined())
            .env("LUA_PATH", luarocks_paths.package_path().joined())
            .env("LUA_CPATH", luarocks_paths.package_cpath().joined())
            .env("HOME", temp_dir.into_path())
            .env("LUAROCKS_CONFIG", luarocks_config)
            .output()?;
        if output.status.success() {
            Ok(())
        } else {
            Err(ExecLuaRocksError::CommandFailure {
                status: output.status,
                stdout: String::from_utf8_lossy(&output.stdout).into(),
                stderr: String::from_utf8_lossy(&output.stderr).into(),
            })
        }
    }
}
