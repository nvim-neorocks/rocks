use std::path::PathBuf;

use assert_fs::assert::PathAssert;
use assert_fs::prelude::{PathChild as _, PathCopy};
use assert_fs::TempDir;
use predicates::prelude::predicate;
use rocks_lib::progress::{MultiProgress, Progress, ProgressBar};
use rocks_lib::{
    config::{ConfigBuilder, LuaVersion},
    lua_installation::LuaInstallation,
    luarocks_installation::LuaRocksInstallation,
};

#[tokio::test]
async fn luarocks_make() {
    let dir = TempDir::new().unwrap();
    let config = ConfigBuilder::new()
        .tree(Some(dir.path().into()))
        .build()
        .unwrap();
    let luarocks = LuaRocksInstallation::new(&config).unwrap();
    let progress = Progress::Progress(MultiProgress::new());
    let bar = progress.map(|p| p.add(ProgressBar::from("Installing luarocks".to_string())));
    luarocks.ensure_installed(&bar).await.unwrap();
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources/test/sample-project-no-build-spec");
    let rockspec_path = project_root.join("foo-1.0.0-1.rockspec");
    let build_dir = TempDir::new().unwrap();
    build_dir.copy_from(&project_root, &["**"]).unwrap();
    let dest_dir = TempDir::new().unwrap();
    let lua_version = config.lua_version().unwrap_or(&LuaVersion::Lua51);
    let lua = LuaInstallation::new(lua_version, &config);
    luarocks
        .make(&rockspec_path, build_dir.path(), dest_dir.path(), &lua)
        .unwrap();
    let foo_dir = dest_dir
        .child("share")
        .child("lua")
        .child(lua_version.version_compatibility_str())
        .child("foo");
    foo_dir.assert(predicate::path::is_dir());
    let foo_init = foo_dir.child("init.lua");
    foo_init.assert(predicate::path::is_file());
    foo_init.assert(predicate::str::contains("return true"));
}
