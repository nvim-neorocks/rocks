use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use crate::{
    config::Config,
    lua_installation::LuaInstallation,
    rockspec::{utils, Build, BuiltinBuildSpec, LuaModule, ModuleSpec},
    tree::RockLayout,
};
use indicatif::{MultiProgress, ProgressBar};
use walkdir::WalkDir;

use super::BuildError;

impl Build for BuiltinBuildSpec {
    type Err = BuildError;

    async fn run(
        self,
        progress: &MultiProgress,
        output_paths: &RockLayout,
        _no_install: bool,
        lua: &LuaInstallation,
        _config: &Config,
        build_dir: &Path,
    ) -> Result<(), Self::Err> {
        // Detect all Lua modules
        let modules = autodetect_modules(build_dir)
            .into_iter()
            .chain(self.modules)
            .collect::<HashMap<_, _>>();

        let bar = progress.add(ProgressBar::new(modules.len() as u64));
        for (counter, (destination_path, module_type)) in modules.iter().enumerate() {
            match module_type {
                ModuleSpec::SourcePath(source) => {
                    if source.extension().map(|ext| ext == "c").unwrap_or(false) {
                        bar.set_message(format!(
                            "Compiling {} -> {}...",
                            &source.to_string_lossy(),
                            &destination_path
                        ));
                        let absolute_source_paths = vec![build_dir.join(source)];
                        utils::compile_c_files(
                            &absolute_source_paths,
                            destination_path,
                            &output_paths.lib,
                            lua,
                        )?
                    } else {
                        bar.set_message(format!(
                            "Copying {} to {}...",
                            &source.to_string_lossy(),
                            &destination_path
                        ));
                        let absolute_source_path = build_dir.join(source);
                        utils::copy_lua_to_module_path(
                            &absolute_source_path,
                            destination_path,
                            &output_paths.src,
                        )?
                    }
                }
                ModuleSpec::SourcePaths(files) => {
                    bar.set_message("Compiling C files...");
                    let absolute_source_paths =
                        files.iter().map(|file| build_dir.join(file)).collect();
                    utils::compile_c_files(
                        &absolute_source_paths,
                        destination_path,
                        &output_paths.lib,
                        lua,
                    )?
                }
                ModuleSpec::ModulePaths(data) => {
                    bar.set_message("Compiling C modules...");
                    utils::compile_c_modules(
                        data,
                        build_dir,
                        destination_path,
                        &output_paths.lib,
                        lua,
                    )?
                }
            }
            bar.set_position(counter as u64);
        }

        Ok(())
    }
}

fn autodetect_modules(build_dir: &Path) -> HashMap<LuaModule, ModuleSpec> {
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
                .expect("failed to autodetect modules");

            // NOTE(vhyrro): You may ask why we convert all paths to Lua module paths
            // just to convert them back later in the `run()` stage.
            //
            // The rockspec requires the format to be like this, and representing our
            // data in this form allows us to respect any overrides made by the user (which follow
            // the `module.name` format, not our internal one).
            let pathbuf = diff.components().skip(1).collect::<PathBuf>();
            let lua_module = LuaModule::from_pathbuf(pathbuf);

            (lua_module, ModuleSpec::SourcePath(diff))
        })
        .collect()
}
