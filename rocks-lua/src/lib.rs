use mlua::prelude::*;
use rocks_lib::config::ConfigBuilder;

mod build {
    use super::*;
    use mlua::Result;
    use rocks_lib::{
        build::BuildBehaviour,
        config::Config,
        lockfile::{LocalPackage, LockConstraint, PinnedState},
        progress::Progress::NoProgress,
        rockspec::Rockspec,
    };

    pub async fn build(
        rockspec: Rockspec,
        pinned: PinnedState,
        constraint: LockConstraint,
        behaviour: BuildBehaviour,
        config: Config,
    ) -> Result<LocalPackage> {
        rocks_lib::build::build(
            rockspec,
            pinned,
            constraint,
            behaviour,
            &config,
            &NoProgress,
        )
        .await
        .into_lua_err()
    }
}

mod operations {
    use super::*;

    use mlua::Result;
    use rocks_lib::{config::Config, manifest::ManifestMetadata, package::PackageReq, progress::Progress::NoProgress, rockspec::Rockspec};

    pub async fn download_rockspec(package_req: String, manifest: ManifestMetadata, config: Config) -> Result<Rockspec> {
        rocks_lib::operations::download_rockspec(&PackageReq::parse(&package_req).into_lua_err()?, &manifest, &config, &NoProgress).await.into_lua_err()
    }
}

#[mlua::lua_module]
pub fn librocks(lua: &Lua) -> Result<LuaTable, mlua::Error> {
    let rocks = lua.create_table()?;

    rocks.set("build", LuaFunction::wrap_raw_async(build::build))?;
    rocks.set("config", ConfigBuilder::default())?;

    rocks.set("download_rockspec", LuaFunction::wrap_raw_async(operations::download_rockspec));

    Ok(rocks)
}
