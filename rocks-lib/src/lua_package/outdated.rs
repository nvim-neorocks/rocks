use eyre::{eyre, Result};

use crate::manifest::ManifestMetadata;

use super::{version::PackageVersion, LuaPackage};

impl LuaPackage {
    /// Tries to find a newer version of a given rock.
    /// Returns the latest version if found.
    pub fn has_update(&self, manifest: &ManifestMetadata) -> Result<Option<PackageVersion>> {
        let latest_version: PackageVersion = manifest
            .latest_version(&self.name)
            .ok_or(eyre!("rock {} not found!", self.name))?
            .parse()?;

        if self.version < latest_version {
            Ok(Some(latest_version))
        } else {
            Ok(None)
        }
    }
}
