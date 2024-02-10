use std::collections::HashMap;

use clap::Args;
use eyre::Result;
use spinners::{Spinner, Spinners};

use crate::rockspec::github_metadata::{self, RepoMetadata};

// General notes and ideas:
// - Should we require the user to create a "project" in order to use this command?
// - Should we grab all collaborators by default? That might end up being massive
//   if there's a sizeable project.

#[derive(Args)]
pub struct WriteRockspec {}

pub async fn write_rockspec(data: WriteRockspec) -> Result<()> {
    let mut spinner = Spinner::new(Spinners::Dots, "Fetching repository metadata...".into());

    let repo_metadata = github_metadata::get_metadata_for(None)
        .await?
        .unwrap_or_else(|| RepoMetadata {
            name: std::env::current_dir()
                .expect("unable to get current working directory")
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
            license: None,
            contributors: vec![users::get_current_username()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()],
            labels: None,
        });

    spinner.stop();

    let mut editor = rustyline::Editor::<(), _>::new()?;

    editor.readline(format!("name (empty for '{}'):", repo_metadata.name).as_str())?;

    Ok(())
}
