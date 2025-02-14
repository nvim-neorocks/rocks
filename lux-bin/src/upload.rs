use clap::Args;
use eyre::Result;
use lux_lib::{
    config::Config,
    project::Project,
    upload::{ProjectUpload, SignatureProtocol},
};

#[derive(Args)]
pub struct Upload {
    #[arg(long, default_value_t)]
    sign_protocol: SignatureProtocol,
}

pub async fn upload(data: Upload, config: Config) -> Result<()> {
    let project = Project::current()?.unwrap();

    ProjectUpload::new(project, &config)
        .sign_protocol(data.sign_protocol)
        .upload_to_luarocks()
        .await?;

    Ok(())
}
