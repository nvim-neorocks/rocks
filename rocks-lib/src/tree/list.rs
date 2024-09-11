use eyre::Result;
use std::collections::HashMap;

use itertools::Itertools;
use walkdir::WalkDir;

use crate::luarock::LuaRock;

use super::Tree;

impl<'a> Tree<'a> {
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

    pub fn into_rock_list(self) -> Result<Vec<LuaRock>> {
        let rock_list = self.list();

        Ok(rock_list
            .into_iter()
            .flat_map(|(name, versions)| {
                versions
                    .into_iter()
                    .map(|version| LuaRock::new(name.clone(), version))
                    .collect_vec()
            })
            .try_collect()?)
    }
}

impl<'a> TryFrom<Tree<'a>> for Vec<LuaRock> {
    type Error = eyre::Report;

    fn try_from(tree: Tree<'a>) -> Result<Self> {
        tree.into_rock_list()
    }
}
