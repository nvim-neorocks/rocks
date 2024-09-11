use eyre::Result;
use std::path::PathBuf;
use target_lexicon::Triple;

use crate::config::{Config, LuaVersion};

pub struct Lua {
    pub include_dir: PathBuf,
    pub lib_dir: PathBuf,
}

impl Lua {
    pub fn new(version: &LuaVersion) -> Result<Self> {
        let output = Self::path(version)?;

        if output.exists() {
            Ok(Lua {
                include_dir: output.join("include"),
                lib_dir: output.join("lib"),
            })
        } else {
            let host = Triple::host();
            let target = &host.to_string();
            let host_operating_system = &host.operating_system.to_string();

            let (include_dir, lib_dir) = match version {
                LuaVersion::LuaJIT => {
                    let build = lua_src::Build::new()
                        .target(target)
                        .host(host_operating_system)
                        .out_dir(output)
                        .build(match version {
                            LuaVersion::Lua51 => lua_src::Version::Lua51,
                            LuaVersion::Lua52 => lua_src::Version::Lua52,
                            LuaVersion::Lua53 => lua_src::Version::Lua53,
                            LuaVersion::Lua54 => lua_src::Version::Lua54,
                            LuaVersion::LuaJIT => unreachable!(),
                        });

                    (
                        build.include_dir().to_path_buf(),
                        build.lib_dir().to_path_buf(),
                    )
                }
                _ => {
                    let build = luajit_src::Build::new()
                        .target(target)
                        .host(host_operating_system)
                        .out_dir(output)
                        .build();

                    (
                        build.include_dir().to_path_buf(),
                        build.lib_dir().to_path_buf(),
                    )
                }
            };

            Ok(Lua {
                include_dir,
                lib_dir,
            })
        }
    }

    pub fn path(version: &LuaVersion) -> Result<PathBuf> {
        Ok(Config::get_default_cache_path()?
            .join("lua")
            .join(version.to_string()))
    }
}
