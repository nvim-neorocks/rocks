use lets_find_up::{find_up_with, FindUpKind, FindUpOptions};
use rocks_toml::{RocksToml, RocksTomlValidationError};
use std::{
    io,
    path::{Path, PathBuf},
};
use thiserror::Error;

use crate::{
    config::LuaVersion,
    lockfile::{Lockfile, ReadOnly},
    lua_rockspec::{LuaRockspec, RockSourceSpec, RockspecError},
    rockspec::Rockspec,
    tree::Tree,
};

pub mod rocks_toml;

pub const ROCKS_TOML: &str = "rocks.toml";

#[derive(Error, Debug)]
#[error(transparent)]
pub enum ProjectError {
    Io(#[from] io::Error),
    Rocks(#[from] RocksTomlValidationError),
    Toml(#[from] toml::de::Error),
}

#[derive(Error, Debug)]
#[error(transparent)]
pub enum IntoRockspecError {
    RocksTomlValidationError(#[from] RocksTomlValidationError),
    RockspecError(#[from] RockspecError),
}

#[derive(Clone, Debug)]
pub struct Project {
    /// The path where the `project.rockspec` resides.
    root: PathBuf,
    /// The parsed rockspec.
    rocks: RocksToml,
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
            ROCKS_TOML,
            FindUpOptions {
                cwd: start.as_ref(),
                kind: FindUpKind::File,
            },
        )? {
            Some(path) => {
                let rocks_content = std::fs::read_to_string(&path)?;
                let rocks = RocksToml::new(&rocks_content)?;

                let root = path.parent().unwrap();

                std::fs::create_dir_all(root)?;

                Ok(Some(Project {
                    root: root.to_path_buf(),
                    rocks,
                }))
            }
            None => Ok(None),
        }
    }

    /// Get the `rocks.toml` path.
    pub fn rocks_path(&self) -> PathBuf {
        self.root.join(ROCKS_TOML)
    }

    /// Get the `rocks.lock` lockfile path.
    pub fn lockfile_path(&self) -> PathBuf {
        self.root.join("rocks.lock")
    }

    /// Get the `rocks.lock` lockfile in the project root, if present.
    pub fn lockfile(&self) -> Result<Option<Lockfile<ReadOnly>>, ProjectError> {
        let path = self.lockfile_path();
        if path.is_file() {
            Ok(Some(Lockfile::new(path)?))
        } else {
            Ok(None)
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn rocks(&self) -> &RocksToml {
        &self.rocks
    }

    pub fn rockspec(&self) -> Result<LuaRockspec, IntoRockspecError> {
        Ok(self.rocks().into_validated_rocks_toml()?.to_rockspec()?)
    }

    /// Create a RockSpec with the source set to the project root.
    pub fn new_local_rockspec(&self) -> Result<LuaRockspec, IntoRockspecError> {
        let mut rocks = self.rockspec()?;
        let mut source = rocks.source().current_platform().clone();
        source.source_spec = RockSourceSpec::File(self.root().to_path_buf());
        source.archive_name = None;
        source.integrity = None;
        rocks.source_mut().current_platform_set(source);
        Ok(rocks)
    }

    pub fn tree(&self, lua_version: LuaVersion) -> io::Result<Tree> {
        Tree::new(self.root.join(".rocks"), lua_version)
    }
}

// TODO: Add plenty of tests
