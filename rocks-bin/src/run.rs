use std::env;

use clap::Args;
use eyre::Result;
use rocks_lib::{
    config::{Config, LuaVersion},
    operations::{self, install_command},
    path::Paths,
    project::Project,
    rockspec::LuaVersionCompatibility,
    tree::Tree,
};
use which::which;

use crate::build::Build;

#[derive(Args)]
pub struct Run {
    /// The command to run.
    command: String,
    /// Arguments to pass to the program.
    args: Option<Vec<String>>,
}

pub async fn run(run: Run, config: Config) -> Result<()> {
    let project = Project::current()?;
    let lua_version = match &project {
        Some(prj) => prj.rocks().lua_version_matches(&config)?,
        None => LuaVersion::from(&config)?,
    };
    let tree = Tree::new(config.tree().clone(), lua_version.clone())?;
    let paths = Paths::from_tree(tree)?;
    unsafe {
        // safe as long as this is single-threaded
        env::set_var("PATH", paths.path_prepended().joined());
    }
    if which(&run.command).is_err() {
        match project {
            Some(_) => super::build::build(Build::default(), config.clone()).await?,
            None => install_command(&run.command, &config).await?,
        }
    };
    operations::Run::new(&run.command, &config)
        .args(run.args.unwrap_or_default())
        .run()
        .await?;
    Ok(())
}
