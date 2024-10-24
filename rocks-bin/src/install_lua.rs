use std::convert::Infallible;

use eyre::Result;
use indicatif::MultiProgress;
use rocks_lib::{
    config::{Config, LuaVersion},
    lua_installation::LuaInstallation,
    progress::with_spinner,
};

pub async fn install_lua(config: Config) -> Result<()> {
    let version_stringified = &LuaVersion::from(&config)?;

    with_spinner(
        &MultiProgress::new(),
        format!("🌔 Installing Lua {}", version_stringified),
        || async {
            // TODO: Detect when path already exists by checking `Lua::path()` and prompt the user
            // whether they'd like to forcefully reinstall.
            LuaInstallation::new(version_stringified, &config);
            Ok::<_, Infallible>(())
        },
    )
    .await?;

    Ok(())
}
