use std::path::PathBuf;

use lux_lib::{
    config::{Config, ConfigBuilder},
    project::Project,
};
use mlua::prelude::*;

fn config(lua: &Lua) -> mlua::Result<LuaTable> {
    let table = lua.create_table()?;

    table.set(
        "default",
        lua.create_function(|_, ()| ConfigBuilder::default().build().into_lua_err())?,
    )?;

    Ok(table)
}

fn project(lua: &Lua) -> mlua::Result<LuaTable> {
    let table = lua.create_table()?;

    table.set(
        "current",
        lua.create_function(|_, ()| Project::current().into_lua_err())?,
    )?;

    table.set(
        "new",
        lua.create_function(|_, path: PathBuf| Project::from(path).into_lua_err())?,
    )?;

    Ok(table)
}

#[mlua::lua_module]
fn liblux_lua(lua: &Lua) -> LuaResult<LuaTable> {
    let exports = lua.create_table()?;

    exports.set("config", config(lua)?)?;
    exports.set("project", project(lua)?)?;

    Ok(exports)
}
