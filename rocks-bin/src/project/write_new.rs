use std::{error::Error, path::PathBuf, str::FromStr};

use clap::Args;
use eyre::{eyre, Result};
use inquire::{
    ui::{RenderConfig, Styled},
    validator::Validation,
    Confirm, Select, Text,
};
use itertools::Itertools;
use spdx::LicenseId;
use spinners::{Spinner, Spinners};

use crate::utils::github_metadata::{self, RepoMetadata};
use rocks_lib::{project::Project, rockspec::LuaDependency};

// General notes and ideas:
// - Create a subdirectory for the project, unless a path is explicitly passed.
// - Automatically detect build type by inspecting the current repo (is there a Cargo.toml? is
//   there something that tells us it's a lua project?).

#[derive(Args)]
pub struct NewProject {
    /// The directory of the project.
    #[arg(long)]
    directory: Option<PathBuf>,

    /// The project's name.
    #[arg(long)]
    name: Option<String>,

    /// The description of the project.
    #[arg(long)]
    description: Option<String>,

    /// The license of the project. Generic license names will be inferred.
    #[arg(long, value_parser = clap_parse_license)]
    license: Option<LicenseId>,

    /// The maintainer of this project. Does not have to be the code author.
    #[arg(long)]
    maintainer: Option<String>,

    /// A comma-separated list of labels to apply to this project.
    #[arg(long, value_parser = clap_parse_list)]
    labels: Option<std::vec::Vec<String>>, // Note: full qualified name required, see https://github.com/clap-rs/clap/issues/4626

    /// A version constraint on the required Lua version for this project.
    /// Examples: ">=5.1", "5.1"
    #[arg(long, value_parser = clap_parse_version)]
    lua_versions: Option<LuaDependency>,
}

fn clap_parse_license(s: &str) -> std::result::Result<LicenseId, String> {
    match validate_license(s) {
        Ok(Validation::Valid) => Ok(parse_license_unchecked(s)),
        Err(_) | Ok(Validation::Invalid(_)) => {
            Err(format!("unable to identify license {s}, please try again!"))
        }
    }
}

fn clap_parse_version(input: &str) -> std::result::Result<LuaDependency, String> {
    LuaDependency::from_str(format!("lua {}", input).as_str()).map_err(|err| err.to_string())
}

fn clap_parse_list(input: &str) -> std::result::Result<Vec<String>, String> {
    if let Some((pos, char)) = input
        .chars()
        .find_position(|&c| c != '-' && c != '_' && c != ',' && c.is_ascii_punctuation())
    {
        Err(format!("Unexpected punctuation '{}' found at column {}. Lists are comma separated but names should not contain punctuation!", char, pos))
    } else {
        Ok(input.split(',').map(|str| str.trim().to_string()).collect())
    }
}

/// Parses a license and panics upon failure.
///
/// # Security
///
/// This should only be invoked after validating the license with [`validate_license`].
fn parse_license_unchecked(input: &str) -> LicenseId {
    spdx::imprecise_license_id(input).unwrap().0
}

fn validate_license(input: &str) -> std::result::Result<Validation, Box<dyn Error + Send + Sync>> {
    if input == "none" {
        return Ok(Validation::Valid);
    }

    Ok(
        match spdx::imprecise_license_id(input).ok_or(format!(
            "Unable to identify license '{}', please try again!",
            input
        )) {
            Ok(_) => Validation::Valid,
            Err(err) => Validation::Invalid(err.into()),
        },
    )
}

