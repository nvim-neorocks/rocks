use std::collections::HashMap;

use eyre::Result;
use itertools::Itertools;

use crate::lua_package::{LuaPackage, PackageName, PackageVersion};

use super::Tree;

impl Tree {
    pub fn list(&self) -> Result<HashMap<PackageName, Vec<PackageVersion>>> {
        let lockfile = self.lockfile()?;

        Ok(lockfile
            .rocks()
            .values()
            .cloned()
            .map(|locked_rock| (locked_rock.name, locked_rock.version))
            .into_group_map())
    }

    pub fn into_rock_list(self) -> Result<Vec<LuaPackage>> {
        let rock_list = self.list()?;

        Ok(rock_list
            .into_iter()
            .flat_map(|(name, versions)| {
                versions
                    .into_iter()
                    .map(|version| LuaPackage::new(name.clone(), version))
                    .collect_vec()
            })
            .collect())
    }
}

impl TryFrom<Tree> for Vec<LuaPackage> {
    type Error = eyre::Report;

    fn try_from(tree: Tree) -> std::result::Result<Self, Self::Error> {
        tree.into_rock_list()
    }
}
