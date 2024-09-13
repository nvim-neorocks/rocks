use crate::{
    config::{Config, LuaVersion},
    lua_installation::LuaInstallation,
    rockspec::{utils, Build as _, BuildBackendSpec, RockSourceSpec, Rockspec},
    tree::{RockLayout, Tree},
};
use eyre::Result;
use git2::Repository;

mod builtin;

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

pub fn build(rockspec: Rockspec, config: &Config) -> Result<()> {
    // TODO(vhyrro): Use a more serious isolation strategy here.
    let temp_dir = tempdir::TempDir::new(&rockspec.package.to_string())?;

    let previous_dir = std::env::current_dir()?;

    std::env::set_current_dir(&temp_dir)?;

    // Install the source in order to build.
    match &rockspec.source.current_platform().source_spec {
        RockSourceSpec::Git(git) => {
            let repo = Repository::clone(&git.url.to_string(), &temp_dir)?;

            if let Some(commit_hash) = &git.checkout_ref {
                let (object, _) = repo.revparse_ext(commit_hash)?;
                repo.checkout_tree(&object, None)?;
            }
        }
        RockSourceSpec::Url(_) => todo!(),
        RockSourceSpec::File(_) => todo!(),
        RockSourceSpec::Cvs(_) => unimplemented!(),
        RockSourceSpec::Mercurial(_) => unimplemented!(),
        RockSourceSpec::Sscm(_) => unimplemented!(),
        RockSourceSpec::Svn(_) => unimplemented!(),
    };

    // TODO(vhyrro): Instead of copying bit-by-bit we should instead perform all of these
    // operations in the temporary directory itself and then copy all results over once they've
    // succeeded.

    let lua_version = LuaVersion::try_from(&rockspec)?;

    let tree = Tree::new(
        config.tree.clone(),
        config.lua_version.clone().unwrap_or(lua_version.clone()),
    )?;

    let output_paths = tree.rock(&rockspec.package, &rockspec.version)?;

    let lua = LuaInstallation::new(config.lua_version.as_ref().unwrap_or(&lua_version), config)?;

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

    // TODO: Ensure dependencies and build dependencies.
    match rockspec.build.default.build_backend.as_ref().cloned() {
        Some(BuildBackendSpec::Builtin(build_spec)) => {
            build_spec.run(rockspec, output_paths, false, &lua)?
        }
        _ => unimplemented!(),
    };

    std::env::set_current_dir(previous_dir)?;

    Ok(())
}
