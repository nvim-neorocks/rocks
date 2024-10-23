use lets_find_up::{find_up_with, FindUpKind, FindUpOptions};
use mlua::{Lua, LuaSerdeExt};
use serde::{Deserialize, Serialize};
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
    /// A partial rockspec representation for write operations
    partial_rockspec: PartialRockspec,
}

#[derive(Debug, Serialize)]
pub struct PartialRockspec {
    pub dependencies: Vec<String>,
}

impl PartialRockspec {
    // TODO: Don't propagate different error types
    pub fn new(rockspec_content: &str) -> mlua::Result<Self> {
        let lua = Lua::new();
        lua.load(dbg!(rockspec_content)).exec()?;

        let globals = lua.globals();

        Ok(PartialRockspec {
            dependencies: globals.get("dependencies")?,
        })
    }
}

impl Project {
    pub fn current() -> Result<Option<Self>, ProjectError> {
        Self::from(std::env::current_dir()?)
    }

    pub fn from(start: PathBuf) -> Result<Option<Self>, ProjectError> {
        match find_up_with(
            "project.rockspec",
            FindUpOptions {
                cwd: &start,
                kind: FindUpKind::File,
            },
        )? {
            Some(path) => {
                let rockspec_content = std::fs::read_to_string(&path)?;
                let partial_rockspec = PartialRockspec::new(&rockspec_content).unwrap();
                let rockspec = Rockspec::new(&rockspec_content)?;

                let root = path.parent().unwrap();

                std::fs::create_dir_all(root)?;

                Ok(Some(Project {
                    root: root.to_path_buf(),
                    rockspec,
                    partial_rockspec,
                }))
            }
            None => Ok(None),
        }
    }

    pub fn flush(&mut self) {
        let lua = Lua::new();
        std::fs::write(self.root().join("project.rockspec"), lua.to_value(&self.partial_rockspec).unwrap().to_string().unwrap()).unwrap();
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = self.flush();
    }
}

impl Project {
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn rockspec(&self) -> &Rockspec {
        &self.rockspec
    }

    pub fn rockspec_mut(&mut self) -> &mut PartialRockspec {
        &mut self.partial_rockspec
    }

    pub fn tree(&self, lua_version: LuaVersion) -> io::Result<Tree> {
        Tree::new(self.root.clone(), lua_version)
    }
}

// TODO: Add plenty of tests
