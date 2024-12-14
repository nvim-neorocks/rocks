use itertools::Itertools;
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    str::FromStr,
};
use walkdir::WalkDir;

use crate::{
    build::utils,
    config::Config,
    lua_installation::LuaInstallation,
    progress::{
        Progress::{self},
        ProgressBar,
    },
    rockspec::{Build, BuiltinBuildSpec, LuaModule, ModuleSpec},
    tree::RockLayout,
};

use super::BuildError;

impl Build for BuiltinBuildSpec {
    type Err = BuildError;

    async fn run(
        self,
        output_paths: &RockLayout,
        _no_install: bool,
        lua: &LuaInstallation,
        _config: &Config,
        build_dir: &Path,
        progress: &Progress<ProgressBar>,
    ) -> Result<(), Self::Err> {
        // Detect all Lua modules
        let modules = autodetect_modules(build_dir, source_paths(build_dir, &self.modules))
            .into_iter()
            .chain(self.modules)
            .collect::<HashMap<_, _>>();

        progress.map(|p| p.set_position(modules.len() as u64));

        for (destination_path, module_type) in modules.iter() {
            match module_type {
                ModuleSpec::SourcePath(source) => {
                    if source.extension().map(|ext| ext == "c").unwrap_or(false) {
                        progress.map(|p| {
                            p.set_message(format!(
                                "Compiling {} -> {}...",
                                &source.to_string_lossy(),
                                &destination_path
                            ))
                        });
                        let absolute_source_paths = vec![build_dir.join(source)];
                        utils::compile_c_files(
                            &absolute_source_paths,
                            destination_path,
                            &output_paths.lib,
                            lua,
                        )?
                    } else {
                        progress.map(|p| {
                            p.set_message(format!(
                                "Copying {} to {}...",
                                &source.to_string_lossy(),
                                &destination_path
                            ))
                        });
                        let absolute_source_path = build_dir.join(source);
                        utils::copy_lua_to_module_path(
                            &absolute_source_path,
                            destination_path,
                            &output_paths.src,
                        )?
                    }
                }
                ModuleSpec::SourcePaths(files) => {
                    progress.map(|p| p.set_message("Compiling C files..."));
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
                    progress.map(|p| p.set_message("Compiling C modules..."));
                    utils::compile_c_modules(
                        data,
                        build_dir,
                        destination_path,
                        &output_paths.lib,
                        lua,
                    )?
                }
            }
        }

        for relative_path in autodetect_bin_scripts(build_dir).iter() {
            let source = build_dir.join("src").join("bin").join(relative_path);
            let target = output_paths.bin.join(relative_path);
            std::fs::create_dir_all(target.parent().unwrap())?;
            std::fs::copy(source, target)?;
        }

        Ok(())
    }
}

fn source_paths(build_dir: &Path, modules: &HashMap<LuaModule, ModuleSpec>) -> HashSet<PathBuf> {
    modules
        .iter()
        .flat_map(|(_, spec)| match spec {
            ModuleSpec::SourcePath(path_buf) => vec![path_buf],
            ModuleSpec::SourcePaths(vec) => vec.iter().collect_vec(),
            ModuleSpec::ModulePaths(module_paths) => module_paths.sources.iter().collect_vec(),
        })
        .map(|path| build_dir.join(path))
        .collect()
}

fn autodetect_modules(
    build_dir: &Path,
    exclude: HashSet<PathBuf>,
) -> HashMap<LuaModule, ModuleSpec> {
    WalkDir::new(build_dir.join("src"))
        .into_iter()
        .chain(WalkDir::new(build_dir.join("lua")))
        .chain(WalkDir::new(build_dir.join("lib")))
        .filter_map(|file| {
            file.ok().and_then(|file| {
                let is_lua_file = PathBuf::from(file.file_name())
                    .extension()
                    .map(|ext| ext == "lua")
                    .unwrap_or(false);
                if is_lua_file && !exclude.contains(&file.clone().into_path()) {
                    Some(file)
                } else {
                    None
                }
            })
        })
        .map(|file| {
            let diff: PathBuf =
                pathdiff::diff_paths(build_dir.join(file.clone().into_path()), build_dir)
                    .expect("failed to autodetect modules");

            // NOTE(vhyrro): You may ask why we convert all paths to Lua module paths
            // just to convert them back later in the `run()` stage.
            //
            // The rockspec requires the format to be like this, and representing our
            // data in this form allows us to respect any overrides made by the user (which follow
            // the `module.name` format, not our internal one).
            let pathbuf = diff.components().skip(1).collect::<PathBuf>();
            let mut lua_module = LuaModule::from_pathbuf(pathbuf);

            // NOTE(mrcjkb): `LuaModule` does not parse as "<module>.init" from files named "init.lua"
            // To make sure we don't change the file structure when installing, we append it here.
            if file.file_name().to_string_lossy().as_bytes() == b"init.lua" {
                lua_module = lua_module.join(&LuaModule::from_str("init").unwrap())
            }

            (lua_module, ModuleSpec::SourcePath(diff))
        })
        .collect()
}

fn autodetect_bin_scripts(build_dir: &Path) -> Vec<PathBuf> {
    WalkDir::new(build_dir.join("src").join("bin"))
        .into_iter()
        .filter_map(|file| file.ok())
        .filter(|file| file.clone().into_path().is_file())
        .map(|file| {
            let diff = pathdiff::diff_paths(build_dir.join(file.into_path()), build_dir)
                .expect("failed to autodetect bin scripts");
            diff.components().skip(2).collect::<PathBuf>()
        })
        .collect()
}
