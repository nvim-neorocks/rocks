use std::path::PathBuf;

use clap::Args;
use eyre::{eyre, Context, OptionExt, Result};
use lux_lib::{
    config::{Config, LuaVersion},
    lockfile::ProjectLockfile,
    operations,
    package::PackageReq,
    project::{Project, PROJECT_TOML},
    rockspec::Rockspec,
};

#[derive(Args)]
pub struct Sync {
    /// The path to the lockfile to synchronise from.
    lockfile: PathBuf,

    /// Path to a lux.toml.
    /// If set, 'lux sync' will also synchronise the dependencies in the rocks.toml
    /// with the lockfile.
    /// This is useful if dependencies have been added or removed manually
    /// and the lockfile is out of sync.
    ///
    /// If not set, lux will check the lockfile's parent directory for a
    /// lux.toml file and use that.
    manifest_path: Option<PathBuf>,

    /// Skip the integrity checks for installed rocks.
    #[arg(long)]
    no_integrity_check: bool,
}

pub async fn sync(args: Sync, config: Config) -> Result<()> {
    let tree = config.tree(LuaVersion::from(&config)?)?;

    let mut lockfile = ProjectLockfile::new(args.lockfile.clone())?;

    let mut sync = operations::Sync::new(&tree, &mut lockfile, &config)
        .validate_integrity(!args.no_integrity_check);

    let manifest_path = match args.manifest_path {
        Some(path) => {
            if !path.is_file() {
                return Err(eyre!("File not found: {}", path.display()));
            }
            Some(path)
        }
        None => {
            let toml_path = args
                .lockfile
                .parent()
                .map(|parent| parent.join(PROJECT_TOML));
            if toml_path.as_ref().is_some_and(|p| p.is_file()) {
                toml_path
            } else {
                None
            }
        }
    };

    if let Some(dependencies) = manifest_path
        .map(|manifest_path| -> Result<Vec<PackageReq>> {
            // TODO(vhyrro): this is hacky and the design of this entire CLI command should be
            // rewritten.
            let project = Project::from(
                manifest_path
                    .parent()
                    .expect("invalid lux.toml path provided"),
            )?
            .ok_or_eyre("Not in a project!")?;
            Ok(project
                .toml()
                .into_remote()?
                .dependencies()
                .current_platform()
                .clone())
        })
        .transpose()?
    {
        sync.add_packages(dependencies);
    }

    sync.sync_dependencies().await.wrap_err("sync failed.")?;

    Ok(())
}
