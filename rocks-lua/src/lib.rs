use mlua::prelude::*;
use tokio::runtime::Runtime;

mod build {
    use super::*;
    use mlua::Result;
    use rocks_lib::{
        build::BuildBehaviour,
        config::Config,
        lockfile::{LocalPackage, LockConstraint, PinnedState},
        progress::Progress::NoProgress,
        rockspec::Rockspec,
    };

    pub async fn build(
        _lua: Lua,
        (rockspec, pinned, constraint, behaviour, config): (
            Rockspec,
            PinnedState,
            LockConstraint,
            BuildBehaviour,
            Config,
        ),
    ) -> Result<LocalPackage> {
        rocks_lib::build::build(
            rockspec,
            pinned,
            constraint,
            behaviour,
            &config,
            &NoProgress,
        )
        .await
        .into_lua_err()
    }
}

#[mlua::lua_module]
pub fn librocks(lua: &Lua) -> Result<LuaTable, mlua::Error> {
    let runtime = Runtime::new().into_lua_err()?;

    let rocks = lua.create_table()?;

    rocks.set("build", lua.create_async_function(|lua, (rockspec, pinned, constraint, behaviour, config)| {
        build::build(lua, (rockspec, pinned, constraint, behaviour, config))
    })?)?;

    Ok(rocks)
}
