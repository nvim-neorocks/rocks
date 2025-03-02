use std::collections::{BTreeMap, HashSet};
use std::error::Error;
use std::fmt::Display;
use std::io::{self, Write};
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::{collections::HashMap, fs::File, io::ErrorKind, path::PathBuf};

use itertools::Itertools;
use mlua::{ExternalResult, FromLua, Function, IntoLua, UserData};
use serde::{de, Deserialize, Serialize, Serializer};
use sha2::{Digest, Sha256};
use ssri::Integrity;
use thiserror::Error;
use url::Url;

use crate::package::{
    PackageName, PackageReq, PackageSpec, PackageVersion, PackageVersionReq,
    PackageVersionReqError, RemotePackageTypeFilterSpec,
};
use crate::remote_package_source::RemotePackageSource;
use crate::rockspec::RockBinaries;

#[derive(Copy, Debug, PartialEq, Eq, Hash, Clone, PartialOrd, Ord)]
pub enum PinnedState {
    Unpinned,
    Pinned,
}

impl FromLua for PinnedState {
    fn from_lua(value: mlua::Value, lua: &mlua::Lua) -> mlua::Result<Self> {
        Ok(Self::from(bool::from_lua(value, lua)?))
    }
}

impl IntoLua for PinnedState {
    fn into_lua(self, lua: &mlua::Lua) -> mlua::Result<mlua::Value> {
        self.as_bool().into_lua(lua)
    }
}

impl Default for PinnedState {
    fn default() -> Self {
        Self::Unpinned
    }
}

impl Display for PinnedState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self {
            PinnedState::Unpinned => "unpinned".fmt(f),
            PinnedState::Pinned => "pinned".fmt(f),
        }
    }
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
    pub binaries: RockBinaries,
}

#[derive(Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Clone)]
pub struct LocalPackageId(String);

impl FromLua for LocalPackageId {
    fn from_lua(value: mlua::Value, lua: &mlua::Lua) -> mlua::Result<Self> {
        Ok(Self(String::from_lua(value, lua)?))
    }
}

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

    /// Constructs a package ID from a hashed string.
    ///
    /// # Safety
    ///
    /// Ensure that the hash you are providing to this function
    /// is not malformed and resolves to a valid package ID for the target
    /// tree you are working with.
    pub unsafe fn from_unchecked(str: String) -> Self {
        Self(str)
    }
}

impl Display for LocalPackageId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl mlua::IntoLua for LocalPackageId {
    fn into_lua(self, lua: &mlua::Lua) -> mlua::Result<mlua::Value> {
        self.0.into_lua(lua)
    }
}

impl LocalPackageSpec {
    pub fn new(
        name: &PackageName,
        version: &PackageVersion,
        constraint: LockConstraint,
        dependencies: Vec<LocalPackageId>,
        pinned: &PinnedState,
        binaries: RockBinaries,
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
            binaries,
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

    pub fn binaries(&self) -> Vec<&PathBuf> {
        self.binaries.iter().collect()
    }

    pub fn to_package(&self) -> PackageSpec {
        PackageSpec::new(self.name.clone(), self.version.clone())
    }

    pub fn into_package_req(self) -> PackageReq {
        PackageSpec::new(self.name, self.version).into_package_req()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", tag = "type")]
pub(crate) enum RemotePackageSourceUrl {
    Git {
        url: String,
        #[serde(rename = "ref")]
        checkout_ref: String,
    }, // GitUrl doesn't have all the trait instances we need
    Url {
        #[serde(deserialize_with = "deserialize_url", serialize_with = "serialize_url")]
        url: Url,
    },
    File {
        path: PathBuf,
    },
}

// TODO(vhyrro): Move to `package/local.rs`
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, FromLua)]
pub struct LocalPackage {
    pub(crate) spec: LocalPackageSpec,
    pub(crate) source: RemotePackageSource,
    pub(crate) source_url: Option<RemotePackageSourceUrl>,
    hashes: LocalPackageHashes,
}

impl UserData for LocalPackage {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("id", |_, this, _: ()| Ok(this.id()));
        methods.add_method("name", |_, this, _: ()| Ok(this.name().clone()));
        methods.add_method("version", |_, this, _: ()| Ok(this.version().clone()));
        methods.add_method("pinned", |_, this, _: ()| Ok(this.pinned()));
        methods.add_method("dependencies", |_, this, _: ()| {
            Ok(this.spec.dependencies.clone())
        });
        methods.add_method("constraint", |_, this, _: ()| {
            Ok(this.spec.constraint.clone())
        });
        methods.add_method("hashes", |_, this, _: ()| Ok(this.hashes.clone()));
        methods.add_method("to_package", |_, this, _: ()| Ok(this.to_package()));
        methods.add_method("into_package_req", |_, this, _: ()| {
            Ok(this.clone().into_package_req())
        });
    }
}

