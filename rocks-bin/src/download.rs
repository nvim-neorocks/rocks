use clap::Args;
use eyre::Result;
use rocks_lib::config::Config;

#[derive(Args)]
pub struct Download {
    name: String,
    version: Option<String>,
}

pub async fn download(dl_data: Download, config: &Config) -> Result<()> {
    println!("Downloading {}...", dl_data.name);

    let (name, version) =
        rocks_lib::rocks::download(&dl_data.name, dl_data.version.as_ref(), None, &config).await?;

    println!("Succesfully downloaded {}@{}", name, version);

    Ok(())
}
