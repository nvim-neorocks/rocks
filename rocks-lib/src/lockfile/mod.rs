use std::io::{self, Write};
use std::{collections::HashMap, fs::File, io::ErrorKind, path::PathBuf};

use itertools::Itertools;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ssri::Integrity;

use crate::package::{PackageName, PackageReq, PackageVersion, PackageVersionReq, RemotePackage};

#[derive(Copy, Debug, PartialEq, Eq, Clone, PartialOrd, Ord)]
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
    fn as_bool(&self) -> bool {
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

// TODO(vhyrro): Move to `package/local.rs`
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct LocalPackage {
    pub name: PackageName,
    pub version: PackageVersion,
    pub pinned: PinnedState,
    pub dependencies: Vec<String>,
    // TODO: Deserialize this directly into a `LuaPackageReq`
    pub constraint: Option<String>,
    pub hashes: LocalPackageHashes,
}

impl LocalPackage {
    pub fn from(
        package: &RemotePackage,
        constraint: LockConstraint,
        hashes: LocalPackageHashes,
    ) -> Self {
        Self {
            name: package.name().clone(),
            version: package.version().clone(),
            pinned: PinnedState::Unpinned,
            dependencies: Vec::default(),
            constraint: match constraint {
                LockConstraint::Unconstrained => None,
                LockConstraint::Constrained(version_req) => Some(version_req.to_string()),
            },
            hashes,
        }
    }

    pub fn id(&self) -> String {
        let mut hasher = Sha256::new();

        hasher.update(format!(
            "{}{}{}{}",
            self.name,
            self.version,
            self.pinned.as_bool(),
            self.constraint.clone().unwrap_or_default()
        ));

        hex::encode(hasher.finalize())
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

    pub fn dependencies(&self) -> Vec<&String> {
        self.dependencies.iter().collect()
    }

    pub fn constraint(&self) -> LockConstraint {
        match &self.constraint {
            // Safe to unwrap as the data can only end up in the struct as a valid constraint
            Some(constraint) => LockConstraint::Constrained(
                constraint
                    .parse()
                    .expect("invalid constraint in LuaPackage"),
            ),
            None => LockConstraint::Unconstrained,
        }
    }

    pub fn to_package(&self) -> RemotePackage {
        RemotePackage::new(self.name.clone(), self.version.clone())
    }

    pub fn into_package_req(self) -> PackageReq {
        RemotePackage::new(self.name, self.version).into_package_req()
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

#[derive(Clone)]
pub enum LockConstraint {
    Unconstrained,
    Constrained(PackageVersionReq),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Lockfile {
    #[serde(skip)]
    filepath: PathBuf,
    // TODO: Serialize this directly into a `Version`
    version: String,
    // NOTE: We cannot directly serialize to a `Sha256` object as they don't implement serde traits.
    rocks: HashMap<String, LocalPackage>,
    entrypoints: Vec<String>,
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
            .and_modify(|rock| rock.dependencies.push(dependency_id));

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

    pub fn rocks(&self) -> &HashMap<String, LocalPackage> {
        &self.rocks
    }

    pub fn get(&self, id: &str) -> Option<&LocalPackage> {
        self.rocks.get(id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut LocalPackage> {
        self.rocks.get_mut(id)
    }

    // TODO: `fn entrypoints() -> Vec<LockedRock>`

    pub fn flush(&mut self) -> io::Result<()> {
        let dependencies = self
            .rocks
            .iter()
            .flat_map(|(_, rock)| &rock.dependencies)
            .collect_vec();

        self.entrypoints = self
            .rocks
            .keys()
            .filter(|id| !dependencies.iter().contains(id))
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
        test_local_dep_package.pinned = PinnedState::Pinned;
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