pub async fn write_project_rockspec(cli_flags: NewProject) -> Result<()> {
    let project = Project::new(None)?;
    let render_config = RenderConfig::default_colored()
        .with_prompt_prefix(Styled::new(">").with_fg(inquire::ui::Color::LightGreen));

    // If the project already exists then ask for override confirmation
    if project.is_some()
        && !Confirm::new("You are already in a project, write anyway?")
            .with_default(false)
            .with_help_message("This may overwrite your existing project.rockspec")
            .with_render_config(render_config)
            .prompt()?
    {
        return Err(eyre!("cancelled creation of project (already exists)"));
    };

    let (package_name, description, license, labels, maintainer, lua_versions) = match cli_flags {
        // If all parameters are provided then don't bother prompting the user
        NewProject {
            name: Some(name),
            description: Some(description),
            labels: Some(labels),
            license,
            lua_versions: Some(lua_versions),
            maintainer: Some(maintainer),
            ..
        } => Ok::<_, eyre::Report>((name, description, license, labels, maintainer, lua_versions)),

        NewProject {
            name,
            description,
            labels,
            license,
            lua_versions,
            maintainer,
            directory,
        } => {
            let mut spinner = Spinner::new(
                Spinners::Dots,
                "Fetching remote repository metadata... ".into(),
            );

            let repo_metadata = match github_metadata::get_metadata_for(directory.as_ref()).await {
                Ok(value) => value.unwrap_or_default(),
                Err(_) => {
                    println!("Could not fetch remote repo metadata, defaulting to empty values.");

                    RepoMetadata::default()
                }
            };

            spinner.stop_and_persist("✔", "Fetched remote repository metadata.".into());

            let package_name = name.map_or_else(
                || {
                    Text::new("Package name:")
                        .with_default(&repo_metadata.name)
                        .with_help_message("A folder with the same name will be created for you.")
                        .with_render_config(render_config)
                        .prompt()
                },
                Ok,
            )?;

            let description = description.map_or_else(
                || {
                    Text::new("Description:")
                        .with_default(&repo_metadata.description.unwrap_or_default())
                        .with_render_config(render_config)
                        .prompt()
                },
                Ok,
            )?;

            let license = license.map_or_else(
                || {
                    Ok::<_, eyre::Error>(
                        match Text::new("License:")
                            .with_default(&repo_metadata.license.unwrap_or("none".into()))
                            .with_help_message("Type 'none' for no license")
                            .with_validator(validate_license)
                            .with_render_config(render_config)
                            .prompt()?
                            .as_str()
                        {
                            "none" => None,
                            license => Some(parse_license_unchecked(license)),
                        },
                    )
                },
                |license| Ok(Some(license)),
            )?;

            let labels = labels.map_or_else(
                || {
                    Ok::<_, eyre::Error>(
                        Text::new("Labels:")
                            .with_placeholder("web,filesystem")
                            .with_help_message("Labels are comma separated")
                            .prompt()?
                            .split(',')
                            .map(|label| label.trim().to_string())
                            .collect_vec(),
                    )
                },
                Ok,
            )?;

            let maintainer = maintainer.map_or_else(
                || {
                    let default_maintainer = repo_metadata
                        .contributors
                        .first()
                        .cloned()
                        .unwrap_or_else(|| {
                            users::get_current_username()
                                .expect("current user could not be found. Was it deleted?")
                                .to_string_lossy()
                                .to_string()
                        });
                    Text::new("Maintainer:")
                        .with_default(&default_maintainer)
                        .prompt()
                },
                Ok,
            )?;

            let lua_versions = lua_versions.map_or_else(
                || {
                    format!(
                        "lua >= {}",
                        Select::new(
                            "What is the lowest Lua version you support?",
                            vec!["5.1", "5.2", "5.3", "5.4"]
                        )
                        .without_filtering()
                        .with_help_message(
                            "This is equivalent to the 'lua >= {version}' constraint."
                        )
                        .prompt()?
                    )
                    .parse()
                },
                Ok,
            )?;

            Ok((
                package_name,
                description,
                license,
                labels,
                maintainer,
                lua_versions,
            ))
        }
    }?;

    std::fs::write(
        "project.rockspec",
        format!(
            r#"
rockspec_format = "3.0"
package = "{package_name}"

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
            license = license
                .map(|license| license.name)
                .unwrap_or("*** enter a license ***"),
            maintainer = maintainer,
            labels = labels
                .into_iter()
                .map(|label| "\"".to_string() + &label + "\"")
                .join(", "),
            version = lua_versions.rock_version_req.to_string().replace('^', "~>"),
        )
        .trim(),
    )?;

    Ok(())
}

// TODO(vhyrro): Add tests
