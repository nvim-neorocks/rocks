use clap::Args;
use eyre::Result;
use rocks_lib::{
    config::{Config, LuaVersion},
    operations::download_rockspec,
    package::PackageReq,
    progress::{MultiProgress, Progress},
    remote_package_db::RemotePackageDB,
    tree::Tree,
};

#[derive(Args)]
pub struct Info {
    package: PackageReq,
}

pub async fn info(data: Info, config: Config) -> Result<()> {
    // TODO(vhyrro): Add `Tree::from(&Config)`
    let tree = Tree::new(config.tree().clone(), LuaVersion::from(&config)?)?;
    let package_db = RemotePackageDB::from_config(&config).await?;

    let progress = MultiProgress::new();
    let bar = Progress::Progress(progress.new_bar());

    let rockspec = download_rockspec(&data.package, &package_db, &bar).await?;

    bar.map(|b| b.finish_and_clear());

    if tree.has_rock(&data.package).is_some() {
        println!("Currently installed in {}", tree.root().display());
    }

    println!("Package name: {}", rockspec.package);
    println!("Package version: {}", rockspec.version);
    println!();

    println!(
        "Summary: {}",
        rockspec.description.summary.unwrap_or("None".into())
    );
    println!(
        "Description: {}",
        rockspec
            .description
            .detailed
            .unwrap_or("None".into())
            .trim()
    );
    println!(
        "License: {}",
        rockspec
            .description
            .license
            .unwrap_or("Unknown (all rights reserved by the author)".into())
    );
    println!(
        "Maintainer: {}",
        rockspec
            .description
            .maintainer
            .unwrap_or("Unspecified".into())
    );

    Ok(())
}
