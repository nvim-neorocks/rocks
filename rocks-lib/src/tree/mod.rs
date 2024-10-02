use std::path::PathBuf;

use crate::{
    config::LuaVersion,
    lua_package::{LuaPackage, LuaPackageReq, PackageName, PackageVersion},
};
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

#[derive(Clone, Debug)]
pub struct Tree {
    /// The Lua version of the tree.
    version: LuaVersion,
    /// The root of the tree.
    root: PathBuf,
}

/// Change-agnostic way of referencing various paths for a rock.
#[derive(Debug, PartialEq)]
pub struct RockLayout {
    pub etc: PathBuf,
    pub lib: PathBuf,
    pub src: PathBuf,
}

impl Tree {
    pub fn new(root: PathBuf, version: LuaVersion) -> Result<Self> {
        // Ensure that the root and the version directory exist.
        std::fs::create_dir_all(root.join(version.to_string()))?;

        // Ensure that the bin directory exists.
        std::fs::create_dir_all(root.join("bin"))?;

        Ok(Self { root, version })
    }

    pub fn root(&self) -> PathBuf {
        self.root.join(self.version.to_string())
    }

    pub fn root_for(&self, rock_name: &PackageName, rock_version: &PackageVersion) -> PathBuf {
        self.root().join(format!("{}@{}", rock_name, rock_version))
    }

    pub fn bin(&self) -> PathBuf {
        self.root.join("bin")
    }

    pub fn has_rock(&self, req: &LuaPackageReq) -> Option<LuaPackage> {
        self.list().get(req.name()).map(|versions| {
            versions.iter().rev().find_map(|version| {
                if req.version_req().matches(version) {
                    Some(LuaPackage::new(req.name().clone(), version.clone()))
                } else {
                    None
                }
            })
        })?
    }

    pub fn rock(
        &self,
        rock_name: &PackageName,
        rock_version: &PackageVersion,
    ) -> Result<RockLayout> {
        // TODO(vhyrro): Don't store rocks with the revision number, that should be stripped almost
        // everywhere by default.
        let rock_path = self.root_for(rock_name, rock_version);

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

    use insta::{assert_yaml_snapshot, sorted_redaction};

    use crate::{config::LuaVersion, tree::RockLayout};

    use super::Tree;

    #[test]
    fn rock_layout() {
        let tree_path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/test/sample-tree");

        let tree = Tree::new(tree_path.clone(), LuaVersion::Lua51).unwrap();

        let neorg = tree
            .rock(&"neorg".into(), &"8.0.0".parse().unwrap())
            .unwrap();

        assert_eq!(
            neorg,
            RockLayout {
                etc: tree_path.join("5.1/neorg@8.0.0/etc"),
                lib: tree_path.join("5.1/neorg@8.0.0/lib"),
                src: tree_path.join("5.1/neorg@8.0.0/src"),
            }
        );

        let lua_cjson = tree
            .rock(&"lua-cjson".into(), &"2.1.0".parse().unwrap())
            .unwrap();

        assert_eq!(
            lua_cjson,
            RockLayout {
                etc: tree_path.join("5.1/lua-cjson@2.1.0/etc"),
                lib: tree_path.join("5.1/lua-cjson@2.1.0/lib"),
                src: tree_path.join("5.1/lua-cjson@2.1.0/src"),
            }
        );
    }

    #[test]
    fn tree_list() {
        let tree_path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/test/sample-tree");

        let tree = Tree::new(tree_path, LuaVersion::Lua51).unwrap();

        assert_yaml_snapshot!(tree.list(), { "." => sorted_redaction() })
    }
}
