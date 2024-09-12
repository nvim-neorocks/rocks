use clap::Args;
use eyre::Result;
use rocks_lib::{
    config::{Config, LuaVersion},
    lua::Lua,
};

#[derive(Args)]
pub struct InstallLua {
    version: LuaVersion,
}

pub fn install_lua(install_data: InstallLua, config: &Config) -> Result<()> {
    let version_stringified = install_data.version.to_string();

    // TODO: Detect when path already exists by checking `Lua::path()` and prompt the user
    // whether they'd like to forcefully reinstall.
    Lua::new(&install_data.version, config)?;

    print!("Succesfully installed Lua {version_stringified}.");

    Ok(())
}