impl LocalPackage {
    pub fn into_package_spec(self) -> PackageSpec {
        PackageSpec::new(self.spec.name, self.spec.version)
    }

    pub fn as_package_spec(&self) -> PackageSpec {
        PackageSpec::new(self.spec.name.clone(), self.spec.version.clone())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct LocalPackageIntermediate {
    name: PackageName,
    version: PackageVersion,
    pinned: PinnedState,
    dependencies: Vec<LocalPackageId>,
    constraint: Option<String>,
    binaries: RockBinaries,
    source: RemotePackageSource,
    source_url: Option<RemotePackageSourceUrl>,
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
                value.binaries,
            ),
            source: value.source,
            source_url: value.source_url,
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
            binaries: value.spec.binaries.clone(),
            source: value.source.clone(),
            source_url: value.source_url.clone(),
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

impl LocalPackage {
    pub(crate) fn from(
        package: &PackageSpec,
        constraint: LockConstraint,
        binaries: RockBinaries,
        source: RemotePackageSource,
        source_url: Option<RemotePackageSourceUrl>,
        hashes: LocalPackageHashes,
    ) -> Self {
        Self {
            spec: LocalPackageSpec::new(
                package.name(),
                package.version(),
                constraint,
                Vec::default(),
                &PinnedState::Unpinned,
                binaries,
            ),
            source,
            source_url,
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

    pub fn to_package(&self) -> PackageSpec {
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
        fields.add_field_method_get("id", |_, this| Ok(this.id()));
    }

    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("to_package", |_, this, ()| Ok(this.to_package()));
        methods.add_method("to_package_req", |_, this, ()| {
            Ok(this.clone().into_package_req())
        });
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Hash)]
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

impl mlua::UserData for LocalPackageHashes {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("rockspec", |_, this, ()| Ok(this.rockspec.to_hex().1));
        methods.add_method("source", |_, this, ()| Ok(this.source.to_hex().1));
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum LockConstraint {
    Unconstrained,
    Constrained(PackageVersionReq),
}

impl IntoLua for LockConstraint {
    fn into_lua(self, lua: &mlua::Lua) -> mlua::Result<mlua::Value> {
        match self {
            LockConstraint::Unconstrained => Ok("*".into_lua(lua).unwrap()),
            LockConstraint::Constrained(req) => req.into_lua(lua),
        }
    }
}

impl FromLua for LockConstraint {
    fn from_lua(value: mlua::Value, lua: &mlua::Lua) -> mlua::Result<Self> {
        let str = String::from_lua(value, lua)?;

        match str.as_str() {
            "*" => Ok(LockConstraint::Unconstrained),
            _ => Ok(LockConstraint::Constrained(str.parse().into_lua_err()?)),
        }
    }
}

impl Default for LockConstraint {
    fn default() -> Self {
        Self::Unconstrained
    }
}

impl LockConstraint {
    pub fn to_string_opt(&self) -> Option<String> {
        match self {
            LockConstraint::Unconstrained => None,
            LockConstraint::Constrained(req) => Some(req.to_string()),
        }
    }

    fn matches_version_req(&self, req: &PackageVersionReq) -> bool {
        match self {
            LockConstraint::Unconstrained => req.is_any(),
            LockConstraint::Constrained(package_version_req) => package_version_req == req,
        }
    }
}

impl From<PackageVersionReq> for LockConstraint {
    fn from(value: PackageVersionReq) -> Self {
        if value.is_any() {
            Self::Unconstrained
        } else {
            Self::Constrained(value)
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

pub trait LockfilePermissions {}
#[derive(Clone)]
pub struct ReadOnly;
#[derive(Clone)]
pub struct ReadWrite;

impl LockfilePermissions for ReadOnly {}
impl LockfilePermissions for ReadWrite {}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct LocalPackageLock {
    // NOTE: We cannot directly serialize to a `Sha256` object as they don't implement serde traits.
    // NOTE: We want to retain ordering of rocks and entrypoints when de/serializing.
    rocks: BTreeMap<LocalPackageId, LocalPackage>,
    entrypoints: Vec<LocalPackageId>,
}

impl LocalPackageLock {
    fn get(&self, id: &LocalPackageId) -> Option<&LocalPackage> {
        self.rocks.get(id)
    }

    fn is_empty(&self) -> bool {
        self.entrypoints.is_empty()
    }

    pub(crate) fn rocks(&self) -> &BTreeMap<LocalPackageId, LocalPackage> {
        &self.rocks
    }

    fn list(&self) -> HashMap<PackageName, Vec<LocalPackage>> {
        self.rocks()
            .values()
            .cloned()
            .map(|locked_rock| (locked_rock.name().clone(), locked_rock))
            .into_group_map()
    }

    fn remove(&mut self, target: &LocalPackage) {
        self.remove_by_id(&target.id())
    }

    fn remove_by_id(&mut self, target: &LocalPackageId) {
        self.rocks.remove(target);
        self.entrypoints.retain(|x| x != target);
    }

    pub fn has_entrypoint(&self, req: &PackageReq) -> Option<LocalPackage> {
        self.entrypoints.iter().find_map(|id| {
            let rock = self.get(id).unwrap();

            if rock.name() == req.name() && req.version_req().matches(rock.version()) {
                Some(rock.clone())
            } else {
                None
            }
        })
    }

    pub(crate) fn has_rock(
        &self,
        req: &PackageReq,
        filter: Option<RemotePackageTypeFilterSpec>,
    ) -> Option<LocalPackage> {
        self.list()
            .get(req.name())
            .map(|packages| {
                packages
                    .iter()
                    .filter(|package| match &filter {
                        Some(filter_spec) => match package.source {
                            RemotePackageSource::LuarocksRockspec(_) => filter_spec.rockspec,
                            RemotePackageSource::LuarocksSrcRock(_) => filter_spec.src,
                            RemotePackageSource::LuarocksBinaryRock(_) => filter_spec.binary,
                            RemotePackageSource::RockspecContent(_) => true,
                            #[cfg(test)]
                            RemotePackageSource::Test => unimplemented!(),
                        },
                        None => true,
                    })
                    .rev()
                    .find(|package| req.version_req().matches(package.version()))
            })?
            .cloned()
    }

    fn has_rock_with_equal_constraint(&self, req: &PackageReq) -> Option<LocalPackage> {
        self.list()
            .get(req.name())
            .map(|packages| {
                packages
                    .iter()
                    .rev()
                    .find(|package| package.constraint().matches_version_req(req.version_req()))
            })?
            .cloned()
    }

    /// Synchronise a list of packages with this lock,
    /// producing a report of packages to add and packages to remove,
    /// based on the version constraint.
    ///
    /// NOTE: The reason we produce a report and don't add/remove packages
    /// here is because packages need to be installed in order to be added.
    pub(crate) fn package_sync_spec(&self, packages: &[PackageReq]) -> PackageSyncSpec {
        let entrypoints_to_keep: HashSet<LocalPackage> = self
            .entrypoints
            .iter()
            .map(|id| {
                self.get(id)
                    .expect("entrypoint not found in malformed lockfile.")
            })
            .filter(|local_pkg| {
                packages.iter().any(|req| {
                    local_pkg
                        .constraint()
                        .matches_version_req(req.version_req())
                })
            })
            .cloned()
            .collect();

        let packages_to_keep: HashSet<&LocalPackage> = entrypoints_to_keep
            .iter()
            .flat_map(|local_pkg| self.get_all_dependencies(&local_pkg.id()))
            .collect();

        let to_add = packages
            .iter()
            .filter(|pkg| self.has_rock_with_equal_constraint(pkg).is_none())
            .cloned()
            .collect_vec();

        let to_remove = self
            .rocks()
            .values()
            .filter(|pkg| !packages_to_keep.contains(*pkg))
            .cloned()
            .collect_vec();

        PackageSyncSpec { to_add, to_remove }
    }

    /// Return all dependencies of a package, including itself
    fn get_all_dependencies(&self, id: &LocalPackageId) -> HashSet<&LocalPackage> {
        let mut packages = HashSet::new();
        if let Some(local_pkg) = self.get(id) {
            packages.insert(local_pkg);
            packages.extend(
                local_pkg
                    .dependencies()
                    .iter()
                    .flat_map(|id| self.get_all_dependencies(id)),
            );
        }
        packages
    }
}

/// A lockfile for an install tree
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Lockfile<P: LockfilePermissions> {
    #[serde(skip)]
    filepath: PathBuf,
    #[serde(skip)]
    _marker: PhantomData<P>,
    // TODO: Serialize this directly into a `Version`
    version: String,
    #[serde(flatten)]
    lock: LocalPackageLock,
}

pub enum LocalPackageLockType {
    Regular,
    Test,
    Build,
}

/// A lockfile for a Lua project
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectLockfile<P: LockfilePermissions> {
    #[serde(skip)]
    filepath: PathBuf,
    #[serde(skip)]
    _marker: PhantomData<P>,
    version: String,
    #[serde(default, skip_serializing_if = "LocalPackageLock::is_empty")]
    dependencies: LocalPackageLock,
    #[serde(default, skip_serializing_if = "LocalPackageLock::is_empty")]
    test_dependencies: LocalPackageLock,
    #[serde(default, skip_serializing_if = "LocalPackageLock::is_empty")]
    build_dependencies: LocalPackageLock,
}

#[derive(Error, Debug)]
pub enum LockfileIntegrityError {
    #[error("rockspec integirty mismatch.\nExpected: {expected}\nBut got: {got}")]
    RockspecIntegrityMismatch { expected: Integrity, got: Integrity },
    #[error("source integrity mismatch.\nExpected: {expected}\nBut got: {got}")]
    SourceIntegrityMismatch { expected: Integrity, got: Integrity },
    #[error("package {0} version {1} with pinned state {2} and constraint {3} not found in the lockfile.")]
    PackageNotFound(PackageName, PackageVersion, PinnedState, String),
}

/// A specification for syncing a list of packages with a lockfile
#[derive(Debug, Default)]
pub(crate) struct PackageSyncSpec {
    pub to_add: Vec<PackageReq>,
    pub to_remove: Vec<LocalPackage>,
}

impl UserData for Lockfile<ReadOnly> {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("version", |_, this, _: ()| Ok(this.version().clone()));
        methods.add_method("rocks", |_, this, _: ()| Ok(this.rocks().clone()));
        methods.add_method("get", |_, this, id: LocalPackageId| {
            Ok(this.get(&id).cloned())
        });
        methods.add_method("map_then_flush", |_, this, f: mlua::Function| {
            let lockfile = this.clone().write_guard();
            f.call::<()>(lockfile)?;
            Ok(())
        });
    }
}

impl<P: LockfilePermissions> Lockfile<P> {
    pub fn version(&self) -> &String {
        &self.version
    }

    pub fn rocks(&self) -> &BTreeMap<LocalPackageId, LocalPackage> {
        self.lock.rocks()
    }

    pub fn local_pkg_lock(&self) -> &LocalPackageLock {
        &self.lock
    }

    pub fn get(&self, id: &LocalPackageId) -> Option<&LocalPackage> {
        self.lock.get(id)
    }

    pub(crate) fn list(&self) -> HashMap<PackageName, Vec<LocalPackage>> {
        self.lock.list()
    }

    pub(crate) fn has_rock(
        &self,
        req: &PackageReq,
        filter: Option<RemotePackageTypeFilterSpec>,
    ) -> Option<LocalPackage> {
        self.lock.has_rock(req, filter)
    }

    /// Find all rocks that match the requirement
    pub fn find_rocks(&self, req: &PackageReq) -> Vec<LocalPackageId> {
        match self.list().get(req.name()) {
            Some(packages) => packages
                .iter()
                .rev()
                .filter(|package| req.version_req().matches(package.version()))
                .map(|package| package.id())
                .collect_vec(),
            None => Vec::default(),
        }
    }

    /// Validate the integrity of an installed package with the entry in this lockfile.
    pub(crate) fn validate_integrity(
        &self,
        package: &LocalPackage,
    ) -> Result<(), LockfileIntegrityError> {
        // NOTE: We can't query by ID, because when installing from a lockfile (e.g. during sync),
        // the constraint is always `==`.
        match self.list().get(package.name()) {
            None => Err(integrity_err_not_found(package)),
            Some(rocks) => match rocks
                .iter()
                .find(|rock| rock.version() == package.version())
            {
                None => Err(integrity_err_not_found(package)),
                Some(expected_package) => {
                    if package
                        .hashes
                        .rockspec
                        .matches(&expected_package.hashes.rockspec)
                        .is_none()
                    {
                        return Err(LockfileIntegrityError::RockspecIntegrityMismatch {
                            expected: expected_package.hashes.rockspec.clone(),
                            got: package.hashes.rockspec.clone(),
                        });
                    }
                    if package
                        .hashes
                        .source
                        .matches(&expected_package.hashes.source)
                        .is_none()
                    {
                        return Err(LockfileIntegrityError::SourceIntegrityMismatch {
                            expected: expected_package.hashes.source.clone(),
                            got: package.hashes.source.clone(),
                        });
                    }
                    Ok(())
                }
            },
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        let dependencies = self
            .lock
            .rocks
            .iter()
            .flat_map(|(_, rock)| rock.dependencies())
            .collect_vec();

        self.lock.entrypoints = self
            .lock
            .rocks
            .keys()
            .filter(|id| !dependencies.iter().contains(&id))
            .cloned()
            .collect();

        let content = serde_json::to_string_pretty(&self)?;

        std::fs::write(&self.filepath, content)?;

        Ok(())
    }
}

impl<P: LockfilePermissions> ProjectLockfile<P> {
    pub(crate) fn rocks(
        &self,
        deps: &LocalPackageLockType,
    ) -> &BTreeMap<LocalPackageId, LocalPackage> {
        match deps {
            LocalPackageLockType::Regular => self.dependencies.rocks(),
            LocalPackageLockType::Test => self.test_dependencies.rocks(),
            LocalPackageLockType::Build => self.build_dependencies.rocks(),
        }
    }

    pub(crate) fn get(
        &self,
        id: &LocalPackageId,
        deps: &LocalPackageLockType,
    ) -> Option<&LocalPackage> {
        match deps {
            LocalPackageLockType::Regular => self.dependencies.get(id),
            LocalPackageLockType::Test => self.test_dependencies.get(id),
            LocalPackageLockType::Build => self.build_dependencies.get(id),
        }
    }

    pub(crate) fn package_sync_spec(
        &self,
        packages: &[PackageReq],
        deps: &LocalPackageLockType,
    ) -> PackageSyncSpec {
        match deps {
            LocalPackageLockType::Regular => self.dependencies.package_sync_spec(packages),
            LocalPackageLockType::Test => self.test_dependencies.package_sync_spec(packages),
            LocalPackageLockType::Build => self.build_dependencies.package_sync_spec(packages),
        }
    }

    pub fn local_pkg_lock(&self, deps: &LocalPackageLockType) -> &LocalPackageLock {
        match deps {
            LocalPackageLockType::Regular => &self.dependencies,
            LocalPackageLockType::Test => &self.test_dependencies,
            LocalPackageLockType::Build => &self.build_dependencies,
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        let dependencies = self
            .dependencies
            .rocks
            .iter()
            .flat_map(|(_, rock)| rock.dependencies())
            .collect_vec();

        self.dependencies.entrypoints = self
            .dependencies
            .rocks
            .keys()
            .filter(|id| !dependencies.iter().contains(&id))
            .cloned()
            .collect();

        let test_dependencies = self
            .test_dependencies
            .rocks
            .iter()
            .flat_map(|(_, rock)| rock.dependencies())
            .collect_vec();

        self.test_dependencies.entrypoints = self
            .test_dependencies
            .rocks
            .keys()
            .filter(|id| !test_dependencies.iter().contains(&id))
            .cloned()
            .collect();

        let build_dependencies = self
            .build_dependencies
            .rocks
            .iter()
            .flat_map(|(_, rock)| rock.dependencies())
            .collect_vec();

        self.build_dependencies.entrypoints = self
            .build_dependencies
            .rocks
            .keys()
            .filter(|id| !build_dependencies.iter().contains(&id))
            .cloned()
            .collect();

        let content = serde_json::to_string_pretty(&self)?;

        std::fs::write(&self.filepath, content)?;

        Ok(())
    }
}

impl Lockfile<ReadOnly> {
    pub fn new(filepath: PathBuf) -> io::Result<Lockfile<ReadOnly>> {
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

        let mut new: Lockfile<ReadOnly> =
            serde_json::from_str(&std::fs::read_to_string(&filepath)?)?;

        new.filepath = filepath;

        Ok(new)
    }

    /// Creates a temporary, writeable lockfile which can never flush.
    #[cfg(feature = "lua")]
    pub(crate) fn into_temporary(self) -> Lockfile<ReadWrite> {
        Lockfile::<ReadWrite> {
            _marker: PhantomData,
            filepath: self.filepath,
            version: self.version,
            lock: self.lock,
        }
    }

    /// Creates a temporary, writeable lockfile which can never flush.
    #[cfg(not(feature = "lua"))]
    fn into_temporary(self) -> Lockfile<ReadWrite> {
        Lockfile::<ReadWrite> {
            _marker: PhantomData,
            filepath: self.filepath,
            version: self.version,
            lock: self.lock,
        }
    }

    /// Creates a lockfile guard, flushing the lockfile automatically
    /// once the guard goes out of scope.
    pub fn write_guard(self) -> LockfileGuard {
        LockfileGuard(self.into_temporary())
    }

    /// Converts the current lockfile into a writeable one, executes `cb` and flushes
    /// the lockfile.
    pub fn map_then_flush<T, F, E>(self, cb: F) -> Result<T, E>
    where
        F: FnOnce(&mut Lockfile<ReadWrite>) -> Result<T, E>,
        E: Error,
        E: From<io::Error>,
    {
        let mut writeable_lockfile = self.into_temporary();

        let result = cb(&mut writeable_lockfile)?;

        writeable_lockfile.flush()?;

        Ok(result)
    }

    // TODO: Add this once async closures are stabilized
    // Converts the current lockfile into a writeable one, executes `cb` asynchronously and flushes
    // the lockfile.
    //pub async fn map_then_flush_async<T, F, E, Fut>(self, cb: F) -> Result<T, E>
    //where
    //    F: AsyncFnOnce(&mut Lockfile<ReadWrite>) -> Result<T, E>,
    //    E: Error,
    //    E: From<io::Error>,
    //{
    //    let mut writeable_lockfile = self.into_temporary();
    //
    //    let result = cb(&mut writeable_lockfile).await?;
    //
    //    writeable_lockfile.flush()?;
    //
    //    Ok(result)
    //}
}

impl ProjectLockfile<ReadOnly> {
    pub fn new(filepath: PathBuf) -> io::Result<ProjectLockfile<ReadOnly>> {
        // Ensure that the lockfile exists
        match File::options().create_new(true).write(true).open(&filepath) {
            Ok(mut file) => {
                write!(
                    file,
                    r#"
                        {{
                            "dependencies": {{
                                "entrypoints": [],
                                "rocks": {{}}
                            }},
                            "version": "1.0.0"
                        }}
                    "#
                )?;
            }
            Err(err) if err.kind() == ErrorKind::AlreadyExists => {}
            Err(err) => return Err(err),
        }

        let mut new: ProjectLockfile<ReadOnly> =
            serde_json::from_str(&std::fs::read_to_string(&filepath)?)?;

        new.filepath = filepath;

        Ok(new)
    }

    /// Creates a temporary, writeable project lockfile which can never flush.
    fn into_temporary(self) -> ProjectLockfile<ReadWrite> {
        ProjectLockfile::<ReadWrite> {
            _marker: PhantomData,
            filepath: self.filepath,
            version: self.version,
            dependencies: self.dependencies,
            test_dependencies: self.test_dependencies,
            build_dependencies: self.build_dependencies,
        }
    }

    /// Creates a project lockfile guard, flushing the lockfile automatically
    /// once the guard goes out of scope.
    pub fn write_guard(self) -> ProjectLockfileGuard {
        ProjectLockfileGuard(self.into_temporary())
    }
}

impl Lockfile<ReadWrite> {
    pub fn add(&mut self, rock: &LocalPackage) {
        self.lock.rocks.insert(rock.id(), rock.clone());
    }

    pub fn add_dependency(&mut self, target: &LocalPackage, dependency: &LocalPackage) {
        let target_id = target.id();
        let dependency_id = dependency.id();

        self.lock
            .rocks
            .entry(target_id)
            .and_modify(|rock| rock.spec.dependencies.push(dependency_id));

        // Since rocks entries are mutable, we only add the dependency if it
        // has not already been added.
        if !self.lock.rocks.contains_key(&dependency.id()) {
            self.add(dependency);
        }
    }

    pub(crate) fn remove(&mut self, target: &LocalPackage) {
        self.lock.remove(target)
    }

    pub(crate) fn remove_by_id(&mut self, target: &LocalPackageId) {
        self.lock.remove_by_id(target)
    }

    pub(crate) fn sync(&mut self, lock: &LocalPackageLock) {
        self.lock = lock.clone();
    }

    // TODO: `fn entrypoints() -> Vec<LockedRock>`
}

impl ProjectLockfile<ReadWrite> {
    pub(crate) fn remove(&mut self, target: &LocalPackage, deps: &LocalPackageLockType) {
        match deps {
            LocalPackageLockType::Regular => self.dependencies.remove(target),
            LocalPackageLockType::Test => self.test_dependencies.remove(target),
            LocalPackageLockType::Build => self.build_dependencies.remove(target),
        }
    }

    pub(crate) fn sync(&mut self, lock: &LocalPackageLock, deps: &LocalPackageLockType) {
        match deps {
            LocalPackageLockType::Regular => {
                self.dependencies = lock.clone();
            }
            LocalPackageLockType::Test => {
                self.test_dependencies = lock.clone();
            }
            LocalPackageLockType::Build => {
                self.build_dependencies = lock.clone();
            }
        }
    }
}

pub struct LockfileGuard(Lockfile<ReadWrite>);

pub struct ProjectLockfileGuard(ProjectLockfile<ReadWrite>);

impl UserData for LockfileGuard {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("version", |_, this, _: ()| Ok(this.version().clone()));
        methods.add_method("rocks", |_, this, _: ()| Ok(this.rocks().clone()));
        methods.add_method("get", |_, this, id: LocalPackageId| {
            Ok(this.get(&id).cloned())
        });
        methods.add_method_mut("add", |_, this, package: LocalPackage| {
            this.add(&package);
            Ok(())
        });
        methods.add_method_mut("add_dependency", |_, this, (target, dependency)| {
            this.add_dependency(&target, &dependency);
            Ok(())
        });
    }
}

impl Serialize for LockfileGuard {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl Serialize for ProjectLockfileGuard {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for LockfileGuard {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(LockfileGuard(Lockfile::<ReadWrite>::deserialize(
            deserializer,
        )?))
    }
}

impl<'de> Deserialize<'de> for ProjectLockfileGuard {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(ProjectLockfileGuard(
            ProjectLockfile::<ReadWrite>::deserialize(deserializer)?,
        ))
    }
}

impl Deref for LockfileGuard {
    type Target = Lockfile<ReadWrite>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Deref for ProjectLockfileGuard {
    type Target = ProjectLockfile<ReadWrite>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for LockfileGuard {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl DerefMut for ProjectLockfileGuard {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Drop for LockfileGuard {
    fn drop(&mut self) {
        let _ = self.flush();
    }
}

impl Drop for ProjectLockfileGuard {
    fn drop(&mut self) {
        let _ = self.flush();
    }
}

#[cfg(feature = "lua")]
impl mlua::UserData for Lockfile<ReadWrite> {
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

fn integrity_err_not_found(package: &LocalPackage) -> LockfileIntegrityError {
    LockfileIntegrityError::PackageNotFound(
        package.name().clone(),
        package.version().clone(),
        package.spec.pinned,
        package
            .spec
            .constraint
            .clone()
            .unwrap_or("UNCONSTRAINED".into()),
    )
}

fn deserialize_url<'de, D>(deserializer: D) -> Result<Url, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    Url::parse(&s).map_err(serde::de::Error::custom)
}

fn serialize_url<S>(url: &Url, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    url.as_str().serialize(serializer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs::remove_file, path::PathBuf};

    use assert_fs::fixture::PathCopy;
    use insta::{assert_json_snapshot, sorted_redaction};

    use crate::{config::LuaVersion::Lua51, package::PackageSpec, tree::Tree};

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
        let mut lockfile = tree.lockfile().unwrap().write_guard();

        let test_package = PackageSpec::parse("test1".to_string(), "0.1.0".to_string()).unwrap();
        let test_local_package = LocalPackage::from(
            &test_package,
            crate::lockfile::LockConstraint::Unconstrained,
            RockBinaries::default(),
            RemotePackageSource::Test,
            None,
            mock_hashes.clone(),
        );
        lockfile.add(&test_local_package);

        let test_dep_package =
            PackageSpec::parse("test2".to_string(), "0.1.0".to_string()).unwrap();
        let mut test_local_dep_package = LocalPackage::from(
            &test_dep_package,
            crate::lockfile::LockConstraint::Constrained(">= 1.0.0".parse().unwrap()),
            RockBinaries::default(),
            RemotePackageSource::Test,
            None,
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

        remove_file(temp.join("5.1/lux.lock")).unwrap();

        let tree = Tree::new(temp.to_path_buf(), Lua51).unwrap();

        let _ = tree.lockfile().unwrap().write_guard(); // Try to create the lockfile but don't actually do anything with it.
    }

    fn get_test_lockfile() -> Lockfile<ReadOnly> {
        let sample_tree = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources/test/sample-tree/5.1/lux.lock");
        Lockfile::new(sample_tree).unwrap()
    }

    #[test]
    fn test_sync_spec() {
        let lockfile = get_test_lockfile();
        let packages = vec![
            PackageReq::parse("neorg@8.8.1-1").unwrap(),
            PackageReq::parse("lua-cjson").unwrap(),
            PackageReq::parse("nonexistent").unwrap(),
        ];

        let sync_spec = lockfile.lock.package_sync_spec(&packages);

        assert_eq!(sync_spec.to_add.len(), 1);

        // Should remove:
        // - neorg 8.0.0-1 (older version)
        // - dependencies unique to neorg 8.0.0-1
        assert!(sync_spec
            .to_remove
            .iter()
            .any(|pkg| pkg.name().to_string() == "neorg"
                && pkg.version() == &"8.0.0-1".parse().unwrap()));
        assert!(sync_spec
            .to_remove
            .iter()
            .any(|pkg| pkg.name().to_string() == "lua-utils.nvim"
                && pkg.constraint() == LockConstraint::Unconstrained));
        assert!(sync_spec
            .to_remove
            .iter()
            .any(|pkg| pkg.name().to_string() == "nvim-nio"
                && pkg.constraint() == LockConstraint::Unconstrained));

        // Should keep dependencies of neorg 8.8.1-1
        assert!(!sync_spec
            .to_remove
            .iter()
            .any(|pkg| pkg.name().to_string() == "nvim-nio"
                && pkg.constraint()
                    == LockConstraint::Constrained(">=1.7.0, <1.8.0".parse().unwrap())));
        assert!(!sync_spec
            .to_remove
            .iter()
            .any(|pkg| pkg.name().to_string() == "lua-utils.nvim"
                && pkg.constraint() == LockConstraint::Constrained("=1.0.2".parse().unwrap())));
        assert!(!sync_spec
            .to_remove
            .iter()
            .any(|pkg| pkg.name().to_string() == "plenary.nvim"
                && pkg.constraint() == LockConstraint::Constrained("=0.1.4".parse().unwrap())));
        assert!(!sync_spec
            .to_remove
            .iter()
            .any(|pkg| pkg.name().to_string() == "nui.nvim"
                && pkg.constraint() == LockConstraint::Constrained("=0.3.0".parse().unwrap())));
        assert!(!sync_spec
            .to_remove
            .iter()
            .any(|pkg| pkg.name().to_string() == "pathlib.nvim"
                && pkg.constraint()
                    == LockConstraint::Constrained(">=2.2.0, <2.3.0".parse().unwrap())));
    }

    #[test]
    fn test_sync_spec_empty() {
        let lockfile = get_test_lockfile();
        let packages = vec![];
        let sync_spec = lockfile.lock.package_sync_spec(&packages);

        // Should remove all packages
        assert!(sync_spec.to_add.is_empty());
        assert_eq!(sync_spec.to_remove.len(), lockfile.rocks().len());
    }

    #[test]
    fn test_sync_spec_different_constraints() {
        let lockfile = get_test_lockfile();
        let packages = vec![PackageReq::parse("nvim-nio>=2.0.0").unwrap()];
        let sync_spec = lockfile.lock.package_sync_spec(&packages);

        let expected: PackageVersionReq = ">=2.0.0".parse().unwrap();
        assert!(sync_spec
            .to_add
            .iter()
            .any(|req| req.name().to_string() == "nvim-nio" && req.version_req() == &expected));

        assert!(sync_spec
            .to_remove
            .iter()
            .any(|pkg| pkg.name().to_string() == "nvim-nio"));
    }
}
