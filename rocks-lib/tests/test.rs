use std::path::PathBuf;

use rocks_lib::{
    config::ConfigBuilder,
    operations::{ensure_busted, run_tests, TestEnv},
    progress::MultiProgress,
    project::Project,
    tree::Tree,
};

#[tokio::test]
async fn run_busted_test() {
    let project_root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/test/sample-project-busted");
    let project: Project = Project::from(project_root).unwrap().unwrap();
    let tree_root = project.root().to_path_buf().join(".rocks");
    let _ = std::fs::remove_dir_all(&tree_root);
    let config = ConfigBuilder::new().tree(Some(tree_root)).build().unwrap();
    let tree = Tree::new(config.tree().clone(), config.lua_version().unwrap().clone()).unwrap();
    ensure_busted(&tree, &config, MultiProgress::new_arc())
        .await
        .unwrap();
    run_tests(project, Vec::new(), TestEnv::Pure, config)
        .await
        .unwrap()
}
