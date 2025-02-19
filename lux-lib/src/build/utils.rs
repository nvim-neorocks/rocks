use crate::{
    build::BuildError,
    config::Config,
    lua_installation::LuaInstallation,
    lua_rockspec::{LuaModule, ModulePaths},
    tree::RockLayout,
};
use itertools::Itertools;
use shlex::try_quote;
use std::{
    io,
    path::{Path, PathBuf},
    process::Output,
};
use target_lexicon::Triple;

use super::variables::HasVariables;

/// Copies a lua source file to a specific destination. The destination is described by a
/// `module.path` syntax (equivalent to the syntax provided to Lua's `require()` function).
pub(crate) fn copy_lua_to_module_path(
    source: &PathBuf,
    target_module: &LuaModule,
    target_dir: &Path,
) -> io::Result<()> {
    let target = target_dir.join(target_module.to_lua_path());

    std::fs::create_dir_all(target.parent().unwrap())?;

    std::fs::copy(source, target)?;

    Ok(())
}

pub(crate) fn recursive_copy_dir(src: &PathBuf, dest: &Path) -> Result<(), io::Error> {
    if src.exists() {
        for file in walkdir::WalkDir::new(src)
            .into_iter()
            .flatten()
            .filter(|file| file.file_type().is_file())
        {
            let relative_src_path: PathBuf =
                pathdiff::diff_paths(src.join(file.clone().into_path()), src)
                    .expect("failed to copy directories!");
            let filepath = file.path();
            let target = dest.join(relative_src_path);
            std::fs::create_dir_all(target.parent().unwrap())?;
            std::fs::copy(filepath, target)?;
        }
    }
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
pub(crate) fn compile_c_files(
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
        .cargo_output(false)
        .cargo_metadata(false)
        .cargo_debug(false)
        .cargo_warnings(false)
        .debug(false)
        .files(files)
        .host(std::env::consts::OS)
        .opt_level(3)
        .out_dir(intermediate_dir)
        .target(&host.to_string());

    for arg in lua.compile_args() {
        build.flag(&arg);
    }

    let objects = build.try_compile_intermediates()?;
    let output = build
        .get_compiler()
        .to_command()
        .args(["-shared", "-o"])
        .arg(parent.join(file))
        .arg(format!("-L{}", lua.lib_dir.to_string_lossy())) // TODO: In luarocks, this is behind a link_lua_explicitly config option Library directory
        .args(lua.link_args())
        .args(&objects)
        .output()?;
    validate_output(output)?;
    Ok(())
}

// TODO: (#261): special cases for mingw/cygwin?

/// the extension for Lua libraries.
pub(crate) fn lua_lib_extension() -> &'static str {
    if cfg!(target_os = "windows") {
        "dll"
    } else {
        "so"
    }
}

/// the extension for Lua objects.
pub(crate) fn lua_obj_extension() -> &'static str {
    if cfg!(target_os = "windows") {
        "obj"
    } else {
        "o"
    }
}

pub(crate) fn default_cflags() -> &'static str {
    if cfg!(target_os = "windows") {
        "/nologo /MD /O2"
    } else {
        "-O2"
    }
}

pub(crate) fn default_libflag() -> &'static str {
    if cfg!(target_os = "macos") {
        "-bundle -undefined dynamic_lookup -all_load"
    } else if cfg!(target_os = "windows") {
        "/nologo /dll"
    } else {
        "-shared"
    }
}

/// Compiles a set of C files (with extra metadata) to a given destination.
/// # Panics
/// Panics if no filename for the target path can be determined.
pub(crate) fn compile_c_modules(
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
        .cargo_output(false)
        .cargo_metadata(false)
        .cargo_debug(false)
        .cargo_warnings(false)
        .debug(false)
        .files(source_files)
        .host(std::env::consts::OS)
        .includes(&include_dirs)
        .opt_level(3)
        .out_dir(intermediate_dir)
        .shared_flag(true)
        .target(&host.to_string());

    for arg in lua.compile_args() {
        build.flag(&arg);
    }

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
    let objects = build.try_compile_intermediates()?;

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
        .arg(format!("-L{}", lua.lib_dir.to_string_lossy())) // TODO: In luarocks, this is behind a link_lua_explicitly config option Library directory
        .args(lua.link_args())
        .args(&objects)
        .args(libdir_args)
        .args(library_args)
        .output()?;
    validate_output(output)?;

    Ok(())
}

pub(crate) fn substitute_variables(
    input: &str,
    output_paths: &RockLayout,
    lua: &LuaInstallation,
    config: &Config,
) -> String {
    let mut substituted = output_paths.substitute_variables(input);
    substituted = lua.substitute_variables(&substituted);
    config.substitute_variables(&substituted)
}

pub(crate) fn escape_path(path: &Path) -> String {
    let path_str = format!("{}", path.display());
    if cfg!(windows) {
        format!("\"{}\"", path_str)
    } else {
        try_quote(&path_str)
            .map(|str| str.to_string())
            .unwrap_or(format!("'{}'", path_str))
    }
}
