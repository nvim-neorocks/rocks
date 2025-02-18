use clap::Args;
use eyre::{OptionExt, Result};
use lux_lib::{
    config::Config,
    operations::{self, TestEnv},
    project::Project,
};

#[derive(Args)]
pub struct Test {
    /// Arguments to pass to the test runner.
    test_args: Option<Vec<String>>,
    /// Don't isolate the user environment (keep `HOME` and `XDG` environment variables).
    #[arg(long)]
    impure: bool,

    /// Ignore the project's lockfile and don't create one.
    #[arg(long)]
    no_lock: bool,
}

pub async fn test(test: Test, config: Config) -> Result<()> {
    let project = Project::current()?
        .ok_or_eyre("'lux test' must be run in a project root, with a 'project.rockspec'")?;
    let test_args = test.test_args.unwrap_or_default();
    let test_env = if test.impure {
        TestEnv::Impure
    } else {
        TestEnv::Pure
    };
    operations::Test::new(project, &config)
        .args(test_args)
        .env(test_env)
        .no_lock(test.no_lock)
        .run()
        .await?;
    Ok(())
}
