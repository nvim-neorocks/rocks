use std::path::PathBuf;

use crate::config::LuaVersion;
use eyre::Result;

mod list;

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
#[derive(Debug, PartialEq)]
pub struct RockLayout {
    pub etc: PathBuf,
    pub lib: PathBuf,
    pub src: PathBuf,
}

impl<'a> Tree<'a> {
    pub fn new(root: &'a PathBuf, version: &'a LuaVersion) -> Result<Self> {
        // Ensure that the root and the version directory exist.
        std::fs::create_dir_all(root.join(version.to_string()))?;

        // Ensure that the bin directory exists.
        std::fs::create_dir_all(root.join("bin"))?;

        Ok(Self { root, version })
    }

    pub fn root(&self) -> PathBuf {
        self.root.join(self.version.to_string())
    }

    pub fn bin(&self) -> PathBuf {
        self.root.join("bin")
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    // use insta::{assert_yaml_snapshot, sorted_redaction};

    use crate::{config::LuaVersion, tree::RockLayout};

    use super::Tree;

    #[test]
    fn rock_layout() {
        let tree_path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/test/sample-tree");

        let tree = Tree::new(&tree_path, &LuaVersion::Lua51).unwrap();

        let neorg = tree
            .rock(&"neorg".to_string(), &"8.0.0-1".to_string())
            .unwrap();

        assert_eq!(
            neorg,
            RockLayout {
                etc: tree_path.join("5.1/neorg@8.0.0-1/etc"),
                lib: tree_path.join("5.1/neorg@8.0.0-1/lib"),
                src: tree_path.join("5.1/neorg@8.0.0-1/src"),
            }
        );

        let lua_cjson = tree
            .rock(&"lua-cjson".to_string(), &"2.1.0.9-1".to_string())
            .unwrap();

        assert_eq!(
            lua_cjson,
            RockLayout {
                etc: tree_path.join("5.1/lua-cjson@2.1.0.9-1/etc"),
                lib: tree_path.join("5.1/lua-cjson@2.1.0.9-1/lib"),
                src: tree_path.join("5.1/lua-cjson@2.1.0.9-1/src"),
            }
        );
    }

    // FIXME: This fails in nix checks
    // #[test]
    // fn tree_list() {
    //     let tree_path =
    //         PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/test/sample-tree");
    //
    //     let tree = Tree::new(&tree_path, &LuaVersion::Lua51).unwrap();
    //
    //     assert_yaml_snapshot!(tree.list(), { "." => sorted_redaction() })
    // }
}
