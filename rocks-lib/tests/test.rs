use std::path::PathBuf;

use indicatif::MultiProgress;
use rocks_lib::{
    config::{ConfigBuilder, LuaVersion},
    operations::{ensure_busted, run_tests, TestEnv},
    project::Project,
    tree::Tree,
};

#[tokio::test]
async fn run_busted_test() {
    let project_root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/test/sample-project-busted");
    let project: Project = Project::from(project_root).unwrap().unwrap();
    let config = ConfigBuilder::new()
        .tree(Some(project.root().to_path_buf().join(".rocks")))
        .lua_version(Some(LuaVersion::Lua51))
        .build()
        .unwrap();
    let tree = Tree::new(config.tree().clone(), config.lua_version().unwrap().clone()).unwrap();
    ensure_busted(&MultiProgress::new(), &tree, &config)
        .await
        .unwrap();
    run_tests(project, Vec::new(), TestEnv::Pure, config)
        .await
        .unwrap()
}
