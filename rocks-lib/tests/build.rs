use rocks_lib::{
    build::{self, BuildBehaviour::Force},
    config::{ConfigBuilder, LuaVersion},
    lockfile::{LockConstraint::Unconstrained, PinnedState::Unpinned},
    progress::{MultiProgress, Progress},
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
        .build()
        .unwrap();

    let progress = MultiProgress::new();
    let bar = progress.new_bar();

    build::build(
        rockspec,
        Unpinned,
        Unconstrained,
        Force,
        &config,
        &Progress::Progress(bar),
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn make_build() {
    let dir = TempDir::new("rocks-test").unwrap();

    let content = String::from_utf8(
        std::fs::read("resources/test/make-project/make-project-scm-1.rockspec").unwrap(),
    )
    .unwrap();
    let rockspec = Rockspec::new(&content).unwrap();

    let config = ConfigBuilder::new()
        .tree(Some(dir.into_path()))
        .build()
        .unwrap();

    let progress = MultiProgress::new();
    let bar = progress.new_bar();

    build::build(
        rockspec,
        Unpinned,
        Unconstrained,
        Force,
        &config,
        &Progress::Progress(bar),
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn command_build() {
    let dir = TempDir::new("rocks-test").unwrap();

    let content =
        String::from_utf8(std::fs::read("resources/test/luaposix-35.1-1.rockspec").unwrap())
            .unwrap();
    let rockspec = Rockspec::new(&content).unwrap();

    let config = ConfigBuilder::new()
        .tree(Some(dir.into_path()))
        .build()
        .unwrap();

    // The rockspec appears to be broken when using luajit headers on macos
    if cfg!(target_os = "macos") && config.lua_version() == Some(&LuaVersion::LuaJIT) {
        println!("luaposix is broken on macos/luajit! Skipping...");
        return;
    }

    let progress = MultiProgress::new();
    let bar = progress.new_bar();

    build::build(
        rockspec,
        Unpinned,
        Unconstrained,
        Force,
        &config,
        &Progress::Progress(bar),
    )
    .await
    .unwrap();
}
