use crate::{
    config::Config, lockfile::LockedPackage, lua_package::LuaPackageReq, progress::with_spinner,
    rockspec::Rockspec,
};

use eyre::Result;
use indicatif::MultiProgress;
use tempdir::TempDir;

pub async fn install(
    progress: &MultiProgress,
    package_req: LuaPackageReq,
    config: &Config,
) -> Result<LockedPackage> {
    with_spinner(
        progress,
        format!("💻 Installing {}", package_req),
        || async { install_impl(progress, package_req, config).await },
    )
    .await
}

async fn install_impl(
    progress: &MultiProgress,
    package_req: LuaPackageReq,
    config: &Config,
) -> Result<LockedPackage> {
    let temp = TempDir::new(&package_req.name().to_string())?;

    let rock = super::download(
        progress,
        &package_req,
        Some(temp.path().to_path_buf()),
        config,
    )
    .await?;

    super::unpack_src_rock(
        progress,
        temp.path().join(rock.path),
        Some(temp.path().to_path_buf()),
    )
    .await?;

    let rockspec_path = walkdir::WalkDir::new(&temp)
        .max_depth(1)
        .same_file_system(true)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .find(|entry| {
            entry.file_type().is_file()
                && entry.path().extension().map(|ext| ext.to_str()) == Some(Some("rockspec"))
        })
        .expect("could not find rockspec in source directory. this is a bug, please report it.")
        .into_path();

    crate::build::build(
        progress,
        Rockspec::new(&std::fs::read_to_string(rockspec_path)?)?,
        config,
    )
    .await
}
