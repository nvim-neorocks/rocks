use crate::{
    build::variables::{self, HasVariables},
    config::LuaVersion,
    lockfile::Lockfile,
    lua_package::{LuaPackage, LuaPackageReq},
};
use eyre::Result;
use std::path::PathBuf;

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
    /// The local installation directory.
    /// Can be substituted in a rockspec's `build.build_variables` and `build.install_variables`
    /// using `$(PREFIX)`.
    pub rock_path: PathBuf,
    /// The `etc` directory, containing resources.
    pub etc: PathBuf,
    /// The `lib` directory, containing native libraries.
    /// Can be substituted in a rockspec's `build.build_variables` and `build.install_variables`
    /// using `$(LIBDIR)`.
    pub lib: PathBuf,
    /// The `src` directory, containing Lua sources.
    /// Can be substituted in a rockspec's `build.build_variables` and `build.install_variables`
    /// using `$(LUADIR)`.
    pub src: PathBuf,
    /// The `bin` directory, containing executables.
    /// Can be substituted in a rockspec's `build.build_variables` and `build.install_variables`
    /// using `$(BINDIR)`.
    pub bin: PathBuf,
    /// The `etc/conf` directory, containing configuration files.
    /// Can be substituted in a rockspec's `build.build_variables` and `build.install_variables`
    /// using `$(CONFDIR)`.
    pub conf: PathBuf,
    /// The `etc/doc` directory, containing documentation files.
    /// Can be substituted in a rockspec's `build.build_variables` and `build.install_variables`
    /// using `$(DOCDIR)`.
    pub doc: PathBuf,
}

impl HasVariables for RockLayout {
    /// Substitute `$(VAR)` with one of the paths, where `VAR`
    /// is one of `PREFIX`, `LIBDIR`, `LUADIR`, `BINDIR`, `CONFDIR` or `DOCDIR`.
    fn substitute_variables(&self, input: String) -> String {
        variables::substitute(
            |var| {
                let path = match var {
                    "PREFIX" => Some(self.rock_path.clone()),
                    "LIBDIR" => Some(self.lib.clone()),
                    "LUADIR" => Some(self.src.clone()),
                    "BINDIR" => Some(self.bin.clone()),
                    "CONFDIR" => Some(self.conf.clone()),
                    "DOCDIR" => Some(self.doc.clone()),
                    _ => None,
                }?;
                Some(path.to_string_lossy().to_string())
            },
            input,
        )
    }
}

impl Tree {
    pub fn new(root: PathBuf, version: LuaVersion) -> Result<Self> {
        let path_with_version = root.join(version.to_string());

        // Ensure that the root and the version directory exist.
        std::fs::create_dir_all(&path_with_version)?;

        // Ensure that the bin directory exists.
        std::fs::create_dir_all(root.join("bin"))?;

        Ok(Self { root, version })
    }

    pub fn root(&self) -> PathBuf {
        self.root.join(self.version.to_string())
    }

    pub fn root_for(&self, package: &LuaPackage) -> PathBuf {
        self.root()
            .join(format!("{}@{}", package.name(), package.version()))
    }

    pub fn bin(&self) -> PathBuf {
        self.root.join("bin")
    }

    pub fn has_rock(&self, req: &LuaPackageReq) -> Option<LuaPackage> {
        self.list().ok()?.get(req.name()).map(|versions| {
            versions.iter().rev().find_map(|version| {
                if req.version_req().matches(version) {
                    Some(LuaPackage::new(req.name().clone(), version.clone()))
                } else {
                    None
                }
            })
        })?
    }

    pub fn rock(&self, package: &LuaPackage) -> Result<RockLayout> {
        // TODO(vhyrro): Don't store rocks with the revision number, that should be stripped almost
        // everywhere by default.
        let rock_path = self.root_for(package);

        let etc = rock_path.join("etc");
        let lib = rock_path.join("lib");
        let src = rock_path.join("src");
        let bin = rock_path.join("bin");
        let conf = etc.join("conf");
        let doc = etc.join("doc");

        std::fs::create_dir_all(&etc)?;
        std::fs::create_dir_all(&lib)?;
        std::fs::create_dir_all(&src)?;
        std::fs::create_dir_all(&bin)?;
        std::fs::create_dir_all(&conf)?;
        std::fs::create_dir_all(&doc)?;

        Ok(RockLayout {
            rock_path,
            etc,
            lib,
            src,
            bin,
            conf,
            doc,
        })
    }

