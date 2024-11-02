use clap::Args;
use eyre::{bail, OptionExt, Result};
use rocks_lib::{config::Config, project::Project};

#[derive(Args)]
pub struct Add {}

pub fn add(data: Add, config: Config) -> Result<()> {
    let mut project = Project::current()?.ok_or_eyre("Unable to add dependency - current directory does not belong to a Lua project. Run `rocks new` to create one.")?;
    project.rockspec_mut().dependencies.push("what the dog doing".to_string());

    Ok(())
}
