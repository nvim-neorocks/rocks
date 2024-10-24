use std::io;

use fs_extra::dir::CopyOptions;
use itertools::Itertools;
use thiserror::Error;

use crate::{lockfile::LocalPackage, package::RemotePackage, tree::Tree};

// TODO(vhyrro): Differentiate pinned LocalPackages at the type level?

#[derive(Error, Debug)]
pub enum PinError {
    #[error("rock {0} is already pinned!")]
    PinStateUnchanged(RemotePackage),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("failed to move old package: {0}")]
    MoveItemsFailure(#[from] fs_extra::error::Error),
}

pub fn pin(package: &mut LocalPackage, tree: &Tree) -> Result<(), PinError> {
    if package.pinned() {
        return Err(PinError::PinStateUnchanged(package.to_package()));
    }

    let mut lockfile = tree.lockfile()?;
    let old_package = package.clone();
    let items = std::fs::read_dir(tree.root_for(package))?
        .filter_map(Result::ok)
        .map(|dir| dir.path())
        .collect_vec();

    package.pinned = true;

    let new_root = tree.root_for(package);

    std::fs::create_dir_all(&new_root)?;

    fs_extra::move_items(&items, new_root, &CopyOptions::new())?;

    lockfile.remove(&old_package);
    lockfile.add(package);
    lockfile.flush()?;

    Ok(())
}
