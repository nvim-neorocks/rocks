use eyre::Result;
use lets_find_up::{find_up_with, FindUpKind, FindUpOptions};
use std::path::PathBuf;

use crate::rockspec::Rockspec;

pub struct Project {
    /// The path where the `project.rockspec` resides.
    pub root: PathBuf,
    //pub rockspec: Rockspec,
}

impl Project {
    pub fn new(start: Option<PathBuf>) -> Result<Option<Self>> {
        match find_up_with(
            "project.rockspec",
            FindUpOptions {
                cwd: &start.unwrap_or(std::env::current_dir()?),
                kind: FindUpKind::File,
            },
        )? {
            Some(path) => {
                //let content = std::fs::read_to_string(&path)?;
                //let rockspec = Rockspec::new(&content)?;

                Ok(Some(Project {
                    root: path.parent().unwrap().to_path_buf(),
                    //rockspec,
                }))
            }
            None => Ok(None),
        }
    }
}
