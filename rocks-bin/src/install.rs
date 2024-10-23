use eyre::Result;
use indicatif::MultiProgress;
use inquire::Confirm;
use rocks_lib::{
    build::BuildBehaviour,
    config::{Config, LuaVersion},
    lockfile::PinnedState,
    package::PackageReq,
    tree::Tree,
};

#[derive(clap::Args)]
pub struct Install {
    package_req: PackageReq,

    #[arg(long)]
    pin: bool,

    #[arg(long)]
    force: bool,
}

pub async fn install(data: Install, config: Config) -> Result<()> {
    let pin = PinnedState::from(data.pin);

    let lua_version = LuaVersion::from(&config)?;
    let tree = Tree::new(config.tree().clone(), lua_version)?;

    let build_behaviour = match tree.has_rock_and(&data.package_req, |rock| pin == rock.pinned()) {
        Some(_) if !data.force => {
            if Confirm::new(&format!(
                "Package {} already exists. Overwrite?",
                data.package_req
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

    // TODO(vhyrro): If the tree doesn't exist then error out.
    rocks_lib::operations::install(
        &MultiProgress::new(),
        data.package_req,
        pin,
        build_behaviour,
        &config,
    )
    .await?;

    Ok(())
}
