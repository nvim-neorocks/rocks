use std::process::Command;

use clap::Args;
use eyre::{eyre, OptionExt as _, Result};
use itertools::Itertools;
use rocks_lib::{
    config::{Config, LuaVersion},
    package::PackageVersion,
    path::Paths,
    project::Project,
    tree::Tree,
};

#[derive(Args, Default)]
#[clap(disable_help_flag = true)]
pub struct RunLua {
    /// Arguments to pass to Lua. See `lua -h`.
    args: Option<Vec<String>>,

    /// Path to the Lua interpreter to use
    #[arg(long)]
    lua: Option<String>,

    /// Print help
    #[arg(long)]
    help: bool,
}

pub async fn run_lua(run_lua: RunLua, config: Config) -> Result<()> {
    let lua_cmd = run_lua.lua.unwrap_or("lua".into());
    if run_lua.help {
        return print_lua_help(&lua_cmd);
    }
    let project = Project::current()?;
    let lua_version = match &project {
        Some(prj) => prj.rockspec().lua_version_from_config(&config)?,
        None => LuaVersion::from(&config)?,
    };
    match get_installed_lua_version(&lua_cmd) {
        Ok(installed_version) => {
            if !lua_version.as_version_req().matches(&installed_version) {
                return Err(eyre!(
                    "lua -v (= {}) does not match expected Lua version {}",
                    installed_version,
                    &lua_version,
                ));
            }
        }
        Err(_) => {
            eprintln!(
                "⚠️ WARNING: Could not parse Lua version from '{} -v' output. Assuming Lua {} compatibility.",
                &lua_cmd, lua_version
            );
        }
    }
    let tree = Tree::new(config.tree().clone(), lua_version.clone())?;
    let paths = Paths::from_tree(tree)?;
    let status = match Command::new(&lua_cmd)
        .args(run_lua.args.unwrap_or_default())
        .env("PATH", paths.path_appended().joined())
        .env("LUA_PATH", paths.package_path().joined())
        .env("LUA_CPATH", paths.package_cpath().joined())
        .status()
    {
        Ok(status) => Ok(status),
        Err(err) => Err(eyre!("Failed to run {}: {}", &lua_cmd, err)),
    }?;
    if status.success() {
        Ok(())
    } else {
        match status.code() {
            Some(code) => Err(eyre!(
                "{} exited with non-zero exit code: {}",
                &lua_cmd,
                code
            )),
            None => Err(eyre!("{} failed with unknown exit code.", &lua_cmd)),
        }
    }
}

fn print_lua_help(lua_cmd: &str) -> Result<()> {
    let output = match Command::new(lua_cmd)
        // HACK: This fails with exit 1, because lua doesn't actually have a help flag (╯°□°)╯︵ ┻━┻
        .arg("-h")
        .output()
    {
        Ok(output) => Ok(output),
        Err(err) => Err(eyre!("Failed to run {}: {}", lua_cmd, err)),
    }?;
    let lua_help = String::from_utf8_lossy(&output.stderr)
        .lines()
        .skip(2)
        .map(|line| format!("  {}", line))
        .collect_vec()
        .join("\n");
    print!(
        "
Usage: rocks lua -- [LUA_OPTIONS] [SCRIPT [ARGS]]...

Arguments:
  [LUA_OPTIONS]...
{}

Options:
  -h, --help  Print help
",
        lua_help,
    );
    Ok(())
}

fn get_installed_lua_version(lua_cmd: &str) -> Result<PackageVersion> {
    let output = match Command::new(lua_cmd).arg("-v").output() {
        Ok(output) => Ok(output),
        Err(err) => Err(eyre!("Failed to run {}: {}", lua_cmd, err)),
    }?;
    // Example: Lua 5.1.5  Copyright (C) 1994-2012 Lua.org, PUC-Rio
    let lua_output = String::from_utf8_lossy(&output.stderr); // Yes, Lua prints to stderr (-‸ლ)
    let lua_version_str = lua_output
        .trim_start_matches("Lua")
        .split_whitespace()
        .next()
        .map(|s| s.to_string())
        .ok_or_eyre(format!(
            "Could not extract Lua version from output: {}",
            lua_output
        ))?;
    Ok(PackageVersion::parse(&lua_version_str)?)
}

#[cfg(test)]
mod test {
    use rocks_lib::config::ConfigBuilder;

    use super::*;

    #[tokio::test]
    async fn test_run_lua() {
        let args = RunLua {
            args: Some(vec!["-v".into()]),
            ..RunLua::default()
        };
        let config = ConfigBuilder::new()
            .lua_version(Some(LuaVersion::Lua51))
            .build()
            .unwrap();
        run_lua(args, config).await.unwrap()
    }
}
