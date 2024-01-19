use anyhow::Result;
use clap::Args;
use rocks_lib::config::Config;

#[derive(Args)]
pub struct Download {
    name: String,
    version: String,
}

pub async fn download(dl_data: Download, config: &Config) -> Result<()> {
    rocks_lib::rocks::download(&dl_data.name, Some(&dl_data.version), &config).await?;

    Ok(())
}
