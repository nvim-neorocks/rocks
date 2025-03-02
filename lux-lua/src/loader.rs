use std::path::PathBuf;

use lux_lib::{
    lockfile::{LocalPackageId, LocalPackageLockType, Lockfile},
    package::PackageReq,
    project::Project,
};
use mlua::prelude::*;
use path_absolutize::Absolutize;

const fn lua_version() -> &'static str {
    if cfg!(feature = "lua51") {
        "5.1"
    } else if cfg!(feature = "lua52") {
        "5.2"
    } else if cfg!(feature = "lua53") {
        "5.3"
    } else if cfg!(feature = "lua54") {
        "5.4"
    } else {
        unreachable!()
    }
}

fn current_file(lua: &Lua) -> String {
    lua.inspect_stack(2)
        .unwrap()
        .source()
        .short_src
        .unwrap()
        .to_string()
}

fn load_file(lua: &Lua, module: String, path: PathBuf) -> mlua::Result<mlua::Function> {
    lua.create_function(move |lua, ()| {
        // We need to handle 3 possible cases:
        // - `src/?.lua`
        // - `src/?/init.lua`
        // - `src/?.so`

        let module_path = module.replace('.', "/");

        if path
            .join("src")
            .join(format!("{}.lua", module_path))
            .exists()
        {
            lua.load("dofile")
                .call::<()>(path.join("src").join(format!("{}.lua", module_path)))
        } else if path
            .join("src")
            .join(&module_path)
            .join("init.lua")
            .exists()
        {
            lua.load("dofile")
                .call::<()>(path.join("src").join(&module_path).join("init.lua"))
        } else if path
            .join("lib")
            .join(format!("{}.so", module_path))
            .exists()
        {
            lua.load("dofile")
                .call::<()>(path.join("lib").join(format!("{}.so", module_path)))
        } else {
            Err(mlua::Error::RuntimeError(format!(
                "module not found: {}",
                module
            )))
        }
    })
}

pub fn load_loader(lua: &Lua) -> mlua::Result<()> {
    let globals = lua.globals();
    let package: LuaTable = globals.get("package")?;
    #[cfg(feature = "lua51")]
    let loaders: LuaTable = package.get("loaders")?;
    #[cfg(not(feature = "lua51"))]
    let loaders: LuaTable = package.get("searchers")?;
    loaders.raw_insert(1, lua.create_function(loader)?)?;

    Ok(())
}

pub fn loader(lua: &Lua, module: String) -> mlua::Result<Option<mlua::Function>> {
    let module_root = match module.split('.').next() {
        Some(module) => module,
        None => return Ok(None),
    };
    let mut current_file = match current_file(lua).as_str() {
        current_file => PathBuf::from(current_file),
        "stdin" => return Ok(None),
    };
    let current_file = current_file.absolutize().into_lua_err()?;

    match lets_find_up::find_up_with(
        "lux.lock",
        lets_find_up::FindUpOptions {
            cwd: current_file.parent().unwrap(),
            kind: lets_find_up::FindUpKind::File,
        },
    )
    .into_lua_err()?
    {
        Some(lock_path) => {
            // Check if we're in a Project and if so, treat the immediate dependencies as our
            // targets.
            match Project::from(lock_path.parent().unwrap()).into_lua_err()? {
                Some(project) => {
                    match project
                        .lockfile()
                        .into_lua_err()?
                        .local_pkg_lock(&LocalPackageLockType::Regular)
                        .has_entrypoint(
                            &PackageReq::new(module_root.to_string(), None).into_lua_err()?,
                        ) {
                        Some(package) => {
                            let path = project.root().join(format!(
                                ".lux/{version}/{id}-{package_name}-{package_version}/",
                                version = lua_version(),
                                id = package.id(),
                                package_name = package.name(),
                                package_version = package.version()
                            ));

                            Ok(Some(load_file(lua, module, path)?))
                        }
                        None => Ok(None),
                    }
                }
                None => {
                    // Get the name of the current module, so we can look up its dependencies in the lockfile.
                    let lockfile = Lockfile::new(lock_path.clone()).into_lua_err()?;

                    // If we're in a lux tree, the path looks like `.lux/5.4/12345678-package_name-1.0.0/src/code.lua`
                    // we need to extract the hash from the path.

                    let module_hash = current_file.into_iter().rev().find_map(|path| {
                        if let [hash, _name, _version, _rest @ ""] =
                            path.to_str().unwrap().splitn(3, '-').collect::<Vec<&str>>()[..4]
                        {
                            Some(hash)
                        } else {
                            None
                        }
                    });

                    if let Some(module_hash) = module_hash {
                        // NOTE(vhyrro): On safety - it's possible that the user *could* tamper
                        // with the lux tree and malform the package hash. In this case, this
                        // should never cause any security-related problems anyway, as we'll
                        // crash right after this function returns None.
                        if let Some(package) = lockfile.get(unsafe {
                            &LocalPackageId::from_unchecked(module_hash.to_string())
                        }) {
                            return package
                                .dependencies()
                                .iter()
                                .find_map(|dep| {
                                    let dep = lockfile.get(dep).unwrap();

                                    if dep.name().to_string() == module {
                                        Some(dep)
                                    } else {
                                        None
                                    }
                                })
                                .and_then(|dep| {
                                    let path = lock_path.parent().unwrap().join(format!(
                                        ".lux/{version}/{id}-{package_name}-{package_version}/",
                                        version = lua_version(),
                                        id = dep.id(),
                                        package_name = dep.name(),
                                        package_version = dep.version()
                                    ));

                                    Some(load_file(lua, module, path))
                                })
                                .transpose();
                        }
                    }

                    Ok(None)
                }
            }
        }
        None => Ok(None),
    }
}
