use std::fmt::Display;

use eyre::{eyre, Result};

use crate::manifest::ManifestMetadata;

use super::{version::PackageVersion, LuaPackage};

impl LuaPackage {
    /// Tries to find a newer version of a given rock.
    /// Returns the latest version if found.
    pub fn has_update(&self, manifest: &ManifestMetadata) -> Result<Option<PackageVersion>> {
        let latest_version = manifest
            .latest_version(&self.name)
            .ok_or(eyre!("rock {} not found!", self.name))?;

        if self.version < *latest_version {
            Ok(Some(latest_version.to_owned()))
        } else {
            Ok(None)
        }
    }
}

impl Display for LuaPackage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(format!("{} {}", self.name, self.version).as_str())
    }
}

#[cfg(test)]
mod test {
    use std::path::PathBuf;

    use crate::{lua_package::LuaPackage, manifest::ManifestMetadata};

    #[test]
    fn rock_has_update() {
        let test_manifest_path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/test/manifest");
        let manifest = String::from_utf8(std::fs::read(&test_manifest_path).unwrap()).unwrap();
        let manifest = ManifestMetadata::new(&manifest).unwrap();

        let test_package = LuaPackage::parse("lua-cjson".to_string(), "2.0.0".to_string()).unwrap();

        assert_eq!(
            test_package.has_update(&manifest).unwrap(),
            Some("2.1.0-1".parse().unwrap())
        );
    }
}
