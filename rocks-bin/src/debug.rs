use crate::unpack::{Unpack, UnpackRemote};
use clap::Subcommand;

#[derive(Subcommand)]
pub enum Debug {
    /// Unpack the contents of a rock.
    Unpack(Unpack),
    /// Download a rock and unpack it.
    UnpackRemote(UnpackRemote),
}
