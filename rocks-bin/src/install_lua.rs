use eyre::{OptionExt, Result};
use indicatif::MultiProgress;
use rocks_lib::{config::Config, lua_installation::LuaInstallation, progress::with_spinner};

pub async fn install_lua(config: Config) -> Result<()> {
    // TODO: Make a single, monolithic getter for the lua version that returns a Result<>
    let version_stringified = config
        .lua_version()
        .ok_or_eyre("lua version not set! Please provide it via `--lua-version`.")?;

    with_spinner(
        &MultiProgress::new(),
        format!("🌔 Installing Lua {}", version_stringified),
        || async {
            // TODO: Detect when path already exists by checking `Lua::path()` and prompt the user
            // whether they'd like to forcefully reinstall.
            LuaInstallation::new(version_stringified, &config)?;
            Ok(())
        },
    )
    .await?;

    Ok(())
}
