use std::fmt::Display;
use std::io::{self, Write};
use std::{collections::HashMap, fs::File, io::ErrorKind, path::PathBuf};

use itertools::Itertools;
use serde::{de, Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ssri::Integrity;
use thiserror::Error;

use crate::package::{
    PackageName, PackageReq, PackageVersion, PackageVersionReq, PackageVersionReqError,
    RemotePackage,
};

#[cfg(feature = "lua")]
use mlua::{ExternalResult as _, FromLua};

#[derive(Copy, Debug, PartialEq, Eq, Hash, Clone, PartialOrd, Ord)]
pub enum PinnedState {
    Unpinned,
    Pinned,
}

impl From<bool> for PinnedState {
    fn from(value: bool) -> Self {
        if value {
            Self::Pinned
        } else {
            Self::Unpinned
        }
    }
}

impl PinnedState {
    pub fn as_bool(&self) -> bool {
        match self {
            Self::Unpinned => false,
            Self::Pinned => true,
        }
    }
}

impl Serialize for PinnedState {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_bool(self.as_bool())
    }
}

impl<'de> Deserialize<'de> for PinnedState {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(match bool::deserialize(deserializer)? {
            false => Self::Unpinned,
            true => Self::Pinned,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct LocalPackageSpec {
    pub name: PackageName,
    pub version: PackageVersion,
    pub pinned: PinnedState,
    pub dependencies: Vec<LocalPackageId>,
    // TODO: Deserialize this directly into a `LuaPackageReq`
    pub constraint: Option<String>,
}

#[derive(Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Clone)]
pub struct LocalPackageId(String);

impl LocalPackageId {
    pub fn new(
        name: &PackageName,
        version: &PackageVersion,
        pinned: PinnedState,
        constraint: LockConstraint,
    ) -> Self {
        let mut hasher = Sha256::new();

        hasher.update(format!(
            "{}{}{}{}",
            name,
            version,
            pinned.as_bool(),
            match constraint {
                LockConstraint::Unconstrained => String::default(),
                LockConstraint::Constrained(version_req) => version_req.to_string(),
            },
        ));

        Self(hex::encode(hasher.finalize()))
    }
}

impl Display for LocalPackageId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl LocalPackageSpec {
    pub fn new(
        name: &PackageName,
        version: &PackageVersion,
        constraint: LockConstraint,
        dependencies: Vec<LocalPackageId>,
        pinned: &PinnedState,
    ) -> Self {
        Self {
            name: name.clone(),
            version: version.clone(),
            pinned: *pinned,
            dependencies,
            constraint: match constraint {
                LockConstraint::Unconstrained => None,
                LockConstraint::Constrained(version_req) => Some(version_req.to_string()),
            },
        }
    }

    pub fn id(&self) -> LocalPackageId {
        LocalPackageId::new(
            self.name(),
            self.version(),
            self.pinned,
            match &self.constraint {
                None => LockConstraint::Unconstrained,
                Some(constraint) => LockConstraint::Constrained(constraint.parse().unwrap()),
            },
        )
    }

    pub fn constraint(&self) -> LockConstraint {
        // Safe to unwrap as the data can only end up in the struct as a valid constraint
        LockConstraint::try_from(&self.constraint).unwrap()
    }

    pub fn name(&self) -> &PackageName {
        &self.name
    }

    pub fn version(&self) -> &PackageVersion {
        &self.version
    }

    pub fn pinned(&self) -> PinnedState {
        self.pinned
    }

    pub fn dependencies(&self) -> Vec<&LocalPackageId> {
        self.dependencies.iter().collect()
    }

    pub fn to_package(&self) -> RemotePackage {
        RemotePackage::new(self.name.clone(), self.version.clone())
    }

    pub fn into_package_req(self) -> PackageReq {
        RemotePackage::new(self.name, self.version).into_package_req()
    }
}

// TODO(vhyrro): Move to `package/local.rs`
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct LocalPackage {
    pub(crate) spec: LocalPackageSpec,
    hashes: LocalPackageHashes,
}

#[cfg_attr(feature = "lua", derive(FromLua,))]
#[derive(Debug, Serialize, Deserialize, Clone)]
struct LocalPackageIntermediate {
    name: PackageName,
    version: PackageVersion,
    pinned: PinnedState,
    dependencies: Vec<LocalPackageId>,
    constraint: Option<String>,
    hashes: LocalPackageHashes,
}

impl TryFrom<LocalPackageIntermediate> for LocalPackage {
    type Error = LockConstraintParseError;

    fn try_from(value: LocalPackageIntermediate) -> Result<Self, Self::Error> {
        let constraint = LockConstraint::try_from(&value.constraint)?;
        Ok(Self {
            spec: LocalPackageSpec::new(
                &value.name,
                &value.version,
                constraint,
                value.dependencies,
                &value.pinned,
            ),
            hashes: value.hashes,
        })
    }
}

impl From<&LocalPackage> for LocalPackageIntermediate {
    fn from(value: &LocalPackage) -> Self {
        Self {
            name: value.spec.name.clone(),
            version: value.spec.version.clone(),
            pinned: value.spec.pinned,
            dependencies: value.spec.dependencies.clone(),
            constraint: value.spec.constraint.clone(),
            hashes: value.hashes.clone(),
        }
    }
}

impl<'de> Deserialize<'de> for LocalPackage {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        LocalPackage::try_from(LocalPackageIntermediate::deserialize(deserializer)?)
            .map_err(de::Error::custom)
    }
}

impl Serialize for LocalPackage {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        LocalPackageIntermediate::from(self).serialize(serializer)
    }
}

#[cfg(feature = "lua")]
impl FromLua for LocalPackage {
    fn from_lua(value: mlua::Value, lua: &mlua::Lua) -> mlua::Result<Self> {
        LocalPackage::try_from(LocalPackageIntermediate::from_lua(value, lua)?)
            .map_err(|err| mlua::Error::DeserializeError(format!("{}", err)))
    }
}

impl LocalPackage {
    pub fn from(
        package: &RemotePackage,
        constraint: LockConstraint,
        hashes: LocalPackageHashes,
    ) -> Self {
        Self {
            spec: LocalPackageSpec::new(
                package.name(),
                package.version(),
                constraint,
                Vec::default(),
                &PinnedState::Unpinned,
            ),
            hashes,
        }
    }

