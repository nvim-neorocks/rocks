use std::str::FromStr;

use clap::Args;
use eyre::{eyre, Result};
use itertools::{Either, Itertools};
use rustyline::error::ReadlineError;
use spdx::LicenseId;
use spinners::{Spinner, Spinners};

use crate::rockspec::github_metadata::{self, RepoMetadata};
use rocks_lib::rockspec::LuaDependency;

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
// - Ask user for a homepage
// - Automatically detect build type by inspecting the current repo (is there a Cargo.toml? is
//   there something that tells us it's a lua project?).

#[derive(Args)]
pub struct WriteRockspec {
    /// The name to give to the rock.
    #[arg(short, long)]
    name: Option<String>,

    /// The description of the rock.
    #[arg(short, long)]
    description: Option<String>,

    /// The license of the rock. Generic license names will be inferred.
    #[arg(short, long, value_parser = parse_license_wrapper)]
    license: Option<Either<LicenseId, ()>>,

    /// The maintainer of this rock. Does not have to be the code author.
    #[arg(short, long)]
    maintainer: Option<String>,
}

fn parse_license_wrapper(s: &str) -> std::result::Result<Either<LicenseId, ()>, String> {
    match parse_license(s.to_string()) {
        Ok(val) => Ok(val.map(Either::Left).unwrap_or(Either::Right(()))),
        Err(err) => Err(err.to_string()),
    }
}

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

fn parse_license(input: String) -> Result<Option<LicenseId>> {
    match input.to_lowercase().as_str() {
        "none" => Ok(None),
        _ => Ok(Some(
            spdx::imprecise_license_id(&input)
                .ok_or(eyre!(
                    "Unable to identify license '{}', please try again!",
                    input
                ))?
                .0,
        )),
    }
}

fn parse_version(input: String) -> Result<LuaDependency> {
    LuaDependency::from_str(format!("lua {}", input).as_str())
}

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

    let package_name = data.name.map(Ok).unwrap_or_else(|| {
        parse!(
            editor
                .readline(format!("Package Name (empty for '{}'): ", repo_metadata.name).as_str()),
            identity,
            repo_metadata.name
        )
    })?;

    let description = data.description.map(Ok).unwrap_or_else(|| {
        parse!(
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
        )
    })?;

    let license = data
        .license // First validate the `--license` option
        .and_then(|license| match license {
            Either::Left(license) => Some(license),
            Either::Right(_) => None,
        })
        .map_or_else(
            || {
                // If there was no `--license` then prompt the user
                parse!(
                    editor.readline(
                        format!(
                            "License (empty for '{}', 'none' for no license): ",
                            repo_metadata
                                .license
                                .as_ref()
                                .unwrap_or(&"none".to_string())
                        )
                        .as_str()
                    ),
                    parse_license,
                    parse_license(repo_metadata.license.unwrap())?
                )
            },
            |val| Ok(Some(val)),
        )?
        .map(|license| license.name)
        .unwrap_or("*** enter a license ***");

    let maintainer = data.maintainer.map(Ok).unwrap_or_else(|| {
        parse!(
            editor.readline(
                format!(
                    "Maintainer (empty for '{}'): ",
                    repo_metadata.contributors.first().unwrap_or(&"".into())
                )
                .as_str()
            ),
            identity,
            repo_metadata.contributors.first().unwrap_or(&"".into())
        )
    })?;

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

    let lua_versions = parse!(
        editor.readline("Supported Lua Versions (empty for '>= 5.1'): ",),
        parse_version,
        LuaDependency::from_str("lua >= 5.1")?
    )?;

    std::fs::write(
        format!("{}-dev.rockspec", package_name),
        format!(
            r#"
rockspec_format = "3.0"
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

dependencies = {{
    "lua{version}",
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
            version = lua_versions.rock_version_req.to_string().replace('^', "~>"),
        )
        .trim(),
    )?;

    Ok(())
}

// TODO(vhyrro): Add tests
