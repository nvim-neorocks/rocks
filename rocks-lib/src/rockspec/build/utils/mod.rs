use std::path::{Path, PathBuf};
use eyre::Result;

use super::ModulePaths;

fn lua_module_to_pathbuf(path: &str, extension: &str) -> PathBuf {
    PathBuf::from(path.replace('.', std::path::MAIN_SEPARATOR_STR) + extension)
}

/// Copies a lua source file to a specific destination. The destination is described by a
/// `module.path` syntax (equivalent to the syntax provided to Lua's `require()` function).
pub fn copy_lua_to_module_path(source: &PathBuf, target_module_name: &str, target_dir: &Path) -> Result<()> {
    let target = lua_module_to_pathbuf(target_module_name, ".lua");
    let target = target_dir.join(target);

    std::fs::create_dir_all(target.parent().unwrap())?;

    std::fs::copy(source, target)?;

    Ok(())
}

/// Compiles a set of C files into a single dynamic library and places them under `{target_dir}/{target_file}`.
pub fn compile_c_files(files: &Vec<PathBuf>, target_file: &str, target_dir: &Path) -> Result<()> {
    let target =
        lua_module_to_pathbuf(target_file, std::env::consts::DLL_SUFFIX);
    let target = target_dir.join(target);

    let parent = target.parent().expect("TODO");
    let file = target.file_name().expect("TODO");

    std::fs::create_dir_all(parent)?;

    cc::Build::new()
        .cargo_metadata(false)
        .debug(false)
        .files(files)
        .host(std::env::consts::OS)
        .opt_level(3)
        .out_dir(parent)
        .shared_flag(true)
        .target(std::env::consts::ARCH)
        .try_compile(file.to_str().unwrap())?;

    Ok(())
}

/// Compiles a set of C files (with extra metadata) to a given destination.
pub fn compile_c_modules(data: &ModulePaths, target_file: &str, target_dir: &Path) -> Result<()> {
    let target =
        lua_module_to_pathbuf(target_file, std::env::consts::DLL_SUFFIX);
    let target = target_dir.join(target);

    std::fs::create_dir_all(target.parent().unwrap())?;

    let mut build = cc::Build::new();
    let build = build
        .cargo_metadata(false)
        .debug(false)
        .host(std::env::consts::OS)
        .opt_level(3)
        .out_dir(std::env::current_dir()?)
        .target(std::env::consts::ARCH)
        .shared_flag(true)
        .files(&data.sources)
        .includes(&data.incdirs);

    // `cc::Build` has no `defines()` function, so we manually feed in the
    // definitions in a verbose loop
    for (name, value) in &data.defines {
        build.define(name, value.as_deref());
    }

    for libdir in &data.libdirs {
        build.flag(&("-L".to_string() + libdir.to_str().unwrap()));
    }

    for library in &data.libraries {
        build.flag(&("-l".to_string() + library.to_str().unwrap()));
    }

    build.try_compile(target.to_str().unwrap())?;

    Ok(())
}
