use eyre::Result;
use tempdir::TempDir;

use crate::{config::Config, lua_package::LuaPackageReq, rockspec::Rockspec};

pub async fn install(package_req: LuaPackageReq, config: &Config) -> Result<()> {
    let temp = TempDir::new(&package_req.name().to_string())?;

    let rock = super::download(&package_req, Some(temp.path().to_path_buf()), config).await?;

    super::unpack_src_rock(temp.path().join(rock.path), Some(temp.path().to_path_buf()))?;

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
        Rockspec::new(&std::fs::read_to_string(rockspec_path)?)?,
        config,
    )
    .await?;

    Ok(())
}
