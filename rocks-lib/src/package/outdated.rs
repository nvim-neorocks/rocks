use std::fmt::Display;

use thiserror::Error;

use crate::manifest::ManifestMetadata;

use super::{version::PackageVersion, PackageName, PackageReq, PackageVersionReq, RemotePackage};

#[derive(Error, Debug)]
#[error("rock {0} not found")]
pub struct RockNotFound(PackageName);

#[derive(Error, Debug)]
#[error("rock {name} has no version that satisfies constraint {constraint}")]
pub struct RockConstraintUnsatisfied {
    name: PackageName,
    constraint: PackageVersionReq,
}

impl RemotePackage {
    /// Tries to find a newer version of a given rock.
    /// Returns the latest version if found.
    pub fn has_update(
        &self,
        manifest: &ManifestMetadata,
    ) -> Result<Option<PackageVersion>, RockNotFound> {
        let latest_version = manifest
            .latest_version(&self.name)
            .ok_or_else(|| RockNotFound(self.name.clone()))?;

        if self.version < *latest_version {
            Ok(Some(latest_version.to_owned()))
        } else {
            Ok(None)
        }
    }

    /// Tries to find a newer version of a rock given a constraint.
    /// Returns the latest version if found.
    pub fn has_update_with(
        &self,
        constraint: &PackageReq,
        manifest: &ManifestMetadata,
    ) -> Result<Option<PackageVersion>, RockConstraintUnsatisfied> {
        let latest_version =
            manifest
                .latest_match(constraint)
                .ok_or_else(|| RockConstraintUnsatisfied {
                    name: self.name.clone(),
                    constraint: constraint.version_req.clone(),
                })?;

        if self.version < latest_version.version {
            Ok(Some(latest_version.version))
        } else {
            Ok(None)
        }
    }
}

impl Display for RemotePackage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(format!("{} {}", self.name, self.version).as_str())
    }
}

#[cfg(test)]
mod test {
    use std::path::PathBuf;

    use crate::{manifest::ManifestMetadata, package::RemotePackage};

    #[test]
    fn rock_has_update() {
        let test_manifest_path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/test/manifest");
        let manifest = String::from_utf8(std::fs::read(&test_manifest_path).unwrap()).unwrap();
        let manifest = ManifestMetadata::new(&manifest).unwrap();

        let test_package =
            RemotePackage::parse("lua-cjson".to_string(), "2.0.0".to_string()).unwrap();

        assert_eq!(
            test_package.has_update(&manifest).unwrap(),
            Some("2.1.0-1".parse().unwrap())
        );
    }
}
