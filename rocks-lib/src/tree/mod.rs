use crate::{
    build::{
        utils::escape_path,
        variables::{self, HasVariables},
    },
    config::LuaVersion,
    lockfile::{LocalPackage, LocalPackageId, Lockfile},
    package::PackageReq,
};
use std::{io, path::PathBuf};

use itertools::Itertools;
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
    fn substitute_variables(&self, input: &str) -> String {
        variables::substitute(
            |var| {
                let path = match var {
                    "PREFIX" => Some(escape_path(&self.rock_path)),
                    "LIBDIR" => Some(escape_path(&self.lib)),
                    "LUADIR" => Some(escape_path(&self.src)),
                    "BINDIR" => Some(escape_path(&self.bin)),
                    "CONFDIR" => Some(escape_path(&self.conf)),
                    "DOCDIR" => Some(escape_path(&self.doc)),
                    _ => None,
                }?;
                Some(path)
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

    pub fn match_rocks(&self, req: &PackageReq) -> io::Result<RockMatches> {
        match self.list()?.get(req.name()) {
            Some(packages) => {
                let mut found_packages = packages
                    .iter()
                    .rev()
                    .filter(|package| req.version_req().matches(package.version()))
                    .map(|package| package.id())
                    .collect_vec();

                Ok(match found_packages.len() {
                    0 => RockMatches::NotFound(req.clone()),
                    1 => RockMatches::Single(found_packages.pop().unwrap()),
                    2.. => RockMatches::Many(found_packages),
                })
            }
            None => Ok(RockMatches::NotFound(req.clone())),
        }
    }

    pub fn match_rocks_and<F>(&self, req: &PackageReq, filter: F) -> io::Result<RockMatches>
    where
        F: Fn(&LocalPackage) -> bool,
    {
        match self.list()?.get(req.name()) {
            Some(packages) => {
                let mut found_packages = packages
                    .iter()
                    .rev()
                    .filter(|package| {
                        req.version_req().matches(package.version()) && filter(package)
                    })
                    .map(|package| package.id())
                    .collect_vec();

                Ok(match found_packages.len() {
                    0 => RockMatches::NotFound(req.clone()),
                    1 => RockMatches::Single(found_packages.pop().unwrap()),
                    2.. => RockMatches::Many(found_packages),
                })
            }
            None => Ok(RockMatches::NotFound(req.clone())),
        }
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

    /// Get this tree's lockfile path.
    pub fn lockfile_path(&self) -> PathBuf {
        self.root().join("lock.json")
    }

    pub fn lockfile(&self) -> io::Result<Lockfile> {
        Lockfile::new(self.lockfile_path())
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
        methods.add_method("match_rocks", |_, this, req: PackageReq| {
            this.match_rocks(&req).map_err(|err| {
                mlua::Error::RuntimeError(format!("IO error while calling 'match_rocks': {}", err))
            })
        });
        methods.add_method(
            "match_rock_and",
            |_, this, (req, callback): (PackageReq, mlua::Function)| {
                this.match_rocks_and(&req, |package| {
                    callback
                        .call(package.clone())
                        .expect("failed to invoke Lua callback in `Tree::match_rock_and()`")
                })
                .map_err(|err| {
                    mlua::Error::RuntimeError(format!(
                        "IO error while calling 'match_rocks_and': {}",
                        err
                    ))
                })
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

#[derive(Clone, Debug)]
pub enum RockMatches {
    NotFound(PackageReq),
    Single(LocalPackageId),
    Many(Vec<LocalPackageId>),
}

// Loosely mimic the Option<T> functions.
impl RockMatches {
    pub fn is_found(&self) -> bool {
        matches!(self, Self::Single(_) | Self::Many(_))
    }

    #[cfg(feature = "lua")]
    fn lua_type(&self) -> String {
        match self {
            Self::NotFound(_) => "not_found",
            Self::Single(_) => "local_package",
            Self::Many(_) => "many",
        }
        .into()
    }

    #[cfg(feature = "lua")]
    fn lua_type_error(&self, expected: &str) -> mlua::Error {
        mlua::Error::RuntimeError(format!(
            "attempted to query field '{}' for type '{}'",
            expected,
            self.lua_type()
        ))
    }
}

#[cfg(feature = "lua")]
impl mlua::UserData for RockMatches {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("type", |_, this| Ok(this.lua_type()));
        fields.add_field_method_get("not_found", |_, this| {
            if let Self::NotFound(package_req) = this {
                Ok(package_req.to_string())
            } else {
                Err(this.lua_type_error("not_found"))
            }
        });
        fields.add_field_method_get("many", |_, this| {
            if let Self::Many(packages) = this {
                Ok(packages.clone())
            } else {
                Err(this.lua_type_error("many"))
            }
        });
        fields.add_field_method_get("single", |_, this| {
            if let Self::Single(package) = this {
                Ok(package.clone())
            } else {
                Err(this.lua_type_error("single"))
            }
        });
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
        package::{PackageName, PackageSpec, PackageVersion},
        remote_package_source::RemotePackageSource,
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
            &PackageSpec::parse("neorg".into(), "8.0.0-1".into()).unwrap(),
            LockConstraint::Unconstrained,
            RemotePackageSource::Test,
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
            &PackageSpec::parse("lua-cjson".into(), "2.1.0-1".into()).unwrap(),
            LockConstraint::Unconstrained,
            RemotePackageSource::Test,
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
    fn rock_layout_substitute() {
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
                &PackageSpec::parse("neorg".into(), "8.0.0-1-1".into()).unwrap(),
                LockConstraint::Unconstrained,
                RemotePackageSource::Test,
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
            .map(|var| neorg.substitute_variables(var))
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
