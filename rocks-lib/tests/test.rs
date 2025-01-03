use std::path::PathBuf;

use rocks_lib::{config::ConfigBuilder, operations::Test, project::Project};

#[tokio::test]
async fn run_busted_test() {
    let project_root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/test/sample-project-busted");
    let project: Project = Project::from(project_root).unwrap().unwrap();
    let tree_root = project.root().to_path_buf().join(".rocks");
    let _ = std::fs::remove_dir_all(&tree_root);
    let config = ConfigBuilder::new().tree(Some(tree_root)).build().unwrap();
    Test::new(project, &config).run().await.unwrap();
}
