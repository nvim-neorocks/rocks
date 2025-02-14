#![cfg(feature = "lua")]

use lux_lib::tree::Tree;
use mlua::{IntoLua, Lua};
use tempdir::TempDir;

#[test]
fn tree_userdata() {
    let temp = TempDir::new("tree-userdata").unwrap();

    let lua = Lua::new();
    let t = Tree::new(temp.into_path(), lux_lib::config::LuaVersion::Lua51).unwrap();
    let tree = t.into_lua(&lua).unwrap();
    lua.globals().set("tree", tree).unwrap();

    lua.load(
        r#"
        print(tree:bin())
    "#,
    )
    .exec()
    .unwrap();
}
