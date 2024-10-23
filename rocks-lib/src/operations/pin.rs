use std::io;

use fs_extra::dir::CopyOptions;
use itertools::Itertools;
use thiserror::Error;

use crate::{
    lockfile::{LocalPackage, PinnedState},
    package::RemotePackage,
    tree::Tree,
};

// TODO(vhyrro): Differentiate pinned LocalPackages at the type level?

#[derive(Error, Debug)]
pub enum PinError {
    #[error("rock {rock} is already {}pinned!", if *.pin_state == PinnedState::Unpinned { "un" } else { "" })]
    PinStateUnchanged {
        pin_state: PinnedState,
        rock: RemotePackage,
    },
    #[error("cannot change pin state of {rock}, since a second version of {rock} is already installed with `pin: {}`", .pin_state.as_bool())]
    PinStateConflict {
        pin_state: PinnedState,
        rock: RemotePackage,
    },
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("failed to move old package: {0}")]
    MoveItemsFailure(#[from] fs_extra::error::Error),
}

pub fn set_pinned_state(
    package: &mut LocalPackage,
    tree: &Tree,
    pin: PinnedState,
) -> Result<(), PinError> {
    if pin == package.pinned() {
        return Err(PinError::PinStateUnchanged {
            pin_state: package.pinned(),
            rock: package.to_package(),
        });
    }

    let mut lockfile = tree.lockfile()?;
    let old_package = package.clone();
    let items = std::fs::read_dir(tree.root_for(package))?
        .filter_map(Result::ok)
        .map(|dir| dir.path())
        .collect_vec();

    package.pinned = pin;

    if lockfile.get(&package.id()).is_some() {
        return Err(PinError::PinStateConflict {
            pin_state: package.pinned(),
            rock: package.to_package(),
        });
    }

    let new_root = tree.root_for(package);

    std::fs::create_dir_all(&new_root)?;

    fs_extra::move_items(&items, new_root, &CopyOptions::new())?;

    lockfile.remove(&old_package);
    lockfile.add(package);
    lockfile.flush()?;

    Ok(())
}
