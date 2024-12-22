use lets_find_up::{find_up_with, FindUpKind, FindUpOptions};
use std::{
    io,
    path::{Path, PathBuf},
};
use thiserror::Error;

use crate::{
    config::LuaVersion,
    rockspec::{Rockspec, RockspecError},
    tree::Tree,
};

#[derive(Error, Debug)]
#[error(transparent)]
pub enum ProjectError {
    Io(#[from] io::Error),
    Rockspec(#[from] RockspecError),
}

#[derive(Debug)]
pub struct Project {
    /// The path where the `project.rockspec` resides.
    root: PathBuf,
    /// The parsed rockspec.
    rockspec: Rockspec,
}

impl Project {
    pub fn current() -> Result<Option<Self>, ProjectError> {
        Self::from(&std::env::current_dir()?)
    }

    pub fn from(start: impl AsRef<Path>) -> Result<Option<Self>, ProjectError> {
        if !start.as_ref().exists() {
            return Ok(None);
        }

        match find_up_with(
            "project.rockspec",
            FindUpOptions {
                cwd: start.as_ref(),
                kind: FindUpKind::File,
            },
        )? {
            Some(path) => {
                let rockspec_content = std::fs::read_to_string(&path)?;
                let rockspec = Rockspec::new(&rockspec_content)?;

                let root = path.parent().unwrap();

                std::fs::create_dir_all(root)?;

                Ok(Some(Project {
                    root: root.to_path_buf(),
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

    pub fn tree(&self, lua_version: LuaVersion) -> io::Result<Tree> {
        Tree::new(self.root.clone(), lua_version)
    }
}

// TODO: Add plenty of tests
