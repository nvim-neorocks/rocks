use std::path::PathBuf;

use lux_lib::{
    build::{Build, BuildBehaviour::Force},
    config::{ConfigBuilder, LuaVersion},
    lua_rockspec::RemoteLuaRockspec,
    progress::{MultiProgress, Progress},
};
use tempdir::TempDir;

#[tokio::test]
async fn builtin_build() {
    let dir = TempDir::new("lux-test").unwrap();

    let content =
        String::from_utf8(std::fs::read("resources/test/lua-cjson-2.1.0-1.rockspec").unwrap())
            .unwrap();
    let rockspec = RemoteLuaRockspec::new(&content).unwrap();

    let config = ConfigBuilder::new()
        .unwrap()
        .tree(Some(dir.into_path()))
        .build()
        .unwrap();

    let progress = MultiProgress::new();
    let bar = progress.new_bar();

    let tree = config.tree(LuaVersion::from(&config).unwrap()).unwrap();

    Build::new(&rockspec, &tree, &config, &Progress::Progress(bar))
        .behaviour(Force)
        .build()
        .await
        .unwrap();
}

#[tokio::test]
async fn make_build() {
    let dir = TempDir::new("lux-test").unwrap();

    let content = String::from_utf8(
        std::fs::read("resources/test/make-project/make-project-scm-1.rockspec").unwrap(),
    )
    .unwrap();
    let rockspec = RemoteLuaRockspec::new(&content).unwrap();

    let config = ConfigBuilder::new()
        .unwrap()
        .tree(Some(dir.into_path()))
        .build()
        .unwrap();

    let progress = MultiProgress::new();
    let bar = progress.new_bar();

    let tree = config.tree(LuaVersion::from(&config).unwrap()).unwrap();

    Build::new(&rockspec, &tree, &config, &Progress::Progress(bar))
        .behaviour(Force)
        .build()
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
    let config = ConfigBuilder::new().unwrap().build().unwrap();
    if cfg!(target_os = "macos") && config.lua_version() == Some(&LuaVersion::LuaJIT) {
        println!("luaposix is broken on macos/luajit! Skipping...");
        return;
    }
    test_build_rockspec("resources/test/luaposix-35.1-1.rockspec".into()).await
}

async fn test_build_rockspec(rockspec_path: PathBuf) {
    let dir = TempDir::new("lux-test").unwrap();

    let content = String::from_utf8(std::fs::read(rockspec_path).unwrap()).unwrap();
    let rockspec = RemoteLuaRockspec::new(&content).unwrap();

    let config = ConfigBuilder::new()
        .unwrap()
        .tree(Some(dir.into_path()))
        .build()
        .unwrap();

    let progress = MultiProgress::new();
    let bar = progress.new_bar();

    let tree = config.tree(LuaVersion::from(&config).unwrap()).unwrap();

    Build::new(&rockspec, &tree, &config, &Progress::Progress(bar))
        .behaviour(Force)
        .build()
        .await
        .unwrap();
}

#[tokio::test]
async fn treesitter_parser_build() {
    let dir = TempDir::new("lux-test").unwrap();

    let content = String::from_utf8(
        std::fs::read("resources/test/tree-sitter-rust-0.0.43.rockspec").unwrap(),
    )
    .unwrap();
    let rockspec = RemoteLuaRockspec::new(&content).unwrap();

    let config = ConfigBuilder::new()
        .unwrap()
        .tree(Some(dir.into_path()))
        .build()
        .unwrap();

    let progress = MultiProgress::new();
    let bar = progress.new_bar();

    let tree = config.tree(LuaVersion::from(&config).unwrap()).unwrap();

    Build::new(&rockspec, &tree, &config, &Progress::Progress(bar))
        .behaviour(Force)
        .build()
        .await
        .unwrap();
}
