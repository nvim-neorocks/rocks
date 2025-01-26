use clap::Args;
use eyre::Result;
use rocks_lib::{
    config::{Config, LuaVersion},
    operations::Download,
    package::PackageReq,
    progress::{MultiProgress, Progress},
    rockspec::Rockspec,
    tree::Tree,
};

#[derive(Args)]
pub struct Info {
    package: PackageReq,
}

pub async fn info(data: Info, config: Config) -> Result<()> {
    // TODO(vhyrro): Add `Tree::from(&Config)`
    let tree = Tree::new(config.tree().clone(), LuaVersion::from(&config)?)?;

    let progress = MultiProgress::new();
    let bar = Progress::Progress(progress.new_bar());

    let rockspec = Download::new(&data.package, &config, &bar)
        .download_rockspec()
        .await?
        .rockspec;

    bar.map(|b| b.finish_and_clear());

    if tree.match_rocks(&data.package)?.is_found() {
        println!("Currently installed in {}", tree.root().display());
    }

    println!("Package name: {}", rockspec.package());
    println!("Package version: {}", rockspec.version());
    println!();

    println!(
        "Summary: {}",
        rockspec
            .description()
            .summary
            .as_ref()
            .unwrap_or(&"None".to_string())
    );
    println!(
        "Description: {}",
        rockspec
            .description()
            .detailed
            .as_ref()
            .unwrap_or(&"None".to_string())
            .trim()
    );
    println!(
        "License: {}",
        rockspec
            .description()
            .license
            .as_ref()
            .unwrap_or(&"Unknown (all rights reserved by the author)".to_string())
    );
    println!(
        "Maintainer: {}",
        rockspec
            .description()
            .maintainer
            .as_ref()
            .unwrap_or(&"Unspecified".to_string())
    );

    Ok(())
}
