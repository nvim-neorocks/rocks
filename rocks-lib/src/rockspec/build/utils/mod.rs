use itertools::Itertools;
use std::{
    io,
    path::{Path, PathBuf},
};

use crate::{build::BuildError, lua_installation::LuaInstallation};

use super::ModulePaths;

fn lua_module_to_pathbuf(path: &str, extension: &str) -> PathBuf {
    PathBuf::from(path.replace('.', std::path::MAIN_SEPARATOR_STR) + extension)
}

/// Copies a lua source file to a specific destination. The destination is described by a
/// `module.path` syntax (equivalent to the syntax provided to Lua's `require()` function).
pub fn copy_lua_to_module_path(
    source: &PathBuf,
    target_module_name: &str,
    target_dir: &Path,
) -> io::Result<()> {
    let target = lua_module_to_pathbuf(target_module_name, ".lua");
    let target = target_dir.join(target);

    std::fs::create_dir_all(target.parent().unwrap())?;

    std::fs::copy(source, target)?;

    Ok(())
}

/// Compiles a set of C files into a single dynamic library and places them under `{target_dir}/{target_file}`.
/// # Panics
/// Panics if no parent or no filename can be determined for the target path.
pub fn compile_c_files(
    files: &Vec<PathBuf>,
    target_file: &str,
    target_dir: &Path,
    lua: &LuaInstallation,
) -> Result<(), BuildError> {
    let target = lua_module_to_pathbuf(target_file, std::env::consts::DLL_SUFFIX);
    let target = target_dir.join(target);

    let parent = target.parent().unwrap_or_else(|| {
        panic!(
            "Couldn't determine parent for path {}",
            target.to_str().unwrap_or("")
        )
    });
    let file = target.file_name().unwrap_or_else(|| {
        panic!(
            "Couldn't determine filename for path {}",
            target.to_str().unwrap_or("")
        )
    });

    std::fs::create_dir_all(parent)?;

    // TODO: Use `target-lexicon` data here instead, it's more reliable.

    // See https://github.com/rust-lang/cc-rs/issues/594#issuecomment-2110551057

    let mut build = cc::Build::new();
    let build = build
        .cargo_metadata(false)
        .debug(false)
        .files(files)
        .host(std::env::consts::OS)
        .includes(&lua.include_dir)
        .opt_level(3)
        .out_dir(parent)
        .target(std::env::consts::ARCH);
    let objects = build.compile_intermediates();
    build
        .get_compiler()
        .to_command()
        .args(["-shared", "-o"])
        .arg(parent.join(file))
        .args(&objects)
        .status()?;
    Ok(())
}

/// the extension for Lua libraries.
pub fn lua_lib_extension() -> &'static str {
    if cfg!(target_os = "windows") {
        "dll"
    } else {
        "so"
    }
}

/// Compiles a set of C files (with extra metadata) to a given destination.
/// # Panics
/// Panics if no filename for the target path can be determined.
pub fn compile_c_modules(
    data: &ModulePaths,
    source_dir: &Path,
    target_file: &str,
    target_dir: &Path,
    lua: &LuaInstallation,
) -> Result<(), BuildError> {
    let target = lua_module_to_pathbuf(target_file, std::env::consts::DLL_SUFFIX);
    let target = target_dir.join(target);

    std::fs::create_dir_all(target.parent().unwrap())?;

    let mut build = cc::Build::new();
    let source_files = data
        .sources
        .iter()
        .map(|dir| source_dir.join(dir))
        .collect_vec();
    let include_dirs = data
        .incdirs
        .iter()
        .map(|dir| source_dir.join(dir))
        .collect_vec();
    let build = build
        .cargo_metadata(false)
        .debug(false)
        .files(source_files)
        .host(std::env::consts::OS)
        .includes(&include_dirs)
        .includes(&lua.include_dir)
        .opt_level(3)
        .out_dir(target_dir)
        .shared_flag(true)
        .target(std::env::consts::ARCH);

    // `cc::Build` has no `defines()` function, so we manually feed in the
    // definitions in a verbose loop
    for (name, value) in &data.defines {
        build.define(name, value.as_deref());
    }

    for libdir in &data.libdirs {
        build.flag(&("-L".to_string() + source_dir.join(libdir).to_str().unwrap()));
    }

    for library in &data.libraries {
        build.flag(&("-l".to_string() + source_dir.join(library).to_str().unwrap()));
    }

    let file = target.file_name().unwrap_or_else(|| {
        panic!(
            "Couldn't determine filename for path {}",
            target.to_str().unwrap_or("")
        )
    });

    build.try_compile(file.to_str().unwrap())?;

    Ok(())
}
