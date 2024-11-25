use std::{io, path::Path, process::ExitStatus};

use crate::{
    config::Config,
    hash::HasIntegrity,
    lockfile::{LocalPackage, LocalPackageHashes, LockConstraint, PinnedState},
    lua_installation::LuaInstallation,
    operations::{self, FetchSrcRockError},
    package::RemotePackage,
    progress::ProgressBar,
    rockspec::{utils, Build as _, BuildBackendSpec, LuaVersionError, Rockspec},
    tree::{RockLayout, Tree},
};
use indicatif::style::TemplateError;
use make::MakeError;
use rust_mlua::RustError;
use thiserror::Error;
mod builtin;
mod make;
mod rust_mlua;
pub mod variables;

#[derive(Error, Debug)]
pub enum BuildError {
    #[error("IO operation failed: {0}")]
    Io(#[from] io::Error),
    #[error("failed to create spinner: {0}")]
    SpinnerFailure(#[from] TemplateError),
    #[error("failed to compile build modules: {0}")]
    CompilationError(#[from] cc::Error),
    #[error(transparent)]
    MakeError(#[from] MakeError),
    #[error(transparent)]
    RustError(#[from] RustError),
    #[error(transparent)]
    LuaVersionError(#[from] LuaVersionError),
    #[error("failed to fetch rock source: {0}")]
    FetchSrcRockError(#[from] FetchSrcRockError),
    #[error("compilation failed.\nstatus: {status}\nstdout: {stdout}\nstderr: {stderr}")]
    CommandFailure {
        status: ExitStatus,
        stdout: String,
        stderr: String,
    },
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum BuildBehaviour {
    NoForce,
    Force,
}

impl From<bool> for BuildBehaviour {
    fn from(value: bool) -> Self {
        if value {
            Self::Force
        } else {
            Self::NoForce
        }
    }
}

async fn run_build(
    progress: &ProgressBar,
    rockspec: &Rockspec,
    output_paths: &RockLayout,
    lua: &LuaInstallation,
    config: &Config,
    build_dir: &Path,
) -> Result<(), BuildError> {
    progress.set_message("🛠️ Building...");

    match rockspec.build.current_platform().build_backend.to_owned() {
        Some(BuildBackendSpec::Builtin(build_spec)) => {
            build_spec
                .run(progress, output_paths, false, lua, config, build_dir)
                .await?
        }
        Some(BuildBackendSpec::Make(make_spec)) => {
            make_spec
                .run(progress, output_paths, false, lua, config, build_dir)
                .await?
        }
        Some(BuildBackendSpec::RustMlua(rust_mlua_spec)) => {
            rust_mlua_spec
                .run(progress, output_paths, false, lua, config, build_dir)
                .await?
        }
        _ => unimplemented!(),
    }

    Ok(())
}

async fn install(
    progress: &ProgressBar,
    rockspec: &Rockspec,
    tree: &Tree,
    output_paths: &RockLayout,
    lua: &LuaInstallation,
    build_dir: &Path,
) -> Result<(), BuildError> {
    progress.set_message(format!(
        "💻 Installing {} {}",
        rockspec.package, rockspec.version
    ));

    let install_spec = &rockspec.build.current_platform().install;
    let lua_len = install_spec.lua.len();
    let lib_len = install_spec.lib.len();
    let bin_len = install_spec.bin.len();
    let total_len = lua_len + lib_len + bin_len;
    progress.set_position(total_len as u64);

    if lua_len > 0 {
        progress.set_message("Copying Lua modules...");
    }
    for (target, source) in &install_spec.lua {
        let absolute_source = build_dir.join(source);
        utils::copy_lua_to_module_path(&absolute_source, target, &output_paths.src)?;
        progress.set_position(progress.position() + 1);
    }
    if lib_len > 0 {
        progress.set_message("Compiling C libraries...");
    }
    for (target, source) in &install_spec.lib {
        utils::compile_c_files(
            &vec![build_dir.join(source)],
            target,
            &output_paths.lib,
            lua,
        )?;
        progress.set_position(progress.position() + 1);
    }
    if lib_len > 0 {
        progress.set_message("Copying binaries...");
    }
    for (target, source) in &install_spec.bin {
        std::fs::copy(build_dir.join(source), tree.bin().join(target))?;
        progress.set_position(progress.position() + 1);
    }
    Ok(())
}

pub async fn build(
    progress: &ProgressBar,
    rockspec: Rockspec,
    pinned: PinnedState,
    constraint: LockConstraint,
    behaviour: BuildBehaviour,
    config: &Config,
) -> Result<LocalPackage, BuildError> {
    progress.set_message(format!(
        "Building {}@{}...",
        rockspec.package, rockspec.version
    ));

    let lua_version = rockspec.lua_version_from_config(config)?;

    let tree = Tree::new(config.tree().clone(), lua_version.clone())?;

    let temp_dir = tempdir::TempDir::new(&rockspec.package.to_string())?;

    // Install the source in order to build.
    let rock_source = rockspec.source.current_platform();
    if let Err(err) = operations::fetch_src(progress, temp_dir.path(), rock_source).await {
        let package = RemotePackage::new(rockspec.package.clone(), rockspec.version.clone());
        progress.println(format!(
            "⚠️ WARNING: Failed to fetch source for {}: {}",
            &package, err
        ));
        progress.println(format!(
            "⚠️ Falling back to .src.rock archive from {}",
            &config.server()
        ));
        operations::fetch_src_rock(progress, &package, temp_dir.path(), config).await?;
    }

    let hashes = LocalPackageHashes {
        rockspec: rockspec.hash()?,
        source: temp_dir.hash()?,
    };

    let mut package = LocalPackage::from(
        &RemotePackage::new(rockspec.package.clone(), rockspec.version.clone()),
        constraint,
        hashes,
    );
    package.spec.pinned = pinned;

    match tree.lockfile()?.get(&package.id()) {
        Some(package) if behaviour == BuildBehaviour::NoForce => Ok(package.clone()),
        _ => {
            let output_paths = tree.rock(&package)?;

            let lua = LuaInstallation::new(&lua_version, config);

            let build_dir = match &rock_source.unpack_dir {
                Some(unpack_dir) => temp_dir.path().join(unpack_dir),
                None => temp_dir.path().into(),
            };

            run_build(progress, &rockspec, &output_paths, &lua, config, &build_dir).await?;

            install(progress, &rockspec, &tree, &output_paths, &lua, &build_dir).await?;

            // Copy over all `copy_directories` to their respective paths
            for directory in &rockspec.build.current_platform().copy_directories {
                for file in walkdir::WalkDir::new(build_dir.join(directory))
                    .into_iter()
                    .flatten()
                {
                    if file.file_type().is_file() {
                        let filepath = file.path();
                        let target = output_paths.etc.join(filepath);
                        std::fs::create_dir_all(target.parent().unwrap())?;
                        std::fs::copy(filepath, target)?;
                    }
                }
            }

            Ok(package)
        }
    }
}
