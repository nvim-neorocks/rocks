use std::path::PathBuf;

use assert_fs::prelude::PathCopy;
use itertools::Itertools;
use rocks_lib::{
    build::{
        Build,
        BuildBehaviour::{self, Force},
    },
    config::{ConfigBuilder, LuaVersion},
    lockfile::PinnedState,
    operations::{Install, LockfileUpdate},
    package::PackageName,
    progress::{MultiProgress, Progress},
    project::Project,
    remote_package_db::RemotePackageDB,
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

    Build::new(&rockspec, &config, &Progress::Progress(bar))
        .behaviour(Force)
        .build()
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

    Build::new(&rockspec, &config, &Progress::Progress(bar))
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
    let config = ConfigBuilder::new().build().unwrap();
    if cfg!(target_os = "macos") && config.lua_version() == Some(&LuaVersion::LuaJIT) {
        println!("luaposix is broken on macos/luajit! Skipping...");
        return;
    }
    test_build_rockspec("resources/test/luaposix-35.1-1.rockspec".into()).await
}

#[tokio::test]
async fn lockfile_update() {
    let config = ConfigBuilder::new()
        .tree(Some(assert_fs::TempDir::new().unwrap().path().into()))
        .build()
        .unwrap();
    let sample_project: PathBuf = "resources/test/sample-project-lockfile-missing-deps".into();
    let project_root = assert_fs::TempDir::new().unwrap();
    project_root.copy_from(&sample_project, &["**"]).unwrap();
    let project_root: PathBuf = project_root.path().into();
    let project = Project::from(project_root).unwrap().unwrap();
    let dependencies = project
        .rockspec()
        .dependencies
        .current_platform()
        .iter()
        .filter(|package| !package.name().eq(&PackageName::new("lua".into())))
        .cloned()
        .collect_vec();
    let mut lockfile = project.lockfile().unwrap().unwrap();
    LockfileUpdate::new(&mut lockfile, &config)
        .packages(dependencies.clone())
        .add_missing_packages()
        .await
        .unwrap();
    let package_db: RemotePackageDB = lockfile.into();
    Install::new(&config)
        .packages(
            dependencies
                .iter()
                .map(|dep| (BuildBehaviour::NoForce, dep.to_owned())),
        )
        .pin(PinnedState::Unpinned)
        .package_db(package_db)
        .install()
        .await
        .unwrap();
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

    Build::new(&rockspec, &config, &Progress::Progress(bar))
        .behaviour(Force)
        .build()
        .await
        .unwrap();
}
