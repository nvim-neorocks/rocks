use eyre::{OptionExt, Result};
use rocks_lib::{
    build::BuildBehaviour,
    config::{Config, LuaVersion},
    lockfile::PinnedState::Pinned,
    operations::{Install, Run},
    progress::MultiProgress,
    project::Project,
};

pub async fn check(config: Config) -> Result<()> {
    let project = Project::current()?.ok_or_eyre("Not in a project!")?;

    Install::new(&config)
        .package(BuildBehaviour::NoForce, "luacheck".parse()?)
        .pin(Pinned)
        .progress(MultiProgress::new_arc())
        .install()
        .await?;

    Run::new("luacheck", &config)
        .arg(&project.root().to_string_lossy())
        .arg("--exclude-files")
        .arg(
            &project
                .tree(LuaVersion::from(&config)?)?
                .root()
                .to_string_lossy(),
        )
        .run()
        .await?;

    Ok(())
}
