use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use crate::{
    config::Config,
    lua_installation::LuaInstallation,
    rockspec::{utils, Build, BuiltinBuildSpec, ModuleSpec},
    tree::RockLayout,
};
use eyre::{OptionExt as _, Result};
use itertools::Itertools as _;
use walkdir::WalkDir;

impl Build for BuiltinBuildSpec {
    fn run(
        self,
        output_paths: &RockLayout,
        _no_install: bool,
        lua: &LuaInstallation,
        _config: &Config,
        build_dir: &Path,
    ) -> Result<()> {
        // Detect all Lua modules
        let modules = autodetect_modules(build_dir)?
            .into_iter()
            .chain(self.modules)
            .collect::<HashMap<_, _>>();

        for (destination_path, module_type) in &modules {
            match module_type {
                ModuleSpec::SourcePath(source) => {
                    let absolute_source_path = build_dir.join(source);
                    utils::copy_lua_to_module_path(
                        &absolute_source_path,
                        destination_path,
                        &output_paths.src,
                    )?
                }
                ModuleSpec::SourcePaths(files) => {
                    let absolute_source_paths =
                        files.iter().map(|file| build_dir.join(file)).collect();
                    utils::compile_c_files(
                        &absolute_source_paths,
                        destination_path,
                        &output_paths.lib,
                        lua,
                    )?
                }
                ModuleSpec::ModulePaths(data) => utils::compile_c_modules(
                    data,
                    build_dir,
                    destination_path,
                    &output_paths.lib,
                    lua,
                )?,
            }
        }

        Ok(())
    }
}

fn autodetect_modules(build_dir: &Path) -> Result<HashMap<String, ModuleSpec>> {
    WalkDir::new(build_dir.join("src"))
        .into_iter()
        .chain(WalkDir::new(build_dir.join("lua")))
        .chain(WalkDir::new(build_dir.join("lib")))
        .filter_map(|file| {
            file.ok().and_then(|file| {
                if PathBuf::from(file.file_name())
                    .extension()
                    .map(|ext| ext == "lua")
                    .unwrap_or(false)
                    && !matches!(
                        file.file_name().to_string_lossy().as_bytes(),
                        b"spec" | b".luarocks" | b"lua_modules" | b"test.lua" | b"tests.lua"
                    )
                {
                    Some(file)
                } else {
                    None
                }
            })
        })
        .map(|file| {
            let diff: PathBuf = pathdiff::diff_paths(build_dir.join(file.into_path()), build_dir)
                .ok_or_eyre("unable to autodetect modules")?;

            // NOTE(vhyrro): You may ask why we convert all paths to Lua module paths
            // just to convert them back later in the `run()` stage.
            //
            // The rockspec requires the format to be like this, and representing our
            // data in this form allows us to respect any overrides made by the user (which follow
            // the `module.name` format, not our internal one).
            let lua_module_path = diff
                .components()
                .skip(1)
                .collect::<PathBuf>()
                .to_string_lossy()
                .trim_end_matches(".lua")
                .replace(std::path::MAIN_SEPARATOR_STR, ".");

            Ok((lua_module_path, ModuleSpec::SourcePath(diff)))
        })
        .try_collect()
}
