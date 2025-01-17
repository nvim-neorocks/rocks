use std::{
    collections::HashMap,
    io::{self, Cursor},
    path::{Path, PathBuf},
};

use bytes::Bytes;
use tempdir::TempDir;
use thiserror::Error;

use crate::rockspec::LuaVersionCompatibility;
use crate::{
    build::{
        external_dependency::{ExternalDependencyError, ExternalDependencyInfo},
        BuildBehaviour,
    },
    config::Config,
    hash::HasIntegrity,
    lockfile::{LocalPackage, LocalPackageHashes, LockConstraint, PinnedState},
    lua_rockspec::{LuaRockspec, LuaVersionError},
    luarocks::rock_manifest::RockManifest,
    package::PackageSpec,
    progress::{Progress, ProgressBar},
    remote_package_source::RemotePackageSource,
    rockspec::Rockspec,
    tree::Tree,
};

use super::rock_manifest::RockManifestError;

#[derive(Error, Debug)]
pub enum InstallBinaryRockError {
    #[error("IO operation failed: {0}")]
    Io(#[from] io::Error),
    #[error(transparent)]
    ExternalDependencyError(#[from] ExternalDependencyError),
    #[error(transparent)]
    LuaVersionError(#[from] LuaVersionError),
    #[error("failed to unpack packed rock: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("rock_manifest not found. Cannot install rock files that were packed using LuaRocks version 1")]
    RockManifestNotFound,
    #[error(transparent)]
    RockManifestError(#[from] RockManifestError),
}

pub(crate) struct BinaryRockInstall<'a> {
    rockspec: &'a LuaRockspec,
    rock_bytes: Bytes,
    source: RemotePackageSource,
    pin: PinnedState,
    constraint: LockConstraint,
    behaviour: BuildBehaviour,
    config: &'a Config,
    progress: &'a Progress<ProgressBar>,
}

impl<'a> BinaryRockInstall<'a> {
    pub(crate) fn new(
        rockspec: &'a LuaRockspec,
        source: RemotePackageSource,
        rock_bytes: Bytes,
        config: &'a Config,
        progress: &'a Progress<ProgressBar>,
    ) -> Self {
        Self {
            rockspec,
            rock_bytes,
            source,
            config,
            progress,
            constraint: LockConstraint::default(),
            behaviour: BuildBehaviour::default(),
            pin: PinnedState::default(),
        }
    }

    pub(crate) fn pin(self, pin: PinnedState) -> Self {
        Self { pin, ..self }
    }

    pub(crate) fn constraint(self, constraint: LockConstraint) -> Self {
        Self { constraint, ..self }
    }

    pub(crate) fn behaviour(self, behaviour: BuildBehaviour) -> Self {
        Self { behaviour, ..self }
    }

    pub(crate) async fn install(self) -> Result<LocalPackage, InstallBinaryRockError> {
        let rockspec = self.rockspec;
        self.progress.map(|p| {
            p.set_message(format!(
                "Unpacking and installing {}@{}...",
                rockspec.package(),
                rockspec.version()
            ))
        });
        for (name, dep) in rockspec.external_dependencies().current_platform() {
            let _ = ExternalDependencyInfo::detect(name, dep, self.config.external_deps())?;
        }

        let lua_version = rockspec.lua_version_matches(self.config)?;

        let tree = Tree::new(self.config.tree().clone(), lua_version.clone())?;

        let hashes = LocalPackageHashes {
            rockspec: rockspec.hash()?,
            source: self.rock_bytes.hash()?,
        };
        let mut package = LocalPackage::from(
            &PackageSpec::new(rockspec.package().clone(), rockspec.version().clone()),
            self.constraint,
            rockspec.binaries(),
            self.source,
            hashes,
        );
        package.spec.pinned = self.pin;
        match tree.lockfile()?.get(&package.id()) {
            Some(package) if self.behaviour == BuildBehaviour::NoForce => Ok(package.clone()),
            _ => {
                let unpack_dir = TempDir::new("rocks-bin-rock").unwrap().into_path();
                let cursor = Cursor::new(self.rock_bytes);
                let mut zip = zip::ZipArchive::new(cursor)?;
                zip.extract(&unpack_dir)?;
                let rock_manifest_file = unpack_dir.join("rock_manifest");
                if !rock_manifest_file.is_file() {
                    return Err(InstallBinaryRockError::RockManifestNotFound);
                }
                let rock_manifest_content = std::fs::read_to_string(rock_manifest_file)?;
                let output_paths = tree.rock(&package)?;
                let rock_manifest = RockManifest::new(&rock_manifest_content)?;
                install_manifest_entry(
                    &rock_manifest.lib,
                    &unpack_dir.join("lib"),
                    &output_paths.lib,
                )?;
                install_manifest_entry(
                    &rock_manifest.lua,
                    &unpack_dir.join("lua"),
                    &output_paths.src,
                )?;
                install_manifest_entry(
                    &rock_manifest.bin,
                    &unpack_dir.join("bin"),
                    &output_paths.bin,
                )?;
                install_manifest_entry(
                    &rock_manifest.doc,
                    &unpack_dir.join("doc"),
                    &output_paths.doc,
                )?;
                install_manifest_entry(&rock_manifest.root, &unpack_dir, &output_paths.etc)?;
                Ok(package)
            }
        }
    }
}

fn install_manifest_entry(
    entry: &HashMap<PathBuf, String>,
    src: &Path,
    dest: &Path,
) -> Result<(), InstallBinaryRockError> {
    for relative_src_path in entry.keys() {
        let target = dest.join(relative_src_path);
        std::fs::create_dir_all(target.parent().unwrap())?;
        std::fs::copy(src.join(relative_src_path), target)?;
    }
    Ok(())
}

#[cfg(test)]
mod test {

    use crate::{
        config::ConfigBuilder,
        operations::{unpack_rockspec, DownloadedPackedRockBytes},
        progress::MultiProgress,
    };

    use super::*;
    #[tokio::test]
    async fn install_binary_rock() {
        if std::env::var("ROCKS_SKIP_IMPURE_TESTS").unwrap_or("0".into()) == "1" {
            println!("Skipping impure test");
            return;
        }
        let content = std::fs::read("resources/test/toml-edit-0.6.0-1.linux-x86_64.rock").unwrap();
        let rock_bytes = Bytes::copy_from_slice(&content);
        let rock = DownloadedPackedRockBytes {
            name: "toml-edit".into(),
            version: "0.6.0-1".parse().unwrap(),
            bytes: rock_bytes,
            file_name: "toml-edit-0.6.0-1.linux-x86_64.rock".into(),
        };
        let rockspec = unpack_rockspec(&rock).await.unwrap();
        let dir = assert_fs::TempDir::new().unwrap();
        let config = ConfigBuilder::new()
            .unwrap()
            .tree(Some(dir.to_path_buf()))
            .build()
            .unwrap();
        let progress = MultiProgress::new();
        let bar = progress.new_bar();
        BinaryRockInstall::new(
            &rockspec,
            RemotePackageSource::Test,
            rock.bytes,
            &config,
            &Progress::Progress(bar),
        )
        .install()
        .await
        .unwrap();
    }
}
