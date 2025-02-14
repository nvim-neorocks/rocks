use eyre::Result;
use lux_lib::{
    config::{Config, LuaVersion},
    lua_installation::LuaInstallation,
    progress::{MultiProgress, ProgressBar},
};

pub async fn install_lua(config: Config) -> Result<()> {
    let version_stringified = &LuaVersion::from(&config)?;

    let progress = MultiProgress::new();
    let bar = progress.add(ProgressBar::from(format!(
        "🌔 Installing Lua ({})",
        version_stringified
    )));

    // TODO: Detect when path already exists by checking `Lua::path()` and prompt the user
    // whether they'd like to forcefully reinstall.
    LuaInstallation::new(version_stringified, &config);

    bar.finish_with_message(format!("🌔 Installed Lua ({})", version_stringified));

    Ok(())
}
