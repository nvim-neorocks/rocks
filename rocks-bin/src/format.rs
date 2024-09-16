use std::path::PathBuf;

use eyre::{OptionExt, Result};
use rocks_lib::project::Project;
use stylua_lib::Config;
use walkdir::WalkDir;

// TODO: Add `PathBuf` parameter that describes what directory or file to format here.
pub fn format() -> Result<()> {
    let project = Project::current()?.ok_or_eyre(
        "`rocks fmt` can only be executed in a rocks project! Run `rocks new` to create one.",
    )?;

    let config: Config = std::fs::read_to_string("stylua.toml")
        .or_else(|_| std::fs::read_to_string(".stylua.toml"))
        .map(|config: String| toml::from_str(&config).unwrap_or_default())
        .unwrap_or_default();

    WalkDir::new(project.root().join("src"))
        .into_iter()
        .chain(WalkDir::new(project.root().join("lua")))
        .chain(WalkDir::new(project.root().join("lib")))
        .try_for_each(|file| {
            if let Ok(file) = file {
                if PathBuf::from(file.file_name())
                    .extension()
                    .is_some_and(|ext| ext == "lua")
                {
                    let formatted_code = stylua_lib::format_code(
                        &std::fs::read_to_string(file.path())?,
                        config,
                        None,
                        stylua_lib::OutputVerification::Full,
                    )?;

                    std::fs::write(file.into_path(), formatted_code)?;
                };
            }
            Ok::<_, eyre::Report>(())
        })?;

    Ok(())
}
