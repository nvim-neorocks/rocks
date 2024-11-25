use itertools::Itertools;
use std::{
    io,
    path::{Path, PathBuf},
    process::Output,
};
use target_lexicon::Triple;

use crate::{build::BuildError, lua_installation::LuaInstallation};

use super::{LuaModule, ModulePaths};

/// Copies a lua source file to a specific destination. The destination is described by a
/// `module.path` syntax (equivalent to the syntax provided to Lua's `require()` function).
pub fn copy_lua_to_module_path(
    source: &PathBuf,
    target_module: &LuaModule,
    target_dir: &Path,
) -> io::Result<()> {
    let target = target_dir.join(target_module.to_lua_path());

    std::fs::create_dir_all(target.parent().unwrap())?;

    std::fs::copy(source, target)?;

    Ok(())
}

fn validate_output(output: Output) -> Result<(), BuildError> {
    if !output.status.success() {
        return Err(BuildError::CommandFailure {
            status: output.status,
            stdout: String::from_utf8_lossy(&output.stdout).into(),
            stderr: String::from_utf8_lossy(&output.stderr).into(),
        });
    }
    Ok(())
}

/// Compiles a set of C files into a single dynamic library and places them under `{target_dir}/{target_file}`.
/// # Panics
/// Panics if no parent or no filename can be determined for the target path.
pub fn compile_c_files(
    files: &Vec<PathBuf>,
    target_module: &LuaModule,
    target_dir: &Path,
    lua: &LuaInstallation,
) -> Result<(), BuildError> {
    let target = target_dir.join(target_module.to_lib_path());

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

    let host = Triple::host();

    // See https://github.com/rust-lang/cc-rs/issues/594#issuecomment-2110551057

    let mut build = cc::Build::new();
    let intermediate_dir = tempdir::TempDir::new(target_module.as_str())?;
    let build = build
        .cargo_metadata(false)
        .debug(false)
        .files(files)
        .host(std::env::consts::OS)
        .includes(&lua.include_dir)
        .opt_level(3)
        .out_dir(intermediate_dir)
        .target(&host.to_string());
    let objects = build.compile_intermediates();
    let output = build
        .get_compiler()
        .to_command()
        .args(["-shared", "-o"])
        .arg(parent.join(file))
        .args(&objects)
        .output()?;
    validate_output(output)?;
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
    target_module: &LuaModule,
    target_dir: &Path,
    lua: &LuaInstallation,
) -> Result<(), BuildError> {
    let target = target_dir.join(target_module.to_lib_path());

    let parent = target.parent().unwrap_or_else(|| {
        panic!(
            "Couldn't determine parent for path {}",
            target.to_str().unwrap_or("")
        )
    });
    std::fs::create_dir_all(parent)?;

    let host = Triple::host();

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

    let intermediate_dir = tempdir::TempDir::new(target_module.as_str())?;
    let build = build
        .cargo_metadata(false)
        .debug(false)
        .files(source_files)
        .host(std::env::consts::OS)
        .includes(&include_dirs)
        .includes(&lua.include_dir)
        .opt_level(3)
        .out_dir(intermediate_dir)
        .shared_flag(true)
        .target(&host.to_string());

    // `cc::Build` has no `defines()` function, so we manually feed in the
    // definitions in a verbose loop
    for (name, value) in &data.defines {
        build.define(name, value.as_deref());
    }

    let file = target.file_name().unwrap_or_else(|| {
        panic!(
            "Couldn't determine filename for path {}",
            target.to_str().unwrap_or("")
        )
    });
    // See https://github.com/rust-lang/cc-rs/issues/594#issuecomment-2110551057
    let objects = build.compile_intermediates();

    let libdir_args = data
        .libdirs
        .iter()
        .map(|libdir| format!("-L{}", source_dir.join(libdir).to_str().unwrap()));

    let library_args = data
        .libraries
        .iter()
        .map(|library| format!("-l{}", library.to_str().unwrap()));

    let output = build
        .get_compiler()
        .to_command()
        .args(["-shared", "-o"])
        .arg(parent.join(file))
        .args(&objects)
        .args(libdir_args)
        .args(library_args)
        .output()?;
    validate_output(output)?;

    Ok(())
}
