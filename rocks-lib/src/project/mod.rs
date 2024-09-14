use eyre::Result;
use lets_find_up::{find_up_with, FindUpKind, FindUpOptions};
use std::
    path::{Path, PathBuf}
;

use crate::{config::LuaVersion, rockspec::Rockspec, tree::Tree};

#[derive(Debug)]
pub struct Project {
    /// The path where the `project.rockspec` resides.
    root: PathBuf,
    /// The parsed rockspec.
    rockspec: Rockspec,
}

impl Project {
    pub fn current() -> Result<Option<Self>> {
        Self::from(std::env::current_dir()?)
    }

    pub fn from(start: PathBuf) -> Result<Option<Self>> {
        match find_up_with(
            "project.rockspec",
            FindUpOptions {
                cwd: &start,
                kind: FindUpKind::File,
            },
        )? {
            Some(path) => {
                let rockspec_content = std::fs::read_to_string(&path)?;
                let rockspec = Rockspec::new(&rockspec_content)?;

                let root = path.parent().unwrap().to_path_buf().join(".rocks");

                std::fs::create_dir_all(&root)?;

                Ok(Some(Project {
                    root,
                    rockspec,
                }))
            }
            None => Ok(None),
        }
    }
}

impl Project {
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn rockspec(&self) -> &Rockspec {
        &self.rockspec
    }

    pub fn tree(&self, lua_version: LuaVersion) -> Result<Tree> {
        Tree::new(self.root.clone(), lua_version)
    }
}

// TODO: Add plenty of tests
