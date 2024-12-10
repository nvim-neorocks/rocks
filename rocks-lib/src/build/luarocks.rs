use crate::{lua_installation::LuaInstallation, rockspec::Rockspec, tree::RockLayout};

use mlua::Lua;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum LuarocksBuildError {
    #[error(transparent)]
    MluaError(#[from] mlua::Error),
}

pub(crate) async fn build(
    rockspec: &Rockspec,
    output_paths: &RockLayout,
    lua: &LuaInstallation,
) -> Result<(), LuarocksBuildError> {
    // TODO: store rockspec content in a temp dir

    // TODO: install build_dependencies?
    // - What about rockspec_format 1 that has build dependencies in the dependencies list?
    //   `--deps-mode none` won't work with those!
    // - Check the rockspec_format and decide what to do? We could install dependencies into
    //   a separate temp tree if rockspec_format != 3

    // TODO: set lua headers in luarocks config

    // Build using Lua, not the CLI wrapper, because the Windows one doesn't seem to work.
    let lua = Lua::new();
    let package_path = "";
    let package_cpath = "";
    let install_tree = ""; // TODO: Temp directory for install tree
    lua.load(format!(
        "
package.path = '{0}'
package.cpath = '{1}'
local commands = {{
    make = 'luarocks.cmd.make',
}}
local args = {{
    'make',
    '--deps-mode',
    'none',
    '--tree',
    '{2}',
}}
local unpack = unpack or table.unpack
cmd.run_command(description, commands, 'luarocks.cmd.external', unpack(args))
",
        package_path, package_cpath, install_tree
    ))
    .exec()?;

    // TODO: Copy files from temp install tree to output_paths
    todo!()
}
