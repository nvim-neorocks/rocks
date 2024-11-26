use std::path::PathBuf;

use rocks_lib::{
    config::ConfigBuilder,
    manifest::ManifestMetadata,
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
    let config = ConfigBuilder::new()
        .tree(Some(project.root().to_path_buf().join(".rocks")))
        .build()
        .unwrap();
    let tree = Tree::new(config.tree().clone(), config.lua_version().unwrap().clone()).unwrap();
    let manifest = ManifestMetadata::from_config(&config).await.unwrap();
    ensure_busted(&MultiProgress::new(), &tree, &manifest, &config)
        .await
        .unwrap();
    run_tests(project, Vec::new(), TestEnv::Pure, config)
        .await
        .unwrap()
}
