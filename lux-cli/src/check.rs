use eyre::{OptionExt, Result};
use lux_lib::{
    build::BuildBehaviour,
    config::Config,
    lockfile::PinnedState::Pinned,
    operations::{Install, Run},
    progress::MultiProgress,
    project::Project,
};

pub async fn check(config: Config) -> Result<()> {
    let project = Project::current()?.ok_or_eyre("Not in a project!")?;

    Install::new(&project.tree(&config)?, &config)
        .package(BuildBehaviour::NoForce, "luacheck".parse()?)
        .pin(Pinned)
        .progress(MultiProgress::new_arc())
        .install()
        .await?;

    Run::new("luacheck", Some(&project), &config)
        .arg(project.root().to_string_lossy())
        .arg("--exclude-files")
        .arg(project.tree(&config)?.root().to_string_lossy())
        .run()
        .await?;

    Ok(())
}
