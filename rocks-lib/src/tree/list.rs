use eyre::Result;
use std::collections::HashMap;

use itertools::Itertools;
use walkdir::WalkDir;

use crate::lua_package::LuaPackage;

use super::Tree;

impl Tree {
    pub fn list(&self) -> HashMap<String, Vec<String>> {
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
                (name.to_string(), version.to_string())
            })
            .into_group_map()
    }

    pub fn into_rock_list(self) -> Result<Vec<LuaPackage>> {
        let rock_list = self.list();

        rock_list
            .into_iter()
            .flat_map(|(name, versions)| {
                versions
                    .into_iter()
                    .map(|version| LuaPackage::parse(name.clone(), version))
                    .collect_vec()
            })
            .try_collect()
    }
}

impl TryFrom<Tree> for Vec<LuaPackage> {
    type Error = eyre::Report;

    fn try_from(tree: Tree) -> Result<Self> {
        tree.into_rock_list()
    }
}
