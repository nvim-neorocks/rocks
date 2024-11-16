use eyre::eyre;
use indicatif::{MultiProgress, ProgressBar};
use inquire::Confirm;
use itertools::Itertools;
use std::path::PathBuf;

use clap::Args;
use eyre::Result;
use rocks_lib::{
    build::BuildBehaviour,
    config::Config,
    lockfile::{LockConstraint::Unconstrained, PinnedState},
    package::{PackageName, PackageReq},
    rockspec::Rockspec,
    tree::Tree,
};

#[derive(Args)]
pub struct Build {
    rockspec_path: Option<PathBuf>,

    #[arg(long)]
    pin: bool,

    #[arg(long)]
    force: bool,
}

pub async fn build(data: Build, config: Config) -> Result<()> {
    let pin = PinnedState::from(data.pin);

    let rockspec_path = data.rockspec_path.map_or_else(|| {
        // Try to infer the rockspec the user meant.

        let cwd = std::env::current_dir()?;

        let rockspec_paths = walkdir::WalkDir::new(cwd)
            .max_depth(1)
            .same_file_system(true)
            .into_iter()
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry.file_type().is_file()
                    && entry.path().extension().map(|ext| ext.to_str()) == Some(Some("rockspec"))
            })
            .collect_vec();

        let rockspec_count = rockspec_paths.len();

        match rockspec_count {
            0 => Err(eyre!("No rockspec files found in the current directory!")),
            1 => Ok(rockspec_paths.first().unwrap().clone().into_path()),
            _ => Err(eyre!("Could not infer the rockspec to use! There are {} rockspecs in the current directory, please provide a path to the one you'd like to use.", rockspec_count)),
        }
    }, Ok)?;

    if rockspec_path
        .extension()
        .map(|ext| ext != "rockspec")
        .unwrap_or(true)
    {
        return Err(eyre!("Provided path is not a valid rockspec!"));
    }

    let rockspec = std::fs::read_to_string(rockspec_path)?;
    let rockspec = Rockspec::new(&rockspec)?;

    let progress = MultiProgress::new();

    let lua_version = rockspec.lua_version_from_config(&config)?;

    let tree = Tree::new(config.tree().clone(), lua_version)?;

    let build_behaviour = match tree.has_rock_and(
        &PackageReq::new(
            rockspec.package.to_string(),
            Some(rockspec.version.to_string()),
        )?,
        |rock| pin == rock.pinned(),
    ) {
        Some(_) if !data.force => {
            if Confirm::new(&format!(
                "Package {} already exists. Overwrite?",
                rockspec.package,
            ))
            .with_default(false)
            .prompt()?
            {
                BuildBehaviour::Force
            } else {
                return Ok(());
            }
        }
        _ => BuildBehaviour::from(data.force),
    };

    // Ensure all dependencies are installed first
    let dependencies = rockspec
        .dependencies
        .current_platform()
        .iter()
        .filter(|package| !package.name().eq(&PackageName::new("lua".into())))
        .collect_vec();
    let bar = progress
        .add(ProgressBar::new(dependencies.len() as u64))
        .with_message("Installing dependencies...");
    for (index, dependency_req) in dependencies
        .into_iter()
        .filter(|req| tree.has_rock(req).is_none())
        .enumerate()
    {
        rocks_lib::operations::install(
            &progress,
            vec![(build_behaviour, dependency_req.clone())],
            pin,
            &config,
        )
        .await?;
        bar.set_position(index as u64);
    }

    rocks_lib::build::build(
        &MultiProgress::new(),
        rockspec,
        pin,
        Unconstrained,
        build_behaviour,
        &config,
    )
    .await?;

    Ok(())
}
