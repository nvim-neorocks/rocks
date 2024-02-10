use clap::Args;
use eyre::Result;

use crate::rockspec::github_metadata;

#[derive(Args)]
pub struct WriteRockspec {

}

pub async fn write_rockspec(data: WriteRockspec) -> Result<()> {
    let repo_metadata = github_metadata::get_metadata_for(None).await?.unwrap();
    println!("{:?}", repo_metadata);
    todo!()
}
