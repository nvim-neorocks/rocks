use std::collections::HashMap;

use itertools::Itertools;
use walkdir::WalkDir;

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
}
