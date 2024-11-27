use std::io;
use std::{path::PathBuf, process::Command};
use target_lexicon::Triple;
use thiserror::Error;

use crate::{
    build::variables::{self, HasVariables},
    config::{Config, LuaVersion},
    package::PackageVersion,
};

pub struct LuaInstallation {
    pub include_dir: PathBuf,
    pub lib_dir: PathBuf,
    /// The argument used to link Lua, e.g. "-llua"
    pub link_lua_arg: String,
}

impl LuaInstallation {
    pub fn new(version: &LuaVersion, config: &Config) -> Self {
        let output = Self::path(version, config);

        let link_lua_arg = match version {
            LuaVersion::LuaJIT => "-lluajit-5.1",
            LuaVersion::LuaJIT52 => "-lluajit-5.2",
            _ => "-llua",
        }
        .into();

        if output.exists() {
            LuaInstallation {
                include_dir: output.join("include"),
                lib_dir: output.join("lib"),
                link_lua_arg,
            }
        } else {
            let host = Triple::host();
            let target = &host.to_string();
            let host_operating_system = &host.operating_system.to_string();

            let (include_dir, lib_dir) = match version {
                LuaVersion::LuaJIT | LuaVersion::LuaJIT52 => {
                    // XXX: luajit_src panics if this is not set.
                    let target_pointer_width =
                        std::env::var("CARGO_CFG_TARGET_POINTER_WIDTH").unwrap_or("64".into());
                    std::env::set_var("CARGO_CFG_TARGET_POINTER_WIDTH", target_pointer_width);
                    let build = luajit_src::Build::new()
                        .target(target)
                        .host(host_operating_system)
                        .out_dir(output)
                        .lua52compat(matches!(version, LuaVersion::LuaJIT52))
                        .build();

                    (
                        build.include_dir().to_path_buf(),
                        build.lib_dir().to_path_buf(),
                    )
                }
                _ => {
                    let build = lua_src::Build::new()
                        .target(target)
                        .host(host_operating_system)
                        .out_dir(output)
                        .build(match version {
                            LuaVersion::Lua51 => lua_src::Version::Lua51,
                            LuaVersion::Lua52 => lua_src::Version::Lua52,
                            LuaVersion::Lua53 => lua_src::Version::Lua53,
                            LuaVersion::Lua54 => lua_src::Version::Lua54,
                            _ => unreachable!(),
                        });

                    (
                        build.include_dir().to_path_buf(),
                        build.lib_dir().to_path_buf(),
                    )
                }
            };

            LuaInstallation {
                include_dir,
                lib_dir,
                link_lua_arg,
            }
        }
    }

    pub fn path(version: &LuaVersion, config: &Config) -> PathBuf {
        config.lua_dir().join(version.to_string())
    }
}

impl HasVariables for LuaInstallation {
    fn substitute_variables(&self, input: String) -> String {
        variables::substitute(
            |var_name| {
                let dir = match var_name {
                    "LUA_INCDIR" => Some(self.include_dir.to_owned()),
                    "LUA_LIBDIR" => Some(self.lib_dir.to_owned()),
                    // TODO: "LUA" ?
                    _ => None,
                }?;
                Some(dir.to_string_lossy().to_string())
            },
            input,
        )
    }
}

#[derive(Error, Debug)]
pub enum GetLuaVersionError {
    #[error("failed to run {0}: {1}")]
    RunLuaCommandError(String, io::Error),
    #[error("failed to parse Lua version from output: {0}")]
    ParseLuaVersionError(String),
    #[error(transparent)]
    PackageVersionParseError(#[from] crate::package::PackageVersionParseError),
    #[error(transparent)]
    LuaVersionError(#[from] crate::config::LuaVersionError),
}

pub fn get_installed_lua_version(lua_cmd: &str) -> Result<PackageVersion, GetLuaVersionError> {
    let output = match Command::new(lua_cmd).arg("-v").output() {
        Ok(output) => Ok(output),
        Err(err) => Err(GetLuaVersionError::RunLuaCommandError(lua_cmd.into(), err)),
    }?;
    let output_vec = if output.stderr.is_empty() {
        output.stdout
    } else {
        // Yes, Lua 5.1 prints to stderr (-‸ლ)
        output.stderr
    };
    let lua_output = String::from_utf8_lossy(&output_vec).to_string();
    parse_lua_version_from_output(&lua_output)
}

fn parse_lua_version_from_output(lua_output: &str) -> Result<PackageVersion, GetLuaVersionError> {
    let lua_version_str = lua_output
        .trim_start_matches("Lua")
        .trim_start_matches("JIT")
        .split_whitespace()
        .next()
        .map(|s| s.to_string())
        .ok_or(GetLuaVersionError::ParseLuaVersionError(
            lua_output.to_string(),
        ))?;
    Ok(PackageVersion::parse(&lua_version_str)?)
}

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn parse_luajit_version() {
        let luajit_output =
            "LuaJIT 2.1.1713773202 -- Copyright (C) 2005-2023 Mike Pall. https://luajit.org/";
        parse_lua_version_from_output(luajit_output).unwrap();
    }

    #[tokio::test]
    async fn parse_lua_51_version() {
        let lua_output = "Lua 5.1.5  Copyright (C) 1994-2012 Lua.org, PUC-Rio";
        parse_lua_version_from_output(lua_output).unwrap();
    }
}
