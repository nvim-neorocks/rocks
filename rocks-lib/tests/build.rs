use rocks_lib::{
    build::{self, BuildBehaviour::Force},
    config::ConfigBuilder,
    lockfile::{LockConstraint::Unconstrained, PinnedState::Unpinned},
    progress::MultiProgress,
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

    build::build(&bar, rockspec, Unpinned, Unconstrained, Force, &config)
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

    build::build(&bar, rockspec, Unpinned, Unconstrained, Force, &config)
        .await
        .unwrap();
}
