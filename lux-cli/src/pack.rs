use std::{path::PathBuf, str::FromStr};

use clap::Args;
use eyre::{eyre, OptionExt, Result};
use lux_lib::{
    build::{Build, BuildBehaviour},
    config::{Config, LuaVersion},
    lua_rockspec::RemoteLuaRockspec,
    operations::{self, Install},
    package::PackageReq,
    progress::MultiProgress,
    project::Project,
    tree::Tree,
};
use tempdir::TempDir;

#[derive(Debug, Clone)]
pub enum PackageOrRockspec {
    Package(PackageReq),
    RockSpec(PathBuf),
}

impl FromStr for PackageOrRockspec {
    type Err = eyre::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let path = PathBuf::from(s);
        if path.is_file() {
            Ok(Self::RockSpec(path))
        } else {
            let pkg = PackageReq::from_str(s).map_err(|err| {
                eyre!(
                    "No file {0} found and cannot parse package query: {1}",
                    s,
                    err
                )
            })?;
            Ok(Self::Package(pkg))
        }
    }
}

#[derive(Args)]
pub struct Pack {
    /// Path to a RockSpec or a package query for a package to pack.
    /// Prioritises installed rocks and will install a rock to a temporary
    /// directory if none is found.
    /// In case of multiple matches, the latest version will be packed.
    /// Examples: "pkg", "pkg@1.0.0", "pkg>=1.0.0"
    ///
    /// If not set, rocks will pack the current project's lux.toml.
    #[clap(value_parser)]
    package_or_rockspec: Option<PackageOrRockspec>,
}

pub async fn pack(args: Pack, config: Config) -> Result<()> {
    let lua_version = LuaVersion::from(&config)?;
    let dest_dir = std::env::current_dir()?;
    let progress = MultiProgress::new_arc();
    let package_or_rockspec = match args.package_or_rockspec {
        Some(package_or_rockspec) => package_or_rockspec,
        None => {
            let project = Project::current()?.ok_or_eyre("Not in a project!")?;
            PackageOrRockspec::RockSpec(project.toml_path())
        }
    };
    let result: Result<PathBuf> = match package_or_rockspec {
        PackageOrRockspec::Package(package_req) => {
            let default_tree = config.tree(lua_version.clone())?;
            match default_tree.match_rocks(&package_req)? {
                lux_lib::tree::RockMatches::NotFound(_) => {
                    let temp_dir = TempDir::new("lux-pack")?.into_path();
                    let tree = Tree::new(temp_dir.clone(), lua_version.clone())?;
                    let packages = Install::new(&tree, &config)
                        .package(BuildBehaviour::Force, package_req)
                        .progress(progress)
                        .install()
                        .await?;
                    let package = packages.first().unwrap();
                    let rock_path =
                        operations::Pack::new(dest_dir, tree, package.clone()).pack()?;
                    Ok(rock_path)
                }
                lux_lib::tree::RockMatches::Single(local_package_id) => {
                    let lockfile = default_tree.lockfile()?;
                    let package = lockfile.get(&local_package_id).unwrap();
                    let rock_path =
                        operations::Pack::new(dest_dir, default_tree, package.clone()).pack()?;
                    Ok(rock_path)
                }
                lux_lib::tree::RockMatches::Many(vec) => {
                    let local_package_id = vec.first().unwrap();
                    let lockfile = default_tree.lockfile()?;
                    let package = lockfile.get(local_package_id).unwrap();
                    let rock_path =
                        operations::Pack::new(dest_dir, default_tree, package.clone()).pack()?;
                    Ok(rock_path)
                }
            }
        }
        PackageOrRockspec::RockSpec(rockspec_path) => {
            let content = std::fs::read_to_string(&rockspec_path)?;
            let rockspec = match rockspec_path
                .extension()
                .map(|ext| ext.to_string_lossy().to_string())
                .unwrap_or("".into())
                .as_str()
            {
                ".rockspec" => Ok(RemoteLuaRockspec::new(&content)?),
                _ => Err(eyre!(
                    "expected a path to a .rockspec or a package requirement."
                )),
            }?;
            let temp_dir = TempDir::new("lux-pack")?.into_path();
            let bar = progress.map(|p| p.new_bar());
            let tree = Tree::new(temp_dir.clone(), lua_version.clone())?;
            let config = config.with_tree(temp_dir);
            let package = Build::new(&rockspec, &tree, &config, &bar).build().await?;
            let rock_path = operations::Pack::new(dest_dir, tree, package).pack()?;
            Ok(rock_path)
        }
    };
    print!("packed rock created at {}", result?.display());
    Ok(())
}
