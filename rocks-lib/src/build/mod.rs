use crate::{
    config::Config,
    lua_installation::LuaInstallation,
    rockspec::{utils, Build as _, BuildBackendSpec, RockSourceSpec, Rockspec},
    tree::{RockLayout, Tree},
};
use async_recursion::async_recursion;
use eyre::{OptionExt as _, Result};

mod builtin;
mod fetch;

fn install(
    rockspec: &Rockspec,
    tree: &Tree,
    output_paths: &RockLayout,
    lua: &LuaInstallation,
) -> Result<()> {
    let install_spec = &rockspec.build.current_platform().install;

    for (target, source) in &install_spec.lua {
        utils::copy_lua_to_module_path(source, target, &output_paths.src)?;
    }

    for (target, source) in &install_spec.lib {
        utils::compile_c_files(&vec![source.into()], target, &output_paths.lib, lua)?;
    }

    for (target, source) in &install_spec.bin {
        std::fs::copy(source, tree.bin().join(target))?;
    }

    Ok(())
}

#[async_recursion]
pub async fn build(rockspec: Rockspec, config: &Config) -> Result<()> {
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
        crate::operations::install(dependency_req.clone(), config).await?;
    }

    // TODO(vhyrro): Use a more serious isolation strategy here.
    let temp_dir = tempdir::TempDir::new(&rockspec.package.to_string())?;

    let previous_dir = std::env::current_dir()?;

    std::env::set_current_dir(&temp_dir)?;

    // Install the source in order to build.
    fetch::fetch_src(temp_dir, &rockspec.source.current_platform().source_spec).await?;

    // TODO(vhyrro): Instead of copying bit-by-bit we should instead perform all of these
    // operations in the temporary directory itself and then copy all results over once they've
    // succeeded.

    let output_paths = tree.rock(&rockspec.package, &rockspec.version)?;

    let lua = LuaInstallation::new(lua_version, config)?;

    install(&rockspec, &tree, &output_paths, &lua)?;

    // Copy over all `copy_directories` to their respective paths
    for directory in &rockspec.build.current_platform().copy_directories {
        for file in walkdir::WalkDir::new(directory).into_iter().flatten() {
            if file.file_type().is_file() {
                let filepath = file.path();
                let target = output_paths.etc.join(filepath);
                std::fs::create_dir_all(target.parent().unwrap())?;
                std::fs::copy(filepath, target)?;
            }
        }
    }

    match rockspec.build.default.build_backend.as_ref().cloned() {
        Some(BuildBackendSpec::Builtin(build_spec)) => {
            build_spec.run(rockspec, output_paths, false, &lua)?
        }
        _ => unimplemented!(),
    };

    std::env::set_current_dir(previous_dir)?;

    Ok(())
}
