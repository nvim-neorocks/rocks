use std::collections::HashSet;
use std::error::Error;
use std::fmt::Display;
use std::io::{self, Write};
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::{collections::HashMap, fs::File, io::ErrorKind, path::PathBuf};

use itertools::Itertools;
use serde::{de, Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ssri::Integrity;
use thiserror::Error;

use crate::package::{
    PackageName, PackageReq, PackageSpec, PackageVersion, PackageVersionReq,
    PackageVersionReqError, RemotePackageTypeFilterSpec,
};
use crate::remote_package_source::RemotePackageSource;
use crate::rockspec::RockBinaries;

#[cfg(feature = "lua")]
use mlua::{ExternalResult, FromLua};

#[derive(Copy, Debug, PartialEq, Eq, Hash, Clone, PartialOrd, Ord)]
pub enum PinnedState {
    Unpinned,
    Pinned,
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

#[cfg(feature = "lua")]
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

// TODO(vhyrro): Move to `package/local.rs`
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LocalPackage {
    pub(crate) spec: LocalPackageSpec,
    pub(crate) source: RemotePackageSource,
    hashes: LocalPackageHashes,
}

impl LocalPackage {
    pub fn into_package_spec(self) -> PackageSpec {
        PackageSpec::new(self.spec.name, self.spec.version)
    }

    pub fn as_package_spec(&self) -> PackageSpec {
        PackageSpec::new(self.spec.name.clone(), self.spec.version.clone())
    }
}

#[cfg_attr(feature = "lua", derive(FromLua,))]
#[derive(Debug, Serialize, Deserialize, Clone)]
struct LocalPackageIntermediate {
    name: PackageName,
    version: PackageVersion,
    pinned: PinnedState,
    dependencies: Vec<LocalPackageId>,
    constraint: Option<String>,
    binaries: RockBinaries,
    source: RemotePackageSource,
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
    pub(crate) fn from(
        package: &PackageSpec,
        constraint: LockConstraint,
        binaries: RockBinaries,
        source: RemotePackageSource,
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Lockfile<P: LockfilePermissions> {
    #[serde(skip)]
    filepath: PathBuf,
    #[serde(skip)]
    _marker: PhantomData<P>,
    // TODO: Serialize this directly into a `Version`
    version: String,
    // NOTE: We cannot directly serialize to a `Sha256` object as they don't implement serde traits.
    rocks: HashMap<LocalPackageId, LocalPackage>,
    entrypoints: HashSet<LocalPackageId>,
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

impl<P: LockfilePermissions> Lockfile<P> {
    pub fn version(&self) -> &String {
        &self.version
    }

    pub fn rocks(&self) -> &HashMap<LocalPackageId, LocalPackage> {
        &self.rocks
    }

    pub fn get(&self, id: &LocalPackageId) -> Option<&LocalPackage> {
        self.rocks.get(id)
    }

    pub(crate) fn list(&self) -> HashMap<PackageName, Vec<LocalPackage>> {
        self.rocks()
            .values()
            .cloned()
            .map(|locked_rock| (locked_rock.name().clone(), locked_rock))
            .into_group_map()
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

    /// Find all rocks that match the requirement
    pub(crate) fn find_rocks(&self, req: &PackageReq) -> Vec<LocalPackageId> {
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

    /// Synchronise a list of packages with this lockfile,
    /// producing a report of packages to add and packages to remove.
    ///
    /// NOTE: The reason we produce a report and don't add/remove packages
    /// here is because packages need to be installed in order to be added.
    pub(crate) fn package_sync_spec(&self, packages: &[PackageReq]) -> PackageSyncSpec {
        let (entrypoints_to_keep, entrypoints_to_remove): (Vec<LocalPackage>, Vec<LocalPackage>) =
            self.entrypoints
                .iter()
                .map(|id| {
                    self.get(id)
                        .expect("entrypoint not found in malformed lockfile.")
                })
                .cloned()
                .partition(|local_pkg| {
                    packages
                        .iter()
                        .any(|req| req.matches(&local_pkg.as_package_spec()))
                });

        let packages_to_keep: HashSet<&LocalPackage> = entrypoints_to_keep
            .iter()
            .flat_map(|local_pkg| self.get_all_dependencies(&local_pkg.id()))
            .collect();

        let to_add = packages
            .iter()
            .filter(|pkg| self.has_rock(pkg, None).is_none())
            .cloned()
            .collect_vec();

        let to_remove = entrypoints_to_remove
            .iter()
            .flat_map(|local_pkg| self.get_all_dependencies(&local_pkg.id()))
            .filter(|dependency| !packages_to_keep.contains(dependency))
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

    /// Creates a temporary, writeable lockfile which can never flush.
    pub fn into_temporary(self) -> Lockfile<ReadWrite> {
        Lockfile::<ReadWrite> {
            _marker: PhantomData,
            filepath: self.filepath,
            version: self.version,
            rocks: self.rocks,
            entrypoints: self.entrypoints,
        }
    }

    /// Creates a lockfile guard, flushing the lockfile automatically
    /// once the guard goes out of scope.
    pub fn write_guard(self) -> LockfileGuard {
        LockfileGuard(self.into_temporary())
    }

    /// Converts the current lockfile into a writeable one, executes `cb` and flushes
    /// the lockfile.
    pub fn map_then_flush<T, F, E>(&mut self, cb: F) -> Result<T, E>
    where
        F: FnOnce(&mut Lockfile<ReadWrite>) -> Result<T, E>,
        E: Error,
        E: From<io::Error>,
    {
        let mut writeable_lockfile = self.clone().into_temporary();

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

impl Lockfile<ReadWrite> {
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

    pub(crate) fn remove(&mut self, target: &LocalPackage) {
        self.remove_by_id(&target.id())
    }

    pub(crate) fn remove_by_id(&mut self, target: &LocalPackageId) {
        self.rocks.remove(target);
        self.entrypoints.remove(target);
    }

    pub(crate) fn sync_lockfile(&mut self, other: &Lockfile<ReadOnly>) {
        self.rocks = other.rocks.clone();
        self.entrypoints = other.entrypoints.clone();
    }

    // TODO: `fn entrypoints() -> Vec<LockedRock>`
}

pub struct LockfileGuard(Lockfile<ReadWrite>);

impl Serialize for LockfileGuard {
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

impl Deref for LockfileGuard {
    type Target = Lockfile<ReadWrite>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for LockfileGuard {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Drop for LockfileGuard {
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

        let _ = tree.lockfile().unwrap().write_guard(); // Try to create the lockfile but don't actually do anything with it.
    }

    fn get_test_lockfile() -> Lockfile<ReadOnly> {
        let sample_tree = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources/test/sample-tree/5.1/lock.json");
        Lockfile::new(sample_tree).unwrap()
    }

    #[test]
    fn test_sync_spec() {
        let lockfile = get_test_lockfile();
        let packages = vec![
            PackageReq::parse("neorg@8.8.1-1").unwrap(),
            PackageReq::parse("lua-cjson@2.1.0-1").unwrap(),
            PackageReq::parse("nonexistent").unwrap(),
        ];

        let sync_spec = lockfile.package_sync_spec(&packages);

        assert_eq!(sync_spec.to_add.len(), 1);

        // Should remove:
        // - say (not requested)
        // - neorg 8.0.0-1 (older version)
        // - dependencies unique to neorg 8.0.0-1
        assert!(sync_spec
            .to_remove
            .iter()
            .any(|pkg| pkg.name().to_string() == "say"
                && pkg.version() == &"1.4.1-3".parse().unwrap()));
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
        let sync_spec = lockfile.package_sync_spec(&packages);

        // Should remove all packages
        assert!(sync_spec.to_add.is_empty());
        assert_eq!(sync_spec.to_remove.len(), lockfile.rocks().len());
    }
}
