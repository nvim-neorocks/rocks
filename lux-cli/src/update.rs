use clap::Args;
use eyre::{Context, Result};
use lux_lib::progress::{MultiProgress, ProgressBar};
use lux_lib::{config::Config, operations};

#[derive(Args)]
pub struct Update {
    /// Skip the integrity checks for installed rocks when syncing the project lockfile.
    #[arg(long)]
    no_integrity_check: bool,
}

pub async fn update(args: Update, config: Config) -> Result<()> {
    let progress = MultiProgress::new_arc();
    progress.map(|p| p.add(ProgressBar::from("🔎 Looking for updates...".to_string())));

    let updated_packages = operations::Update::new(&config)
        .progress(progress)
        .validate_integrity(!args.no_integrity_check)
        .update()
        .await
        .wrap_err("update failed.")?;

    if updated_packages.is_empty() {
        println!("Nothing to update.");
        return Ok(());
    }

    Ok(())
}
