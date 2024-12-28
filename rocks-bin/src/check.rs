use eyre::{OptionExt, Result};
use rocks_lib::{
    build::BuildBehaviour,
    config::{Config, LuaVersion},
    lockfile::PinnedState::Pinned,
    operations::{self, install},
    progress::MultiProgress,
    project::Project,
    remote_package_db::RemotePackageDB,
};

pub async fn check(config: Config) -> Result<()> {
    let project = Project::current()?.ok_or_eyre("Not in a project!")?;

    let db = RemotePackageDB::from_config(&config).await?;

    install(
        vec![(BuildBehaviour::NoForce, "luacheck".parse()?)],
        Pinned,
        &db,
        &config,
        MultiProgress::new_arc(),
    )
    .await?;

    operations::run(
        "luacheck",
        vec![
            project.root().to_string_lossy().into(),
            "--exclude-files".into(),
            project
                .tree(LuaVersion::from(&config)?)?
                .root()
                .to_string_lossy()
                .to_string(),
        ],
        config,
    )
    .await?;

    Ok(())
}
