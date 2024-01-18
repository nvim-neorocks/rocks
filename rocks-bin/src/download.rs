use anyhow::Result;
use clap::Args;

#[derive(Args)]
pub struct Download {
    name: String,
    version: String,
}

pub async fn download(dl_data: Download) -> Result<()> {
    rocks_lib::rocks::download(&dl_data.name, Some(&dl_data.version)).await?;

    Ok(())
}
