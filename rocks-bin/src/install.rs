use eyre::Result;
use inquire::Confirm;
use itertools::Itertools;
use rocks_lib::{
    build::BuildBehaviour,
    config::{Config, LuaVersion},
    lockfile::PinnedState,
    manifest::Manifest,
    package::PackageReq,
    progress::MultiProgress,
    tree::Tree,
};

#[derive(clap::Args)]
pub struct Install {
    /// Package or list of packages to install.
    package_req: Vec<PackageReq>,

    /// Pin the package so that it doesn't get updated.
    #[arg(long)]
    pin: bool,

    /// Reinstall without prompt if a package is already installed.
    #[arg(long)]
    force: bool,
}

pub async fn install(data: Install, config: Config) -> Result<()> {
    let pin = PinnedState::from(data.pin);

    let lua_version = LuaVersion::from(&config)?;
    let tree = Tree::new(config.tree().clone(), lua_version)?;

    let packages = data
        .package_req
        .into_iter()
        .filter_map(|req| {
            let build_behaviour: Option<BuildBehaviour> =
                match tree.has_rock_and(&req, |rock| pin == rock.pinned()) {
                    Some(_) if !data.force => {
                        if Confirm::new(&format!("Package {} already exists. Overwrite?", req))
                            .with_default(false)
                            .prompt()
                            .expect("Error prompting for reinstall")
                        {
                            Some(BuildBehaviour::Force)
                        } else {
                            None
                        }
                    }
                    _ => Some(BuildBehaviour::from(data.force)),
                };
            build_behaviour.map(|it| (it, req))
        })
        .collect_vec();

    let manifest = Manifest::from_config(config.server(), &config).await?;

    // TODO(vhyrro): If the tree doesn't exist then error out.
    rocks_lib::operations::install(packages, pin, &manifest, &config, MultiProgress::new_arc())
        .await?;

    Ok(())
}