    pub fn lockfile(&self) -> Result<Lockfile> {
        Lockfile::new(self.root().join("lock.json"))
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use insta::assert_yaml_snapshot;
    use itertools::Itertools;

    use crate::{
        build::variables::HasVariables as _,
        config::LuaVersion,
        lua_package::{LuaPackage, PackageName, PackageVersion},
        tree::RockLayout,
    };

    use super::Tree;

    #[test]
    fn rock_layout() {
        let tree_path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/test/sample-tree");

        let tree = Tree::new(tree_path.clone(), LuaVersion::Lua51).unwrap();

        let neorg = tree
            .rock(&LuaPackage::parse("neorg".into(), "8.0.0".into()).unwrap())
            .unwrap();

        assert_eq!(
            neorg,
            RockLayout {
                rock_path: tree_path.join("5.1/neorg@8.0.0"),
                etc: tree_path.join("5.1/neorg@8.0.0/etc"),
                lib: tree_path.join("5.1/neorg@8.0.0/lib"),
                src: tree_path.join("5.1/neorg@8.0.0/src"),
                bin: tree_path.join("5.1/neorg@8.0.0/bin"),
                conf: tree_path.join("5.1/neorg@8.0.0/etc/conf"),
                doc: tree_path.join("5.1/neorg@8.0.0/etc/doc"),
            }
        );

        let lua_cjson = tree
            .rock(&LuaPackage::parse("lua-cjson".into(), "2.1.0".into()).unwrap())
            .unwrap();

        assert_eq!(
            lua_cjson,
            RockLayout {
                rock_path: tree_path.join("5.1/lua-cjson@2.1.0"),
                etc: tree_path.join("5.1/lua-cjson@2.1.0/etc"),
                lib: tree_path.join("5.1/lua-cjson@2.1.0/lib"),
                src: tree_path.join("5.1/lua-cjson@2.1.0/src"),
                bin: tree_path.join("5.1/lua-cjson@2.1.0/bin"),
                conf: tree_path.join("5.1/lua-cjson@2.1.0/etc/conf"),
                doc: tree_path.join("5.1/lua-cjson@2.1.0/etc/doc"),
            }
        );
    }

    #[test]
    fn tree_list() {
        let tree_path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/test/sample-tree");

        let tree = Tree::new(tree_path, LuaVersion::Lua51).unwrap();
        let result = tree.list().unwrap();
        // note: sorted_redaction doesn't work because we have a nested Vec
        let sorted_result: Vec<(PackageName, Vec<PackageVersion>)> = result
            .into_iter()
            .sorted()
            .map(|(name, version)| (name, version.into_iter().sorted().collect_vec()))
            .collect_vec();

        assert_yaml_snapshot!(sorted_result)
    }

    #[test]
    fn rock_layout_substiture() {
        let tree_path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/test/sample-tree");

        let tree = Tree::new(tree_path.clone(), LuaVersion::Lua51).unwrap();

        let neorg = tree
            .rock(&LuaPackage::parse("neorg".into(), "8.0.0-1".into()).unwrap())
            .unwrap();
        let build_variables = vec![
            "$(PREFIX)",
            "$(LIBDIR)",
            "$(LUADIR)",
            "$(BINDIR)",
            "$(CONFDIR)",
            "$(DOCDIR)",
            "$(UNRECOGNISED)",
        ];
        let result: Vec<String> = build_variables
            .into_iter()
            .map(|var| neorg.substitute_variables(var.into()))
            .collect();
        assert_eq!(
            result,
            vec![
                neorg.rock_path.to_string_lossy().to_string(),
                neorg.lib.to_string_lossy().to_string(),
                neorg.src.to_string_lossy().to_string(),
                neorg.bin.to_string_lossy().to_string(),
                neorg.conf.to_string_lossy().to_string(),
                neorg.doc.to_string_lossy().to_string(),
                "$(UNRECOGNISED)".into(),
            ]
        );
    }
}
