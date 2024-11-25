use rocks_lib::{
    config::{ConfigBuilder, LuaVersion},
    operations::{install_command, run},
};
use tempdir::TempDir;

#[tokio::test]
async fn run_nlua() {
    let dir = TempDir::new("rocks-test").unwrap();
    let config = ConfigBuilder::new()
        .tree(Some(dir.into_path()))
        .lua_version(Some(LuaVersion::Lua51))
        .build()
        .unwrap();
    install_command("nlua", &config).await.unwrap();
    run("nlua", vec!["-v".into()], config).await.unwrap()
}