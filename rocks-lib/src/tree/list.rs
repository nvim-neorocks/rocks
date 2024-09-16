use std::collections::HashMap;

use itertools::Itertools;
use walkdir::WalkDir;

use crate::lua_package::{LuaPackage, PackageName, PackageVersion};

use super::Tree;

impl Tree {
    pub fn list(&self) -> HashMap<PackageName, Vec<PackageVersion>> {
        // TODO: Replace this with a single lockfile read and cache it.
        WalkDir::new(self.root())
            .min_depth(1)
            .max_depth(1)
            .into_iter()
            .map(|rock_directory| {
                let rock_dir = rock_directory.unwrap();
                let (name, version) = rock_dir
                    .file_name()
                    .to_str()
                    .unwrap()
                    .split_once('@')
                    .unwrap();
                (
                    PackageName::new(name.to_string()),
                    PackageVersion::parse(version).unwrap_or_else(|_| {
                        panic!(
                            "invalid version found in rocktree at '{}'. This is a bug!",
                            rock_dir.path().display()
                        )
                    }),
                )
            })
            .into_group_map()
    }

    pub fn into_rock_list(self) -> Vec<LuaPackage> {
        let rock_list = self.list();

        rock_list
            .into_iter()
            .flat_map(|(name, versions)| {
                versions
                    .into_iter()
                    .map(|version| LuaPackage::new(name.clone(), version))
                    .collect_vec()
            })
            .collect()
    }
}

impl From<Tree> for Vec<LuaPackage> {
    fn from(tree: Tree) -> Self {
        tree.into_rock_list()
    }
}
