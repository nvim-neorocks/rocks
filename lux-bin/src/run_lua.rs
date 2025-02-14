use std::process::Command;

use clap::Args;
use eyre::{eyre, Result};
use itertools::Itertools;
use lux_lib::{
    config::{Config, LuaVersion},
    lua_installation::get_installed_lua_version,
    path::Paths,
    project::Project,
    rockspec::LuaVersionCompatibility,
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
        Some(prj) => prj.toml().lua_version_matches(&config)?,
        None => LuaVersion::from(&config)?,
    };
    match get_installed_lua_version(&lua_cmd).and_then(|ver| Ok(LuaVersion::from_version(ver)?)) {
        Ok(installed_version) => {
            if installed_version != lua_version {
                return Err(eyre!(
                    "{} -v (= {}) does not match expected Lua version {}",
                    &lua_cmd,
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
    let tree = config.tree(lua_version)?;

    let paths = if let Some(project) = project {
        let mut paths = Paths::new(tree)?;

        paths.prepend(&Paths::new(project.tree(&config)?)?);

        paths
    } else {
        Paths::new(tree)?
    };

    let status = match Command::new(&lua_cmd)
        .args(run_lua.args.unwrap_or_default())
        .env("PATH", paths.path_prepended().joined())
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
Usage: lux lua -- [LUA_OPTIONS] [SCRIPT [ARGS]]...

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

#[cfg(test)]
mod test {
    use std::path::PathBuf;

    use lux_lib::config::ConfigBuilder;

    use super::*;

    #[tokio::test]
    async fn test_run_lua() {
        let args = RunLua {
            args: Some(vec!["-v".into()]),
            ..RunLua::default()
        };
        let temp: PathBuf = assert_fs::TempDir::new().unwrap().path().into();
        let config = ConfigBuilder::new()
            .unwrap()
            .tree(Some(temp.clone()))
            .luarocks_tree(Some(temp))
            .build()
            .unwrap();
        run_lua(args, config).await.unwrap()
    }
}
