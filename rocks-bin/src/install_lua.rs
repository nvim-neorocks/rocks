use eyre::{OptionExt, Result};
use rocks_lib::{config::Config, lua_installation::LuaInstallation};

pub fn install_lua(config: &Config) -> Result<()> {
    // TODO: Make a single, monolithic getter for the lua version that returns a Result<>
    let version_stringified = config
        .lua_version
        .as_ref()
        .ok_or_eyre("lua version not set! Please provide it via `--lua-version`.")?;

    // TODO: Detect when path already exists by checking `Lua::path()` and prompt the user
    // whether they'd like to forcefully reinstall.
    LuaInstallation::new(version_stringified, config)?;

    print!("Succesfully installed Lua {version_stringified}.");

    Ok(())
}
