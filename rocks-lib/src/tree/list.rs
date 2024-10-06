use std::collections::HashMap;

use eyre::Result;
use itertools::Itertools;

use crate::{lockfile::LocalPackage, remote_package::PackageName};

use super::Tree;

// TODO: Due to the whininess of the borrow checker, we resort to cloning package information
// whenever returning it. In the future, it'd be greatly benefitial to instead return mutable
// references to the packages, which would allow for in-place manipulation of the lockfile.
// Cloning isn't destructive, but it's sure expensive.

impl Tree {
    pub fn list(&self) -> Result<HashMap<PackageName, Vec<LocalPackage>>> {
        Ok(self
            .lockfile()?
            .rocks()
            .values()
            .cloned()
            .map(|locked_rock| (locked_rock.name.clone(), locked_rock))
            .into_group_map())
    }

    pub fn as_rock_list(&self) -> Result<Vec<LocalPackage>> {
        let rock_list = self.list()?;

        Ok(rock_list.values().flatten().cloned().collect())
    }
}
