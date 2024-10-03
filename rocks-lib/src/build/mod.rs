use std::path::Path;

use crate::{
    config::Config,
    lua_installation::LuaInstallation,
    progress::with_spinner,
    rockspec::{utils, Build as _, BuildBackendSpec, Rockspec},
    tree::{RockLayout, Tree},
};
use async_recursion::async_recursion;
use eyre::{OptionExt as _, Result};
use indicatif::MultiProgress;
mod builtin;
mod fetch;
mod make;
pub mod variables;

async fn run_build(
    progress: &MultiProgress,
    rockspec: &Rockspec,
    output_paths: &RockLayout,
    lua: &LuaInstallation,
    config: &Config,
    build_dir: &Path,
) -> Result<()> {
    with_spinner(progress, "🛠️ Building...".into(), || async {
        match rockspec.build.default.build_backend.to_owned() {
            Some(BuildBackendSpec::Builtin(build_spec)) => {
                build_spec.run(output_paths, false, lua, config, build_dir)?
            }
            Some(BuildBackendSpec::Make(make_spec)) => {
                make_spec.run(output_paths, false, lua, config, build_dir)?
            }
            _ => unimplemented!(),
        }
        Ok(())
    })
    .await?;
    Ok(())
}

async fn install(
    progress: &MultiProgress,
    rockspec: &Rockspec,
    tree: &Tree,
    output_paths: &RockLayout,
    lua: &LuaInstallation,
    build_dir: &Path,
) -> Result<()> {
    with_spinner(
        progress,
        format!("💻 Installing {} {}", rockspec.package, rockspec.version),
        || async {
            let install_spec = &rockspec.build.current_platform().install;
            for (target, source) in &install_spec.lua {
                let absolute_source = build_dir.join(source);
                utils::copy_lua_to_module_path(&absolute_source, target, &output_paths.src)?;
            }
            for (target, source) in &install_spec.lib {
                utils::compile_c_files(
                    &vec![build_dir.join(source)],
                    target,
                    &output_paths.lib,
                    lua,
                )?;
            }
            for (target, source) in &install_spec.bin {
                std::fs::copy(build_dir.join(source), tree.bin().join(target))?;
            }
            Ok(())
        },
    )
    .await?;
    Ok(())
}

#[async_recursion]
pub async fn build(progress: &MultiProgress, rockspec: Rockspec, config: &Config) -> Result<()> {
    // TODO(vhyrro): Create a unified way of accessing the Lua version with centralized error
    // handling.
    let lua_version = rockspec.lua_version();
    let lua_version = config.lua_version().or(lua_version.as_ref()).ok_or_eyre(
        "lua version not set! Please provide a version through `--lua-version <ver>`",
    )?;

    let tree = Tree::new(config.tree().clone(), lua_version.clone())?;

    // Recursively build all dependencies.
    // TODO: Handle build dependencies as well.
    for dependency_req in rockspec
        .build_dependencies
        .current_platform()
        .iter()
        .filter(|req| tree.has_rock(req).is_none())
    {
        // NOTE: This recursive operation will create another `tree` object
        // which will in turn acquire another lock to the filesystem.
        // Once we implement fs locks, this could become a problem, so a more sophisticated
        // filesystem acquiring mechanism will have to be devised.
        crate::operations::install(progress, dependency_req.clone(), config).await?;
    }

    let temp_dir = tempdir::TempDir::new(&rockspec.package.to_string())?;

    // Install the source in order to build.
    let rock_source = rockspec.source.current_platform();
    fetch::fetch_src(progress, temp_dir.path(), rock_source).await?;

    // TODO(vhyrro): Instead of copying bit-by-bit we should instead perform all of these
    // operations in the temporary directory itself and then copy all results over once they've
    // succeeded.

    let output_paths = tree.rock(&rockspec.package, &rockspec.version)?;

    let lua = LuaInstallation::new(lua_version, config)?;

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

    Ok(())
}
