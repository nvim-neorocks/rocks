use eyre::{bail, Result};
use fs_extra::dir::CopyOptions;
use itertools::Itertools;

use crate::{
    lockfile::{LocalPackage, PinnedState},
    tree::Tree,
};

// TODO(vhyrro): Differentiate pinned LocalPackages at the type level?

pub fn set_pinned_state(package: &mut LocalPackage, tree: &Tree, pin: PinnedState) -> Result<()> {
    if pin == package.pinned() {
        bail!(
            "Rock {} is already {}pinned!",
            package.to_package(),
            if pin == PinnedState::Unpinned {
                "un"
            } else {
                ""
            }
        );
    }

    let mut lockfile = tree.lockfile()?;
    let old_package = package.clone();
    let items = std::fs::read_dir(tree.root_for(package))?
        .filter_map(Result::ok)
        .map(|dir| dir.path())
        .collect_vec();

    package.pinned = pin;

    let new_root = tree.root_for(package);

    std::fs::create_dir_all(&new_root)?;

    fs_extra::move_items(&items, new_root, &CopyOptions::new())?;

    lockfile.remove(&old_package);
    lockfile.add(package);
    lockfile.flush()?;

    Ok(())
}
