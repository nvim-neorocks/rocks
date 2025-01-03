use std::path::PathBuf;

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
async fn cmake_build() {
    test_build_rockspec("resources/test/luv-1.48.0-2.rockspec".into()).await
}

#[tokio::test]
async fn command_build() {
    // The rockspec appears to be broken when using luajit headers on macos
    let config = ConfigBuilder::new().build().unwrap();
    if cfg!(target_os = "macos") && config.lua_version() == Some(&LuaVersion::LuaJIT) {
        println!("luaposix is broken on macos/luajit! Skipping...");
        return;
    }
    test_build_rockspec("resources/test/luaposix-35.1-1.rockspec".into()).await
}

async fn test_build_rockspec(rockspec_path: PathBuf) {
    let dir = TempDir::new("rocks-test").unwrap();

    let content = String::from_utf8(std::fs::read(rockspec_path).unwrap()).unwrap();
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
