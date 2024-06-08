use std::path::PathBuf;

use crate::config::LuaVersion;
use eyre::Result;

/// A tree is a collection of files where installed rocks are located.
///
/// `rocks` diverges from the traditional hierarchy employed by luarocks.
/// Instead, we opt for a much simpler approach:
///
/// - /rocks/<lua-version> - contains rocks
/// - /rocks/<lua-version>/<rock>/etc - documentation and supplementary files for the rock
/// - /rocks/<lua-version>/<rock>/lib - shared libraries (.so files)
/// - /rocks/<lua-version>/<rock>/src - library code for the rock
/// - /bin - binary files produced by various rocks

pub struct Tree<'a> {
    /// The Lua version of the tree.
    version: &'a LuaVersion,
    /// The root of the tree.
    root: &'a PathBuf,
}

/// Change-agnostic way of referencing various paths for a rock.
pub struct RockLayout {
    pub etc: PathBuf,
    pub lib: PathBuf,
    pub src: PathBuf,
}

impl<'a> Tree<'a> {
    pub fn new(root: &'a PathBuf, version: &'a LuaVersion) -> Result<Self> {
        // Ensure that the root and the version directory exists.
        std::fs::create_dir_all(root.join(version.to_string()))?;

        Ok(Self { root, version })
    }

    pub fn root(&self) -> PathBuf {
        self.root.join(self.version.to_string())
    }

    pub fn bin(&self) -> PathBuf {
        self.root().join("bin")
    }

    pub fn rock(&self, rock_name: &String, rock_version: &String) -> Result<RockLayout> {
        // TODO(vhyrro): Don't store rocks with the revision number, that should be stripped almost
        // everywhere by default.
        let rock_path = self.root().join(format!("{}@{}", rock_name, rock_version));

        let etc = rock_path.join("etc");
        let lib = rock_path.join("lib");
        let src = rock_path.join("src");

        std::fs::create_dir_all(&etc)?;
        std::fs::create_dir_all(&lib)?;
        std::fs::create_dir_all(&src)?;

        Ok(RockLayout { etc, lib, src })
    }
}
