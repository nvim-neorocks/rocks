use rocks_lib::{
    build,
    config::{ConfigBuilder, LuaVersion},
    rockspec::Rockspec,
};
use tempdir::TempDir;

#[tokio::test]
async fn builtin_build() {
    let dir = TempDir::new("rocks-test").unwrap();

    let content =
        String::from_utf8(std::fs::read("resources/test/lua-cjson-2.1.0-1.rockspec").unwrap())
            .unwrap();
    let rockspec = Rockspec::new(&content).unwrap();

    let config = ConfigBuilder::new()
        .tree(Some(dir.into_path()))
        .lua_version(Some(LuaVersion::Lua51))
        .build()
        .unwrap();

    build::build(rockspec, &config).await.unwrap();
}
