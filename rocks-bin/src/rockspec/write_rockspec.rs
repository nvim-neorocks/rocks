use clap::Args;
use eyre::{eyre, Result};
use itertools::Itertools;
use rustyline::error::ReadlineError;
use spinners::{Spinner, Spinners};

use crate::rockspec::github_metadata::{self, RepoMetadata};

macro_rules! parse {
    ($initial:expr, $parser:expr, $alternative:expr) => {
        loop {
            match $initial {
                Ok(value) => {
                    if value.is_empty() {
                        break Ok($alternative.into());
                    }

                    match $parser(value) {
                        Ok(value) => break Ok(value),
                        Err(err) => {
                            println!("Error: {}", err.to_string());
                            continue;
                        }
                    }
                }
                Err(ReadlineError::Interrupted) => {
                    break Err(eyre!("Ctrl-C pressed, exiting..."));
                }
                Err(ReadlineError::Eof) => {
                    break Err(eyre!("Ctrl-D pressed, exiting..."));
                }
                Err(err) => break Err(err.into()),
            }
        }
    };
}

// General notes and ideas:
// - Should we require the user to create a "project" in order to use this command?
// - Should we grab all collaborators by default? That might end up being massive
//   if there's a sizeable project.
// - Ask user for a homepage
// - Automatically detect build type by inspecting the current repo (is there a Cargo.toml? is
//   there something that tells us it's a lua project?).

#[derive(Args)]
pub struct WriteRockspec {}

fn identity(input: String) -> Result<String> {
    Ok(input)
}

fn parse_list(input: String) -> Result<Vec<String>> {
    if let Some((pos, char)) = input
        .chars()
        .find_position(|&c| c != '-' && c != '_' && c.is_ascii_punctuation())
    {
        Err(eyre!("Unexpected punctuation '{}' found at column {}. Lists are space separated and do not consist of punctuation!", char, pos))
    } else {
        Ok(input.split_whitespace().map_into().collect())
    }
}

pub async fn write_rockspec(_data: WriteRockspec) -> Result<()> {
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
            description: None,
            license: None,
            contributors: vec![users::get_current_username()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()],
            labels: None,
        });

    spinner.stop();

    let mut editor = rustyline::Editor::<(), _>::new()?;

    // TODO: Make prompts coloured

    // let mut stdout = BufferedStandardStream::stdout(ColorChoice::Always);
    // stdout.set_color(ColorSpec::new().set_fg(Some(Color::Green)))?;

    // TODO(vhyrro): Make the array inputs less confusing (mention it being space separated)
    let package_name = parse!(
        editor.readline(format!("Package Name (empty for '{}'): ", repo_metadata.name).as_str()),
        identity,
        repo_metadata.name
    )?;

    let description = parse!(
        editor.readline(
            format!(
                "Description (empty for '{}'): ",
                repo_metadata
                    .description
                    .as_ref()
                    .unwrap_or(&"*** enter a description ***".to_string())
            )
            .as_str()
        ),
        identity,
        repo_metadata.description.unwrap_or_default()
    )?;

    let license = parse!(
        editor.readline(
            format!(
                "License (empty for '{}'): ",
                repo_metadata
                    .license
                    .as_ref()
                    .unwrap_or(&"*** enter a license ***".to_string())
            )
            .as_str()
        ),
        identity, // TODO: verify license validity
        repo_metadata
            .license
            .unwrap_or("*** enter a license ***".into())
    )?;

    let maintainer = parse!(
        editor.readline(
            format!(
                "Maintainer (empty for '{}'): ",
                repo_metadata.contributors.first().unwrap_or(&"".into())
            )
            .as_str()
        ),
        identity,
        repo_metadata.contributors.first().unwrap_or(&"".into())
    )?;

    let labels = parse!(
        editor.readline(
            format!(
                "Labels (empty for '[{}]'): ",
                repo_metadata
                    .labels
                    .as_ref()
                    .unwrap_or(&Vec::default())
                    .join(" ")
            )
            .as_str()
        ),
        parse_list,
        repo_metadata.labels.unwrap_or_default()
    )?
    .into_iter()
    .map(|label| "\"".to_string() + &label + "\"")
    .join(", ");

    std::fs::write(
        format!("{}-dev.rockspec", package_name),
        format!(
            r#"
package = "{package_name}"
version = "dev-1"

source = {{
    url = "*** provide a url here ***",
}}

description = {{
    summary = "{summary}",
    maintainer = "{maintainer}",
    license = "{license}",
    labels = {{ {labels} }},
}}

build = {{
    type = "builtin",
}}
    "#,
            package_name = package_name,
            summary = description,
            license = license,
            labels = labels,
            maintainer = maintainer,
        )
        .trim(),
    )?;

    Ok(())
}
