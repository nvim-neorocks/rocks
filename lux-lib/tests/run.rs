use lux_lib::{
    config::ConfigBuilder,
    operations::{install_command, Run},
};
use tempdir::TempDir;

#[tokio::test]
async fn run_nlua() {
    let dir = TempDir::new("lux-test").unwrap();
    let config = ConfigBuilder::new()
        .unwrap()
        .tree(Some(dir.into_path()))
        .build()
        .unwrap();
    install_command("nlua", &config).await.unwrap();
    Run::new("nlua", None, &config)
        .arg("-v")
        .run()
        .await
        .unwrap();
}
