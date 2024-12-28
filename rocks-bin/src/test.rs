use clap::Args;
use eyre::{OptionExt, Result};
use rocks_lib::{
    config::Config,
    manifest::Manifest,
    operations::{ensure_busted, ensure_dependencies, run_tests, TestEnv},
    progress::MultiProgress,
    project::Project,
    tree::Tree,
};

#[derive(Args)]
pub struct Test {
    /// Arguments to pass to the test runner.
    test_args: Option<Vec<String>>,
    /// Don't isolate the user environment (keep `HOME` and `XDG` environment variables).
    #[arg(long)]
    impure: bool,
}

pub async fn test(test: Test, config: Config) -> Result<()> {
    let project = Project::current()?
        .ok_or_eyre("'rocks test' must be run in a project root, with a 'project.rockspec'")?;
    let rockspec = project.rockspec();
    let lua_version = match rockspec.lua_version_from_config(&config) {
        Ok(lua_version) => Ok(lua_version),
        Err(_) => rockspec.test_lua_version().ok_or_eyre("lua version not set! Please provide a version through `--lua-version <ver>` or add it to your rockspec's dependencies"),
    }?;
    let manifest = Manifest::from_config(config.server(), &config).await?;
    let test_config = config.with_lua_version(lua_version);
    let tree = Tree::new(
        test_config.tree().clone(),
        test_config.lua_version().unwrap().clone(),
    )?;
    let progress = MultiProgress::new_arc();
    // TODO(#204): Only ensure busted if running with busted (e.g. a .busted directory exists)
    ensure_busted(&tree, &manifest, &test_config, progress.clone()).await?;
    ensure_dependencies(rockspec, &tree, &manifest, &test_config, progress).await?;
    let test_args = test.test_args.unwrap_or_default();
    let test_env = if test.impure {
        TestEnv::Impure
    } else {
        TestEnv::Pure
    };
    run_tests(project, test_args, test_env, test_config).await?;
    Ok(())
}
