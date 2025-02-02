use std::path::PathBuf;

use clap::Args;
use eyre::{eyre, Context, Result};
use rocks_lib::{
    config::{Config, LuaVersion},
    lockfile::Lockfile,
    operations,
    package::PackageReq,
    project::{rocks_toml::RocksToml, ROCKS_TOML},
    rockspec::Rockspec,
};

#[derive(Args)]
pub struct Sync {
    /// The path to the lockfile to synchronise from.
    lockfile: PathBuf,

    /// Path to a rocks.toml.
    /// If set, 'rocks sync' will also synchronise the dependencies in the rocks.toml
    /// with the lockfile.
    /// This is useful if dependencies have been added or removed manually
    /// and the lockfile is out of sync.
    ///
    /// If not set, rocks will check the lockfile's parent directory for a
    /// rocks.toml file and use that.
    manifest_path: Option<PathBuf>,

    /// Skip the integrity checks for installed rocks.
    #[arg(long)]
    no_integrity_check: bool,
}

pub async fn sync(args: Sync, config: Config) -> Result<()> {
    let tree = config.tree(LuaVersion::from(&config)?)?;

    let mut lockfile = Lockfile::new(args.lockfile.clone())?;

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
            let rocks_path = args.lockfile.parent().map(|parent| parent.join(ROCKS_TOML));
            if rocks_path.as_ref().is_some_and(|p| p.is_file()) {
                rocks_path
            } else {
                None
            }
        }
    };

    if let Some(dependencies) = manifest_path
        .map(|manifest_path| -> Result<Vec<PackageReq>> {
            let content = std::fs::read_to_string(&manifest_path)?;
            let rocks = RocksToml::new(&content)?;
            Ok(rocks
                .into_validated_rocks_toml()?
                .dependencies()
                .current_platform()
                .clone())
        })
        .transpose()?
    {
        sync.add_packages(dependencies);
    }

    sync.sync().await.wrap_err("sync failed.")?;

    Ok(())
}
