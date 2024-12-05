use crate::{
    build::variables::{self, HasVariables},
    config::LuaVersion,
    lockfile::{LocalPackage, Lockfile},
    package::PackageReq,
};
use std::{io, path::PathBuf};

#[cfg(feature = "lua")]
use mlua::ExternalResult as _;

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
    /// This points to a global binary path at the root of the current tree by default.
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

#[cfg(feature = "lua")]
impl mlua::UserData for RockLayout {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("rock_path", |_, this| Ok(this.rock_path.clone()));
        fields.add_field_method_get("etc", |_, this| Ok(this.etc.clone()));
        fields.add_field_method_get("lib", |_, this| Ok(this.lib.clone()));
        fields.add_field_method_get("src", |_, this| Ok(this.src.clone()));
        fields.add_field_method_get("bin", |_, this| Ok(this.bin.clone()));
        fields.add_field_method_get("conf", |_, this| Ok(this.conf.clone()));
        fields.add_field_method_get("doc", |_, this| Ok(this.doc.clone()));
    }
}

impl Tree {
    pub fn new(root: PathBuf, version: LuaVersion) -> io::Result<Self> {
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

    pub fn root_for(&self, package: &LocalPackage) -> PathBuf {
        self.root().join(format!(
            "{}-{}@{}",
            package.id(),
            package.name(),
            package.version()
        ))
    }

    pub fn bin(&self) -> PathBuf {
        self.root.join("bin")
    }

    pub fn has_rock(&self, req: &PackageReq) -> Option<LocalPackage> {
        self.list()
            .ok()?
            .get(req.name())
            .map(|packages| {
                packages
                    .iter()
                    .rev()
                    .find(|package| req.version_req().matches(package.version()))
            })?
            .cloned()
    }

    pub fn has_rock_and<F>(&self, req: &PackageReq, filter: F) -> Option<LocalPackage>
    where
        F: Fn(&LocalPackage) -> bool,
    {
        self.list()
            .ok()?
            .get(req.name())
            .map(|packages| {
                packages
                    .iter()
                    .rev()
                    .find(|package| req.version_req().matches(package.version()) && filter(package))
            })?
            .cloned()
    }

    /// Create a `RockLayout` for a package, without creating the directories.
    pub fn rock_layout(&self, package: &LocalPackage) -> RockLayout {
        let rock_path = self.root_for(package);
        let bin = self.bin();
        let etc = rock_path.join("etc");
        let lib = rock_path.join("lib");
        let src = rock_path.join("src");
        let conf = etc.join("conf");
        let doc = etc.join("doc");

        RockLayout {
            rock_path,
            etc,
            lib,
            src,
            bin,
            conf,
            doc,
        }
    }

    /// Create a `RockLayout` for a package, creating the directories.
    pub fn rock(&self, package: &LocalPackage) -> io::Result<RockLayout> {
        let rock_layout = self.rock_layout(package);
        std::fs::create_dir_all(&rock_layout.etc)?;
        std::fs::create_dir_all(&rock_layout.lib)?;
        std::fs::create_dir_all(&rock_layout.src)?;
        std::fs::create_dir_all(&rock_layout.conf)?;
        std::fs::create_dir_all(&rock_layout.doc)?;
        Ok(rock_layout)
    }

    pub fn lockfile(&self) -> io::Result<Lockfile> {
        Lockfile::new(self.root().join("lock.json"))
    }
}

#[cfg(feature = "lua")]
impl mlua::UserData for Tree {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("root", |_, this, ()| Ok(this.root()));
        methods.add_method("root_for", |_, this, package: LocalPackage| {
            Ok(this.root_for(&package))
        });
        methods.add_method("bin", |_, this, ()| Ok(this.bin()));
        methods.add_method("has_rock", |_, this, req: PackageReq| {
            Ok(this.has_rock(&req))
        });
        methods.add_method(
            "has_rock_and",
            |_, this, (req, callback): (PackageReq, mlua::Function)| {
                Ok(this.has_rock_and(&req, |package| {
                    callback
                        .call(package.clone())
                        .expect("failed to invoke Lua callback in `Tree::has_rock_and()`")
                }))
            },
        );
        methods.add_method("rock_layout", |_, this, package: LocalPackage| {
            Ok(this.rock_layout(&package))
        });
        methods.add_method("rock", |_, this, package: LocalPackage| {
            this.rock(&package).into_lua_err()
        });
        methods.add_method("lockfile", |_, this, ()| this.lockfile().into_lua_err());
    }
}

#[cfg(test)]
mod tests {
    use assert_fs::prelude::PathCopy as _;
    use itertools::Itertools;
    use std::path::PathBuf;

    use insta::assert_yaml_snapshot;

