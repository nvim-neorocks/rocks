use eyre::{bail, Result};
use fs_extra::dir::CopyOptions;
use itertools::Itertools;

use crate::{lockfile::LocalPackage, tree::Tree};

// TODO(vhyrro): Differentiate pinned LocalPackages at the type level?

pub fn pin(package: &mut LocalPackage, tree: &Tree) -> Result<()> {
    if package.pinned() {
        bail!("Rock {} is already pinned!", package.to_package());
    }

    let mut lockfile = tree.lockfile()?;
    let old_package = package.clone();
    let items = std::fs::read_dir(tree.root_for(package))?
        .filter_map(Result::ok)
        .map(|dir| dir.path())
        .collect_vec();

    package.pinned = true;

    if lockfile.get(&package.id()).is_some() {
        bail!("Cannot change pin status of {0}, since a second version of {0} is already installed with `pin: {1}`", package.name(), package.pinned());
    }

    let new_root = tree.root_for(package);

    std::fs::create_dir_all(&new_root)?;

    fs_extra::move_items(&items, new_root, &CopyOptions::new())?;

    lockfile.remove(&old_package);
    lockfile.add(package);
    lockfile.flush()?;

    Ok(())
}
