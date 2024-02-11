use clap::Args;
use eyre::{eyre, Result};
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

                    if let Result::<_, eyre::Error>::Err(err) = $parser(&value) {
                        println!("Error: {}", err.to_string());
                        continue;
                    }

                    break Ok(value);
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

#[derive(Args)]
pub struct WriteRockspec {}

fn verify_list(input: &String) -> Result<()> {
    Ok(())
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
        |_| { Ok(()) },
        repo_metadata.name
    )?;

    let description = parse!(
        editor.readline(
            format!(
                "Description (empty for '{}'): ",
                repo_metadata
                    .description
                    .as_ref()
                    .unwrap_or(&"".to_string())
            )
            .as_str()
        ),
        |_| { Ok(()) },
        repo_metadata.description.unwrap_or_default()
    )?;

    let authors = parse!(
        editor.readline(
            format!(
                "Authors (empty for '[{}]'): ",
                repo_metadata.contributors.join(", ")
            )
            .as_str()
        ),
        verify_list,
        repo_metadata.contributors.join(" ")
    )?;

    let labels = parse!(
        editor.readline(
            format!(
                "Labels (empty for '[{}]'): ",
                repo_metadata
                    .labels
                    .as_ref()
                    .unwrap_or(&Vec::default())
                    .join(", ")
            )
            .as_str()
        ),
        verify_list,
        repo_metadata.labels.unwrap_or_default().join(" ")
    )?;

    println!("{}, {}, {}, {}", package_name, description, authors, labels);

    Ok(())
}
