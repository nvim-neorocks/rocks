use eyre::Result;
use rocks_lib::{
    config::{Config, LuaVersion},
    lua_installation::LuaInstallation,
    progress::{MultiProgress, ProgressBar},
};

pub async fn install_lua(config: Config) -> Result<()> {
    let version_stringified = &LuaVersion::from(&config)?;

    let progress = MultiProgress::new();
    progress.add(ProgressBar::from(format!(
        "🌔 Installing Lua {}",
        version_stringified
    )));

    // TODO: Detect when path already exists by checking `Lua::path()` and prompt the user
    // whether they'd like to forcefully reinstall.
    LuaInstallation::new(version_stringified, &config);

    Ok(())
}