    pub fn id(&self) -> LocalPackageId {
        self.spec.id()
    }

    pub fn name(&self) -> &PackageName {
        self.spec.name()
    }

    pub fn version(&self) -> &PackageVersion {
        self.spec.version()
    }

    pub fn pinned(&self) -> PinnedState {
        self.spec.pinned()
    }

    pub fn dependencies(&self) -> Vec<&LocalPackageId> {
        self.spec.dependencies()
    }

    pub fn constraint(&self) -> LockConstraint {
        self.spec.constraint()
    }

    pub fn hashes(&self) -> &LocalPackageHashes {
        &self.hashes
    }

    pub fn to_package(&self) -> RemotePackage {
        self.spec.to_package()
    }

    pub fn into_package_req(self) -> PackageReq {
        self.spec.into_package_req()
    }
}

#[cfg(feature = "lua")]
impl mlua::UserData for LocalPackage {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("name", |_, this| Ok(this.name().to_string()));
        fields.add_field_method_get("version", |_, this| Ok(this.version().to_string()));
        fields.add_field_method_get("pinned", |_, this| Ok(this.pinned().as_bool()));
        fields.add_field_method_get("dependencies", |_, this| {
            Ok(this
                .spec
                .dependencies
                .iter()
                .map(|id| id.clone().0)
                .collect_vec())
        });
        fields.add_field_method_get("constraint", |_, this| Ok(this.spec.constraint.clone()));
        fields.add_field_method_get("id", |_, this| Ok(this.id().0));
    }

    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("to_package", |_, this, ()| Ok(this.to_package()));
        methods.add_method("to_package_req", |_, this, ()| {
            Ok(this.clone().into_package_req())
        });
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct LocalPackageHashes {
    pub rockspec: Integrity,
    pub source: Integrity,
}

impl Ord for LocalPackageHashes {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let a = (self.rockspec.to_hex().1, self.source.to_hex().1);
        let b = (other.rockspec.to_hex().1, other.source.to_hex().1);
        a.cmp(&b)
    }
}

