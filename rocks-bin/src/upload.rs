use clap::Args;
use eyre::Result;
use rocks_lib::{
    config::Config,
    project::Project,
    upload::{upload_from_project, ApiKey, SignatureProtocol},
};

#[derive(Args)]
pub struct Upload {
    #[arg(long, default_value_t)]
    sign_protocol: SignatureProtocol,
}

pub async fn upload(data: Upload, config: Config) -> Result<()> {
    let project = Project::current()?.unwrap();

    upload_from_project(&project, &ApiKey::new()?, data.sign_protocol, &config).await?;

    Ok(())
}