    use crate::{
        build::variables::HasVariables as _,
        config::LuaVersion,
        lockfile::{LocalPackage, LocalPackageHashes, LockConstraint},
        package::{PackageName, PackageVersion, RemotePackage},
        tree::RockLayout,
    };

    use super::Tree;

    #[test]
    fn rock_layout() {
        let tree_path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/test/sample-tree");

        let temp = assert_fs::TempDir::new().unwrap();
        temp.copy_from(&tree_path, &["**"]).unwrap();
        let tree_path = temp.to_path_buf();

        let tree = Tree::new(tree_path.clone(), LuaVersion::Lua51).unwrap();

        let mock_hashes = LocalPackageHashes {
            rockspec: "sha256-uU0nuZNNPgilLlLX2n2r+sSE7+N6U4DukIj3rOLvzek="
                .parse()
                .unwrap(),
            source: "sha256-uU0nuZNNPgilLlLX2n2r+sSE7+N6U4DukIj3rOLvzek="
                .parse()
                .unwrap(),
        };

        let package = LocalPackage::from(
            &RemotePackage::parse("neorg".into(), "8.0.0-1".into()).unwrap(),
            LockConstraint::Unconstrained,
            mock_hashes.clone(),
        );

        let id = package.id();

        let neorg = tree.rock(&package).unwrap();

        assert_eq!(
            neorg,
            RockLayout {
                bin: tree_path.join("bin"),
                rock_path: tree_path.join(format!("5.1/{id}-neorg@8.0.0-1")),
                etc: tree_path.join(format!("5.1/{id}-neorg@8.0.0-1/etc")),
                lib: tree_path.join(format!("5.1/{id}-neorg@8.0.0-1/lib")),
                src: tree_path.join(format!("5.1/{id}-neorg@8.0.0-1/src")),
                conf: tree_path.join(format!("5.1/{id}-neorg@8.0.0-1/etc/conf")),
                doc: tree_path.join(format!("5.1/{id}-neorg@8.0.0-1/etc/doc")),
            }
        );

        let package = LocalPackage::from(
            &RemotePackage::parse("lua-cjson".into(), "2.1.0-1".into()).unwrap(),
            LockConstraint::Unconstrained,
            mock_hashes.clone(),
        );

        let id = package.id();

        let lua_cjson = tree.rock(&package).unwrap();

        assert_eq!(
            lua_cjson,
            RockLayout {
                bin: tree_path.join("bin"),
                rock_path: tree_path.join(format!("5.1/{id}-lua-cjson@2.1.0-1")),
                etc: tree_path.join(format!("5.1/{id}-lua-cjson@2.1.0-1/etc")),
                lib: tree_path.join(format!("5.1/{id}-lua-cjson@2.1.0-1/lib")),
                src: tree_path.join(format!("5.1/{id}-lua-cjson@2.1.0-1/src")),
                conf: tree_path.join(format!("5.1/{id}-lua-cjson@2.1.0-1/etc/conf")),
                doc: tree_path.join(format!("5.1/{id}-lua-cjson@2.1.0-1/etc/doc")),
            }
        );
    }

    #[test]
    fn tree_list() {
        let tree_path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/test/sample-tree");

        let temp = assert_fs::TempDir::new().unwrap();
        temp.copy_from(&tree_path, &["**"]).unwrap();
        let tree_path = temp.to_path_buf();

        let tree = Tree::new(tree_path, LuaVersion::Lua51).unwrap();
        let result = tree.list().unwrap();
        // note: sorted_redaction doesn't work because we have a nested Vec
        let sorted_result: Vec<(PackageName, Vec<PackageVersion>)> = result
            .into_iter()
            .sorted()
            .map(|(name, package)| {
                (
                    name,
                    package
                        .into_iter()
                        .map(|package| package.spec.version)
                        .sorted()
                        .collect_vec(),
                )
            })
            .collect_vec();

        assert_yaml_snapshot!(sorted_result)
    }

    #[test]
    fn rock_layout_substiture() {
        let tree_path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/test/sample-tree");

        let temp = assert_fs::TempDir::new().unwrap();
        temp.copy_from(&tree_path, &["**"]).unwrap();
        let tree_path = temp.to_path_buf();

        let tree = Tree::new(tree_path.clone(), LuaVersion::Lua51).unwrap();

        let mock_hashes = LocalPackageHashes {
            rockspec: "sha256-uU0nuZNNPgilLlLX2n2r+sSE7+N6U4DukIj3rOLvzek="
                .parse()
                .unwrap(),
            source: "sha256-uU0nuZNNPgilLlLX2n2r+sSE7+N6U4DukIj3rOLvzek="
                .parse()
                .unwrap(),
        };

        let neorg = tree
            .rock(&LocalPackage::from(
                &RemotePackage::parse("neorg".into(), "8.0.0-1-1".into()).unwrap(),
                LockConstraint::Unconstrained,
                mock_hashes.clone(),
            ))
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