impl PartialOrd for LocalPackageHashes {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(feature = "lua")]
impl mlua::UserData for LocalPackageHashes {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("rockspec", |_, this| Ok(this.rockspec.to_string()));
        fields.add_field_method_get("source", |_, this| Ok(this.source.to_string()));
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum LockConstraint {
    Unconstrained,
    Constrained(PackageVersionReq),
}

impl LockConstraint {
    pub fn to_string_opt(&self) -> Option<String> {
        match self {
            LockConstraint::Unconstrained => None,
            LockConstraint::Constrained(req) => Some(req.to_string()),
        }
    }
}

#[derive(Error, Debug)]
pub enum LockConstraintParseError {
    #[error("Invalid constraint in LuaPackage: {0}")]
    LockConstraintParseError(#[from] PackageVersionReqError),
}

impl TryFrom<&Option<String>> for LockConstraint {
    type Error = LockConstraintParseError;

    fn try_from(constraint: &Option<String>) -> Result<Self, Self::Error> {
        match constraint {
            Some(constraint) => {
                let package_version_req = constraint.parse()?;
                Ok(LockConstraint::Constrained(package_version_req))
            }
            None => Ok(LockConstraint::Unconstrained),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Lockfile {
    #[serde(skip)]
    filepath: PathBuf,
    // TODO: Serialize this directly into a `Version`
    version: String,
    // NOTE: We cannot directly serialize to a `Sha256` object as they don't implement serde traits.
    rocks: HashMap<LocalPackageId, LocalPackage>,
    entrypoints: Vec<LocalPackageId>,
}

impl Lockfile {
    pub fn new(filepath: PathBuf) -> io::Result<Self> {
        // Ensure that the lockfile exists
        match File::options().create_new(true).write(true).open(&filepath) {
            Ok(mut file) => {
                write!(
                    file,
                    r#"
                        {{
                            "entrypoints": [],
                            "rocks": {{}},
                            "version": "1.0.0"
                        }}
                    "#
                )?;
            }
            Err(err) if err.kind() == ErrorKind::AlreadyExists => {}
            Err(err) => return Err(err),
        }

        let mut new: Lockfile = serde_json::from_str(&std::fs::read_to_string(&filepath)?)?;

        new.filepath = filepath;

        Ok(new)
    }

    pub fn add(&mut self, rock: &LocalPackage) {
        self.rocks.insert(rock.id(), rock.clone());
    }

    pub fn add_dependency(&mut self, target: &LocalPackage, dependency: &LocalPackage) {
        let target_id = target.id();
        let dependency_id = dependency.id();

        self.rocks
            .entry(target_id)
            .and_modify(|rock| rock.spec.dependencies.push(dependency_id));

        // Since rocks entries are mutable, we only add the dependency if it
        // has not already been added.
        if !self.rocks.contains_key(&dependency.id()) {
            self.add(dependency);
        }
    }

    pub fn remove(&mut self, target: &LocalPackage) {
        self.rocks.remove(&target.id());
    }

    pub fn version(&self) -> &String {
        &self.version
    }

    pub fn rocks(&self) -> &HashMap<LocalPackageId, LocalPackage> {
        &self.rocks
    }

    pub fn get(&self, id: &LocalPackageId) -> Option<&LocalPackage> {
        self.rocks.get(id)
    }

    pub fn get_mut(&mut self, id: &LocalPackageId) -> Option<&mut LocalPackage> {
        self.rocks.get_mut(id)
    }

    // TODO: `fn entrypoints() -> Vec<LockedRock>`

    pub fn flush(&mut self) -> io::Result<()> {
        let dependencies = self
            .rocks
            .iter()
            .flat_map(|(_, rock)| rock.dependencies())
            .collect_vec();

        self.entrypoints = self
            .rocks
            .keys()
            .filter(|id| !dependencies.iter().contains(&id))
            .cloned()
            .collect();

        let content = serde_json::to_string(self)?;

        std::fs::write(&self.filepath, content)?;

        Ok(())
    }
}

impl Drop for Lockfile {
    fn drop(&mut self) {
        let _ = self.flush();
    }
}

#[cfg(feature = "lua")]
impl mlua::UserData for Lockfile {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method_mut("add", |_, this, package: LocalPackage| {
            this.add(&package);
            Ok(())
        });
        methods.add_method_mut(
            "add_dependency",
            |_, this, (target, dependency): (LocalPackage, LocalPackage)| {
                this.add_dependency(&target, &dependency);
                Ok(())
            },
        );
        methods.add_method_mut("remove", |_, this, target: LocalPackage| {
            this.remove(&target);
            Ok(())
        });

        methods.add_method("version", |_, this, ()| Ok(this.version().to_owned()));
        methods.add_method("rocks", |_, this, ()| {
            Ok(this
                .rocks()
                .iter()
                .map(|(id, rock)| (id.0.clone(), rock.clone()))
                .collect::<HashMap<String, LocalPackage>>())
        });

        methods.add_method("get", |_, this, id: String| {
            Ok(this.get(&LocalPackageId(id)).cloned())
        });
        methods.add_method_mut("flush", |_, this, ()| this.flush().into_lua_err());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs::remove_file, path::PathBuf};

    use assert_fs::fixture::PathCopy;
    use insta::{assert_json_snapshot, sorted_redaction};

    use crate::{config::LuaVersion::Lua51, package::RemotePackage, tree::Tree};

    #[test]
    fn parse_lockfile() {
        let temp = assert_fs::TempDir::new().unwrap();
        temp.copy_from(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/test/sample-tree"),
            &["**"],
        )
        .unwrap();

        let tree = Tree::new(temp.to_path_buf(), Lua51).unwrap();
        let lockfile = tree.lockfile().unwrap();

        assert_json_snapshot!(lockfile, { ".**" => sorted_redaction() });
    }

    #[test]
    fn add_rocks() {
        let temp = assert_fs::TempDir::new().unwrap();
        temp.copy_from(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/test/sample-tree"),
            &["**"],
        )
        .unwrap();

        let mock_hashes = LocalPackageHashes {
            rockspec: "sha256-uU0nuZNNPgilLlLX2n2r+sSE7+N6U4DukIj3rOLvzek="
                .parse()
                .unwrap(),
            source: "sha256-uU0nuZNNPgilLlLX2n2r+sSE7+N6U4DukIj3rOLvzek="
                .parse()
                .unwrap(),
        };

        let tree = Tree::new(temp.to_path_buf(), Lua51).unwrap();
        let mut lockfile = tree.lockfile().unwrap();

        let test_package = RemotePackage::parse("test1".to_string(), "0.1.0".to_string()).unwrap();
        let test_local_package = LocalPackage::from(
            &test_package,
            crate::lockfile::LockConstraint::Unconstrained,
            mock_hashes.clone(),
        );
        lockfile.add(&test_local_package);

        let test_dep_package =
            RemotePackage::parse("test2".to_string(), "0.1.0".to_string()).unwrap();
        let mut test_local_dep_package = LocalPackage::from(
            &test_dep_package,
            crate::lockfile::LockConstraint::Constrained(">= 1.0.0".parse().unwrap()),
            mock_hashes.clone(),
        );
        test_local_dep_package.spec.pinned = PinnedState::Pinned;
        lockfile.add(&test_local_dep_package);

        lockfile.add_dependency(&test_local_package, &test_local_dep_package);

        assert_json_snapshot!(lockfile, { ".**" => sorted_redaction() });
    }

    #[test]
    fn parse_nonexistent_lockfile() {
        let tree_path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/test/sample-tree");

        let temp = assert_fs::TempDir::new().unwrap();
        temp.copy_from(&tree_path, &["**"]).unwrap();

        remove_file(temp.join("5.1/lock.json")).unwrap();

        let tree = Tree::new(temp.to_path_buf(), Lua51).unwrap();

        tree.lockfile().unwrap(); // Try to create the lockfile but don't actually do anything with it.
    }
}
