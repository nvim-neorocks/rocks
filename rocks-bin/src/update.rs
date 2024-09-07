use clap::Args;

#[derive(Args)]
pub struct Update {
    /// Don't update, just list the outdated rocks.
    #[arg(long)]
    list: bool,
}
